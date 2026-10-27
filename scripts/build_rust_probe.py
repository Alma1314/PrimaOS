#!/usr/bin/env python3
"""Build Prima core/alloc and the explicitly restricted single-thread std PAL.

Only an ignored copy of the pinned rust-src is patched. No installed toolchain
files are modified, no dependency is downloaded, and Linux/wasm cfgs are not
impersonated. This PAL provides native stdout/stderr, monotonic Instant and
sleep/yield. Applications supply entry hooks and an allocator; other OS APIs
retain the explicitly documented unsupported scope. System fails with OOM.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def prepare_sysroot():
    source_root = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    version = subprocess.check_output(["rustc", "-Vv"], text=True)
    if "1a833e165" not in version:
        raise ValueError("Prima std overlay requires the repository's pinned rustc 1a833e165")
    library = source_root / "lib/rustlib/src/rust/library"
    files = ["std/src/sys/alloc/mod.rs", "std/src/sys/io/error/mod.rs",
             "std/src/sys/thread_local/mod.rs", "std/src/sys/random/mod.rs",
             "std/src/sys/stdio/mod.rs", "std/src/sys/time/mod.rs",
             "std/src/sys/thread/mod.rs", "std/src/sys/time/unsupported.rs",
             "std/src/sys/thread/unsupported.rs", "std/src/thread/lifecycle.rs"]
    hashes = {name: hashlib.sha256((library / name).read_bytes()).hexdigest() for name in files}
    lock = json.loads((ROOT / "prima-user/rust-probe/rust-src.lock.json").read_text())
    if hashes != lock["upstream_sha256"] or lock["compiler_commit"] not in version:
        raise ValueError("pinned rust-src patch inputs differ from rust-src.lock.json")
    pal_sources = {name: ROOT / f"prima-user/rust-probe/pal/{name}.rs"
                   for name in ("stdio", "time", "thread")}
    overlay_hashes = {name: hashlib.sha256(path.read_bytes()).hexdigest()
                      for name, path in pal_sources.items()}
    digest = hashlib.sha256(Path(__file__).read_bytes() + version.encode()
                            + json.dumps([hashes, overlay_hashes], sort_keys=True).encode()).hexdigest()
    destination = ROOT / "target/prima-rust-sysroot"
    stamp = destination / ".stamp"
    copied = destination / "lib/rustlib/src/rust/library"
    if not stamp.exists() or stamp.read_text() != digest:
        if copied.exists():
            shutil.rmtree(copied)
        copied.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(library, copied, symlinks=True)
        # Explicit native modules; unsupported operations retain upstream PAL.
        # No Unix identity or libc ABI is introduced.
        replacements = {
            files[0]: ('cfg_select! {\n    any(\n        target_family = "unix",', 'cfg_select! {\n    target_vendor = "prima" => { mod prima; use prima as imp; }\n    any(\n        target_family = "unix",'),
            files[1]: ("cfg_select! {", 'cfg_select! {\n    target_vendor = "prima" => { mod generic; pub use generic::*; }'),
            files[2]: ('cfg_select! {\n    any(\n        all(target_family = "wasm", not(target_feature = "atomics"), not(target_os = "wasi")),', 'cfg_select! {\n    any(\n        target_vendor = "prima",\n        all(target_family = "wasm", not(target_feature = "atomics"), not(target_os = "wasi")),'),
            files[3]: ('        target_os = "vexos",', '        target_vendor = "prima",\n        target_os = "vexos",'),
            files[4]: ('cfg_select! {', 'cfg_select! {\n    target_vendor = "prima" => { mod prima; pub use prima::*; }'),
            files[5]: ('cfg_select! {', 'cfg_select! {\n    target_vendor = "prima" => {\n        mod prima; use prima as imp;\n        #[expect(dead_code)] mod unsupported;\n    }'),
            files[6]: ('cfg_select! {', 'cfg_select! {\n    target_vendor = "prima" => {\n        mod prima; pub use prima::{sleep, yield_now};\n        #[expect(dead_code)] mod unsupported;\n        pub use unsupported::{Thread, available_parallelism, current_os_id, set_name, DEFAULT_MIN_STACK_SIZE};\n    }'),
            # Reject before Thread::new uses System for its internal handle.
            files[9]: ('{\n    let stack_size = stack_size.unwrap_or_else', '{\n    if cfg!(target_vendor = "prima") {\n        return Err(io::Error::UNSUPPORTED_PLATFORM);\n    }\n    let stack_size = stack_size.unwrap_or_else'),
        }
        for name, (before, after) in replacements.items():
            path = copied / name
            content = path.read_text()
            if content.count(before) != 1:
                raise ValueError(f"rust-src patch anchor changed: {name}")
            path.write_text(content.replace(before, after, 1))
        for name, before, after in [
            (files[2], '        any(\n            all(target_family = "wasm", not(target_os = "wasi")),',
             '        any(\n            target_vendor = "prima",\n            all(target_family = "wasm", not(target_os = "wasi")),'),
            (files[3], '#[cfg(not(any(\n', '#[cfg(not(any(\n    target_vendor = "prima",\n'),
        ]:
            path = copied / name
            content = path.read_text()
            if content.count(before) != 1:
                raise ValueError(f"rust-src patch anchor changed: {name}")
            path.write_text(content.replace(before, after, 1))
        (copied / "std/src/sys/alloc/prima.rs").write_text('''// Prima restricted PAL: use an application global allocator.
use crate::alloc::Layout;
pub unsafe fn alloc(_: Layout) -> *mut u8 { crate::ptr::null_mut() }
pub unsafe fn alloc_zeroed(_: Layout) -> *mut u8 { crate::ptr::null_mut() }
pub unsafe fn dealloc(_: *mut u8, _: Layout) {}
pub unsafe fn realloc(_: *mut u8, _: Layout, _: usize) -> *mut u8 { crate::ptr::null_mut() }
''')
        for name, path in pal_sources.items():
            shutil.copyfile(path, copied / f"std/src/sys/{name}/prima.rs")
        for name in (source_root / "lib/rustlib").iterdir():
            if name.name == "src":
                continue
            link = destination / "lib/rustlib" / name.name
            if not link.exists():
                link.symlink_to(name, target_is_directory=name.is_dir())
        (destination / "source-provenance.json").write_text(json.dumps({
            "compiler": version, "source_root": str(library), "upstream_sha256": hashes,
            "overlay_sha256": digest, "license": "MIT OR Apache-2.0",
            "prima_pal_sha256": overlay_hashes,
        }, indent=2) + "\n")
        stamp.write_text(digest)
    return source_root / "bin/rustc", destination, digest


def main():
    flags = set(sys.argv[1:])
    if not flags <= {"--std", "--clippy"} or len(flags) != len(sys.argv[1:]):
        raise ValueError("usage: build_rust_probe.py [--std] [--clippy]")
    standard, clippy = "--std" in flags, "--clippy" in flags
    compiler, sysroot, overlay_digest = prepare_sysroot()
    env = os.environ.copy()
    env.update(RUSTC=str(ROOT / "scripts/rustc_prima.py"),
               PRIMA_REAL_RUSTC=str(compiler), PRIMA_RUST_SYSROOT=str(sysroot))
    # Cargo treats sysroot sources as immutable and can otherwise reuse an old
    # build-std rlib after a local overlay changes. Make its input hash part of
    # every compilation fingerprint, preserving caller-provided flags.
    rustflags = (env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
                 if env.get("CARGO_ENCODED_RUSTFLAGS") else env.get("RUSTFLAGS", "").split())
    rustflags += ["--cfg", f'prima_overlay_hash="{overlay_digest}"']
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(rustflags)
    if clippy:
        # Clippy recognizes a compiler wrapper by its rustc basename. Its own
        # driver must also query this sysroot, without cached installed paths.
        shim = sysroot / "bin/rustc"
        shim.parent.mkdir(exist_ok=True)
        if not shim.exists():
            shim.symlink_to(ROOT / "scripts/rustc_prima.py")
        env.update(RUSTC=str(shim), SYSROOT=str(sysroot), CARGO_CACHE_RUSTC_INFO="0")
    target_dir = ROOT / ("target/rust-clippy" if clippy else "target/rust-probe")
    command = ["cargo", "clippy" if clippy else "build", "--release", "--offline", "--locked", "--manifest-path",
               str(ROOT / "prima-user/rust-probe/Cargo.toml"), "--target",
               str(ROOT / "targets/x86_64-prima.json"), "--target-dir", str(target_dir),
               "-Zjson-target-spec", "-Zbuild-std-features=compiler-builtins-mem",
               "-Zbuild-std=" + ("std,panic_abort" if standard else "core,alloc,panic_abort")]
    if standard:
        command += ["--features", "std"]
    if clippy:
        command += ["--no-deps", "--", "-D", "warnings"]
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    if clippy:
        print("PASS Rust probe Clippy: " + ("restricted std" if standard else "core/alloc"))


if __name__ == "__main__":
    main()
