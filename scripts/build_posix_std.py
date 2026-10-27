#!/usr/bin/env python3
"""Build the Prima POSIX std target using the pinned Unix/musl ABI sources.

The target is os=prima, not Linux. Explicit cfg aliases select Linux/musl ABI
bindings in isolated source copies, because the existing persona implements
that userspace ABI. Every trap still goes through prima_syscall.c. Installed
Rust and registry sources are never edited. No downloads are performed.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
LIBC_VERSION = 'libc-0.2.189'


def inherited_rustflags(env):
    if 'CARGO_ENCODED_RUSTFLAGS' in env:
        encoded = env['CARGO_ENCODED_RUSTFLAGS']
        return encoded.split('\x1f') if encoded else []
    return shlex.split(env.get('RUSTFLAGS', ''))


def tree_hash(root):
    digest = hashlib.sha256()
    for path in sorted(root.rglob('*')):
        if path.is_file():
            digest.update(path.relative_to(root).as_posix().encode() + b'\0' + path.read_bytes())
    return digest.hexdigest()


def replace(path, before, after):
    content = path.read_text()
    if content.count(before) != 1:
        raise ValueError(f'Prima POSIX patch anchor changed: {path}: {before!r}')
    path.write_text(content.replace(before, after))


def prepare():
    version = subprocess.check_output(['rustc', '-Vv'], text=True)
    source_root = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    library = source_root / 'lib/rustlib/src/rust/library'
    # Use only the version already locked by the pinned Rust library.
    cargo_root = Path(os.environ.get('CARGO_HOME', str(Path.home() / '.cargo')))
    candidates = sorted((cargo_root / 'registry/src').glob('*/' + LIBC_VERSION))
    if not candidates:
        raise ValueError('locked libc 0.2.189 source missing from the Cargo cache')
    libc = candidates[0]
    lock = json.loads((ROOT / 'prima-user/rust-probe/posix-src.lock.json').read_text())
    hashes = {'std': tree_hash(library / 'std'), 'unwind': tree_hash(library / 'unwind'), 'backtrace': tree_hash(library / 'backtrace'), 'libc': tree_hash(libc)}
    if lock['compiler_commit'] not in version or hashes != lock['upstream_sha256']:
        raise ValueError('Prima POSIX pinned Rust/libc sources differ from posix-src.lock.json')
    digest = hashlib.sha256(Path(__file__).read_bytes() + (ROOT / 'scripts/rustc_prima.py').read_bytes() + json.dumps(hashes, sort_keys=True).encode()).hexdigest()
    sysroot = ROOT / 'target/prima-posix-sysroot'
    copied = sysroot / 'lib/rustlib/src/rust/library'
    stamp = sysroot / '.stamp'
    if not stamp.exists() or stamp.read_text() != digest:
        if copied.exists():
            shutil.rmtree(copied)
        copied.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(library, copied, symlinks=True)
        bindings = copied / 'prima-libc'
        shutil.copytree(libc, bindings)
        changed = []
        # Prima's adapter has Linux x86_64 musl layouts/errno/call numbers.
        # Select that ABI explicitly without injecting any built-in target cfg.
        for directory in [copied / 'std', bindings]:
            for path in sorted(directory.rglob('*.rs')):
                content = path.read_text()
                revised = content.replace('target_os = "linux"', 'any(target_os = "linux", target_os = "prima")')
                if revised != content:
                    path.write_text(revised)
                    changed.append(str(path.relative_to(copied)))
        # Prima's current socket ABI uses socket()+fcntl, and its futex
        # adapter accepts relative WAIT rather than Linux WAIT_BITSET.
        socket = copied / 'std/src/sys/net/connection/socket/unix.rs'
        socket.write_text(socket.read_text().replace(
            'any(target_os = "linux", target_os = "prima")', 'target_os = "linux"', 1))
        replace(copied / 'std/src/sys/pal/unix/futex.rs',
                '                target_os = "freebsd" => {',
                '''                target_os = "prima" => {
                    let relative = timespec.as_ref().map(|deadline| {
                        let deadline = Timespec::new(deadline.tv_sec, deadline.tv_nsec).unwrap();
                        deadline.sub_timespec(&Timespec::now(libc::CLOCK_MONOTONIC))
                            .unwrap_or(Duration::ZERO)
                    });
                    let relative = relative.map(|duration| libc::timespec {
                        tv_sec: duration.as_secs() as libc::time_t,
                        tv_nsec: duration.subsec_nanos() as libc::c_long,
                    });
                    libc::syscall(libc::SYS_futex, futex as *const Atomic<u32>,
                        libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG, expected,
                        relative.as_ref().map_or(null(), |t| t as *const libc::timespec))
                }
                target_os = "freebsd" => {''')
        # Use upstream's no-backtrace implementation: abort-only Prima has
        # no unwinder. Keep backtrace_in_libstd, which also controls imports.
        replace(copied / 'backtrace/src/backtrace/mod.rs',
                '                not(target_os = "emscripten"),',
                '                not(target_os = "emscripten"),\n                not(target_os = "prima"),')
        # This target is panic=abort only; do not request a system unwinder.
        unwind = copied / 'unwind/src/lib.rs'
        unwind.write_text(unwind.read_text().replace('target_env = "musl"',
            'all(target_env = "musl", not(target_os = "prima"))'))
        replace(copied / 'std/src/sys/thread/unix.rs',
                'pub const DEFAULT_MIN_STACK_SIZE: usize = 2 * 1024 * 1024;',
                'pub const DEFAULT_MIN_STACK_SIZE: usize = if cfg!(target_os = "prima") { 64 * 1024 } else { 2 * 1024 * 1024 };')
        replace(copied / 'std/build.rs', '    if target_os == "linux"',
                '    if target_os == "prima" || target_os == "linux"')
        # Build-std also treats host build scripts as immutable sysroot
        # inputs. Give the patched script an overlay-specific source path so
        # Cargo cannot reuse the old unknown-target/restricted-std decision.
        build_script = f'build_prima_{digest}.rs'
        shutil.copyfile(copied / 'std/build.rs', copied / 'std' / build_script)
        replace(copied / 'std/Cargo.toml', 'name = "std"\n',
                f'name = "std"\nbuild = "{build_script}"\n')
        # No gettid personality call yet: report no OS diagnostic ID rather
        # than casting its -1 error return into a fictitious u64 thread ID.
        replace(copied / 'std/src/sys/thread/unix.rs',
                'any(target_os = "android", any(target_os = "linux", target_os = "prima")) => {',
                'any(target_os = "android", target_os = "linux") => {')
        # Runtime initialization must not require unsupported poll/altstack.
        # Unix's normal fd checks and args initialization still run.
        replace(copied / 'std/src/sys/pal/unix/mod.rs',
                '#[cfg(target_vendor = "unikraft")]\n                    libc::ENOSYS',
                '#[cfg(any(target_vendor = "unikraft", target_os = "prima"))]\n                    libc::ENOSYS')
        # There is no sigaltstack or recoverable stack-overflow protocol yet.
        # Use the existing generic no-handler implementation on Prima.
        stack = copied / 'std/src/sys/pal/unix/stack_overflow.rs'
        stack.write_text((library / 'std/src/sys/pal/unix/stack_overflow.rs').read_text())
        # No entropy source exists in the current profile. Keep random bytes
        # explicitly unsupported; HashMap uses upstream's non-secure fallback.
        random = copied / 'std/src/sys/random/mod.rs'
        content = random.read_text()
        content = content.replace('cfg_select! {', 'cfg_select! {\n    target_os = "prima" => { mod unsupported; pub use unsupported::{fill_bytes, hashmap_random_keys}; }', 1)
        random.write_text(content)
        for path in (source_root / 'lib/rustlib').iterdir():
            if path.name != 'src':
                link = sysroot / 'lib/rustlib' / path.name
                if not link.exists():
                    link.symlink_to(path, target_is_directory=path.is_dir())
        (sysroot / 'source-provenance.json').write_text(json.dumps({
            'compiler': version, 'upstream_sha256': hashes, 'overlay_sha256': digest,
            'abi_alias_files': changed, 'license': 'MIT OR Apache-2.0',
        }, indent=2) + '\n')
        stamp.write_text(digest)
    return source_root / 'bin/rustc', sysroot, digest


def build():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--clippy', action='store_true')
    parser.add_argument('--manifest-path', type=Path,
                        default=ROOT / 'prima-user/rust-probe/Cargo.toml')
    options = parser.parse_args()
    clippy = options.clippy
    compiler, sysroot, digest = prepare()
    subprocess.run(['python3', str(ROOT / 'prima-user/musl/build.py')], cwd=ROOT, check=True)
    objects = ROOT / 'target/posix-crt'
    objects.mkdir(exist_ok=True)
    prefix = ROOT / 'target/musl-prefix'
    c_dir = ROOT / 'prima-user/c'
    names = ['prima_crt.c', 'prima_crt_start.S', 'prima_exec_bridge.S', 'prima_syscall.c']
    for name in names:
        subprocess.run(['clang', '-target', 'x86_64-unknown-none', '-O2', '-std=c11', '-Werror',
                        '-ffreestanding', '-fno-stack-protector', '-DPRIMA_WITH_LIBC', '-fPIC',
                        '-mgeneral-regs-only', '-nostdinc', '-isystem', str(prefix / 'include'),
                        '-c', str(c_dir / name), '-o', str(objects / (name + '.o'))], check=True)
    env = os.environ.copy()
    env.setdefault('CARGO_BUILD_JOBS', '2')
    env.update(RUSTC=str(ROOT / 'scripts/rustc_prima.py'), PRIMA_REAL_RUSTC=str(compiler),
               PRIMA_RUST_SYSROOT=str(sysroot), CARGO_CACHE_RUSTC_INFO='0',
               PRIMA_LIBC_SOURCE=str(sysroot / 'lib/rustlib/src/rust/library/prima-libc/src/lib.rs'))
    # Cargo does not track files named only by -C link-arg. Include their
    # contents in its fingerprint so C/persona changes cannot reuse an old ELF.
    link_inputs = [objects / (name + '.o') for name in names]
    link_inputs += [prefix / 'lib/libc.a', c_dir / 'prima_libc.map']
    link_digest = hashlib.sha256(b''.join(path.read_bytes() for path in link_inputs)).hexdigest()
    # Preserve application cfg/search/link options; the backend's required
    # abort strategy follows user options so it cannot silently become unwind.
    flags = inherited_rustflags(env)
    flags += ['-C', 'panic=abort', '--cfg', f'prima_overlay_hash="{digest}"', '--cfg', f'prima_link_hash="{link_digest}"']
    for name in names:
        flags += ['-C', 'link-arg=' + str(objects / (name + '.o'))]
    flags += ['-C', 'link-arg=-lc', '-C', 'link-arg=-lgcc',
              '-C', 'link-arg=--gc-sections',
              '-C', 'link-arg=--version-script=' + str(c_dir / 'prima_libc.map')]
    gcc = Path(subprocess.check_output(['gcc', '-print-libgcc-file-name'], text=True).strip()).parent
    flags += ['-L', 'native=' + str(gcc), '-L', 'native=' + str(prefix / 'lib')]
    env['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
    if clippy:
        shim = sysroot / 'bin/rustc'
        shim.parent.mkdir(exist_ok=True)
        if not shim.exists():
            shim.symlink_to(ROOT / 'scripts/rustc_prima.py')
        env.update(RUSTC=str(shim), SYSROOT=str(sysroot))
    command = ['cargo', 'clippy' if clippy else 'build', '--release', '--offline', '--locked',
               '--manifest-path', str(options.manifest_path.resolve()),
               '--target', str(ROOT / 'targets/x86_64-prima-posix.json'),
               '--target-dir', str(ROOT / ('target/posix-clippy' if clippy else 'target/rust-posix')),
               '-Zjson-target-spec', '-Zbuild-std=std,panic_abort',
               '-Zbuild-std-features=compiler-builtins-mem']
    if options.manifest_path.resolve() == ROOT / 'prima-user/rust-probe/Cargo.toml':
        command += ['--features', 'posix']
    if clippy:
        command += ['--no-deps', '--', '-D', 'warnings']
    subprocess.run(command, cwd=ROOT, env=env, check=True)


if __name__ == '__main__':
    (ROOT / 'target').mkdir(exist_ok=True)
    with (ROOT / 'target/.posix-std-build.lock').open('w') as mutex:
        fcntl.flock(mutex, fcntl.LOCK_EX)
        build()
