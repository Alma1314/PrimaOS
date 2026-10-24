#!/usr/bin/env python3
"""Build the locked static musl port; no downloads or host libc inputs."""
import argparse
import fcntl
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CFLAGS = "-O2 -fno-stack-protector -fno-omit-frame-pointer -fPIC"
CC = "clang --target=x86_64-unknown-none"


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def verify_snapshot(snapshot, lock):
    if not snapshot.is_dir():
        raise ValueError(f"musl snapshot missing: {snapshot}; restore from musl-snapshot.lock.toml")
    if Path(output("git", "-C", str(snapshot), "rev-parse", "--show-toplevel")).resolve() != snapshot.resolve():
        raise ValueError("musl snapshot must be its own Git checkout")
    if output("git", "-C", str(snapshot), "rev-parse", "HEAD") != lock["commit"]:
        raise ValueError("musl snapshot commit differs from lock")
    if output("git", "-C", str(snapshot), "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("musl snapshot is dirty (including untracked files)")
    for filename, field in [(lock["license_file"], "license_sha256"), ("VERSION", "version_file_sha256")]:
        if hashlib.sha256((snapshot / filename).read_bytes()).hexdigest() != lock[field]:
            raise ValueError(f"musl snapshot {filename} hash differs from lock")


def export_snapshot(snapshot, work):
    # Copy only tracked sources: ignored obj/config.mak from a local upstream
    # build must neither poison the cross build nor require deleting user data.
    work.mkdir()
    for name in output("git", "-C", str(snapshot), "ls-files", "-z").split("\0"):
        if name:
            destination = work / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(snapshot / name, destination, follow_symlinks=False)


def cache_key(root, lock, tools):
    digest = hashlib.sha256()
    for value in [str(root.resolve()), CC, CFLAGS, *tools]:
        digest.update(value.encode() + b"\0")
    for path in [HERE / "build.py", HERE / "build.sh", root / "docs/superpowers/compat/musl-snapshot.lock.toml"]:
        digest.update(path.read_bytes())
    for target in lock["patches"]:
        source = root / lock["patch_dir"] / Path(target).name
        digest.update(target.encode() + b"\0" + source.read_bytes())
    return digest.hexdigest()


def cache_valid(prefix, stamp):
    required = ["lib/libc.a", "include/stdio.h", "include/pthread.h", "include/bits/syscall.h"]
    return ((prefix / ".stamp").is_file()
            and (prefix / ".stamp").read_text().strip() == stamp
            and all((prefix / name).is_file() and (prefix / name).stat().st_size > 0 for name in required))


def audit_disassembly(disassembly):
    """Only the port's native clone/exit and signal restore may trap directly."""
    calls = {}
    member, number = "", None
    for line in disassembly.splitlines():
        if ":     file format " in line:
            member, number = line.split(":", 1)[0], None
        match = re.search(r"mov\s+\$(0x[0-9a-f]+|[0-9]+),%[er]?a[xl]\b", line)
        if match:
            number = int(match[1], 0)
        if re.search(r"\b(sysenter|int\s+\$0x80)\s*$", line):
            raise ValueError(f"unexpected trap in musl {member}")
        if re.search(r"\bsyscall\s*$", line):
            calls.setdefault(member, []).append(number)
    if calls != {"restore.lo": [58], "clone.lo": [48, 49]}:
        raise ValueError(f"unexpected raw syscall boundary in musl: {calls}")


def audit_archive(prefix):
    audit_disassembly(output("objdump", "-d", str(prefix / "lib/libc.a")))


def build(check=False):
    lock = tomllib.loads((ROOT / "docs/superpowers/compat/musl-snapshot.lock.toml").read_text())
    snapshot = ROOT / lock["snapshot_dir"]
    jobs = os.environ.get("PRIMA_MUSL_JOBS", "2")
    if not jobs.isascii() or not jobs.isdecimal() or int(jobs) < 1:
        raise ValueError("PRIMA_MUSL_JOBS must be a positive integer")
    (ROOT / "target").mkdir(exist_ok=True)
    # Serialize concurrent xtask builds; never remove another builder's files.
    with (ROOT / "target/.musl-build.lock").open("w") as mutex:
        fcntl.flock(mutex, fcntl.LOCK_EX)
        verify_snapshot(snapshot, lock)  # Validate even on a cache hit.
        tools = [output("clang", "--version"), output("llvm-ar", "--version"), output("objdump", "--version")]
        stamp = cache_key(ROOT, lock, tools)
        prefix = ROOT / lock["sysroot_dir"]
        if check:
            if cache_valid(prefix, stamp):
                audit_archive(prefix)
            print(f"PASS musl locked snapshot; cache={'hit' if cache_valid(prefix, stamp) else 'miss'}")
            return
        if cache_valid(prefix, stamp):
            return
        work, build_dir = ROOT / "target/musl-prima", ROOT / "target/musl-build"
        # Invalidate before rebuilding; an interrupted install is never a cache hit.
        for path in [work, build_dir, prefix]:
            if path.exists():
                shutil.rmtree(path)
        export_snapshot(snapshot, work)
        for target in lock["patches"]:
            shutil.copyfile(ROOT / lock["patch_dir"] / Path(target).name, work / target)
        build_dir.mkdir()
        env = os.environ.copy()
        for key in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "LIBS", "CPATH", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH", "LIBRARY_PATH", "CROSS_COMPILE", "MAKEFLAGS", "MAKEFILES", "MFLAGS", "CONFIG_SITE"]:
            env.pop(key, None)
        env.update(CC=CC, CFLAGS=CFLAGS, AR="llvm-ar", RANLIB="llvm-ar s")
        subprocess.run([str(work / "configure"), "--target=x86_64", "--disable-shared", "--enable-static", f"--prefix={prefix}"], cwd=build_dir, env=env, check=True)
        for args in [[f"-j{jobs}"], ["install"]]:
            subprocess.run(["make", *args, "AR=llvm-ar", "RANLIB=llvm-ar s"], cwd=build_dir, env=env, check=True)
        audit_archive(prefix)
        (prefix / ".stamp").write_text(stamp + "\n")
        if not cache_valid(prefix, stamp):
            (prefix / ".stamp").unlink()
            raise ValueError("musl install incomplete")
        print(f"PASS musl static sysroot (jobs={jobs})")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify locked inputs and cache without building")
    args = parser.parse_args()
    try:
        build(args.check)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"ERROR musl: {error}")
