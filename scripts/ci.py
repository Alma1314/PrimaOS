#!/usr/bin/env python3
"""Run one bounded CI group; retain failures and identities under ignored target/."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent
sys.dont_write_bytecode = True
GROUPS = ('host', 'lint', 'docs', 'bios', 'linux', 'gate', 'gate-audit', 'domains', 'formal', 'smoke')
IMAGE_GROUPS = {'bios', 'linux', 'gate', 'gate-audit', 'domains', 'smoke'}
BUDGETS = dict(host=900, lint=900, docs=300, bios=2400, linux=1800, gate=1200,
               **{'gate-audit': 1200}, domains=600, formal=3600, smoke=600)
SELECTORS = ('PRIMA_GATE_CASE', 'PRIMA_GATE_AUDIT_SITE', 'PRIMA_STAGE6_DOMAINS',
             'PRIMA_LINUX_ELF', 'PRIMA_QEMU_DEBUG_LOG', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
             'RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'PRIMA_REAL_RUSTC', 'PRIMA_RUST_SYSROOT')


def digest(path, algorithm='sha256'):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, algorithm).hexdigest()


def verify_hash(path, expected, algorithm='sha256'):
    observed = digest(path, algorithm)
    if observed != expected:
        raise ValueError(f'input hash mismatch: {path}')
    return observed


def environment(root=ROOT):
    env = {key: value for key, value in os.environ.items() if key not in SELECTORS}
    toolchain = tomllib.loads((root / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    env.update(LC_ALL='C', LANG='C', CARGO_BUILD_JOBS='2', PRIMA_MUSL_JOBS='2',
               CARGO_NET_OFFLINE='true', RUSTUP_TOOLCHAIN=toolchain,
               LAKE_NO_CACHE='true', MATHLIB_NO_CACHE_ON_UPDATE='1')
    return env


def step(argv, env=None, cwd='.'):
    return dict(argv=argv, env=env or {}, cwd=cwd)


def commands(group, root=ROOT):
    toolchain = tomllib.loads((root / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    cargo = lambda *args: ['rustup', 'run', toolchain, 'cargo', *args]
    xtask = lambda *args: cargo('run', '--locked', '-p', 'xtask', '--', *args)
    probe = lambda feature, verb, env=None: step(
        cargo('run', '--locked', '-p', 'xtask', '--features', feature, '--', verb), env)
    if group == 'host':
        return [step(cargo('test', '--locked', '--workspace')),
                step(['python3', 'prima-user/musl/test_port.py']),
                step(['python3', 'scripts/test_posix_std.py']),
                step(['python3', 'scripts/test_ci.py'])]
    if group == 'lint':
        return [step(xtask('lint')), step(xtask('tcb'))]
    if group == 'docs':
        result = [step(['python3', 'scripts/docs.py', action]) for action in ('self-test', 'build', 'check')]
        if os.environ.get('PRIMA_CI_BASE'):
            result += [step(['python3', 'scripts/docs.py', 'check', '--base', os.environ['PRIMA_CI_BASE']])]
        return result
    if group == 'bios':
        return [step(xtask('test'))]
    if group == 'smoke':
        return [step(xtask('test-process', 'W5')), step(xtask('compat'))]
    if group == 'linux':
        return [probe('linux-probe', 'test-linux', {
            'PRIMA_LINUX_ELF': str(root / 'target/ci-inputs/busybox/usr/bin/busybox')})]
    if group == 'gate':
        return [probe('gate-probe', 'test-gate', {} if case is None else {'PRIMA_GATE_CASE': case})
                for case in (None, 'G2', 'G2-death', 'G3', 'G4', 'G5', 'N1', 'I1')]
    if group == 'gate-audit':
        return [probe('gate-audit-probe', 'test-gate-audit', {'PRIMA_GATE_AUDIT_SITE': site})
                for site in ('int80-entry', 'dispatch', 'int80-return', 'fast-return', 'pick-next', 'resume', 'irq')]
    if group == 'domains':
        return [step(xtask('test-domains'), {'PRIMA_STAGE6_DOMAINS': '1'})]
    if group == 'formal':
        lock = json.loads((root / 'lean/refinement/toolchain.lock.json').read_text())
        return [step(['elan', 'run', lock['lean'], 'lake', 'build'], cwd='lean'),
                step(['elan', 'run', lock['lean'], 'lake', 'env', 'lean', 'Checks/KernelHandleTable.lean'], cwd='lean'),
                step(['python3', 'scripts/check_handle_refinement.py', '--mutation-test'],
                     {'RUSTUP_TOOLCHAIN': lock['rust']})]
    raise ValueError(f'unknown group: {group}')


def git_output(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def source_identity(root):
    names = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z', '--cached', '--others', '--exclude-standard'])
    value = hashlib.sha256()
    for name in sorted(set(names.split(b'\0')) - {b''}):
        path = root / os.fsdecode(name)
        value.update(name + b'\0')
        if path.is_symlink():
            value.update(b'link\0' + os.fsencode(os.readlink(path)))
        elif path.is_file():
            value.update(str(path.stat().st_mode & 0o111).encode() + b'\0')
            value.update(bytes.fromhex(digest(path)))
        else:
            value.update(b'deleted')
    return dict(revision=git_output(root, 'rev-parse', 'HEAD'),
                dirty=bool(git_output(root, 'status', '--porcelain')), files_sha256=value.hexdigest())


def tool_identity(env):
    tools = {}
    for name, args in [('rustup', ['--version']), ('rustc', ['-Vv']), ('cargo', ['-V']), ('clang', ['--version']),
                       ('llvm-ar', ['--version']), ('cc', ['--version']),
                       ('qemu-system-x86_64', ['--version']), ('elan', ['--version']),
                       ('git', ['--version']), ('make', ['--version']), ('objdump', ['--version'])]:
        try:
            argv = (['rustup', 'run', env['RUSTUP_TOOLCHAIN'], name] if name in ('rustc', 'cargo') else [name]) + args
            result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=15)
            tools[name] = dict(exit_code=result.returncode, version=(result.stdout + result.stderr).strip())
        except (OSError, subprocess.TimeoutExpired) as error:
            tools[name] = dict(error=str(error))
    return dict(platform=platform.platform(), python=sys.version, tools=tools,
                runner_image=os.environ.get('ImageVersion'), cpu_count=os.cpu_count())


def outputs(root, patterns=('*.img', '*.elf', '*/*.img', '*/*.elf')):
    return {path: (path.stat().st_size, path.stat().st_mtime_ns)
            for pattern in patterns for path in (root / 'target/prima-os').glob(pattern)
            if path.is_file()}


def write_result(path, data):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')
    temporary.replace(path)


def run_step(item, root, env, log, timeout):
    if timeout <= 0:
        raise subprocess.TimeoutExpired(item['argv'], timeout)
    with log.open('w') as output:
        child = subprocess.Popen(item['argv'], cwd=root / item['cwd'],
                                 env=dict(env, **item['env']), stdout=output,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            return child.wait(timeout=max(0.01, timeout))
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
            raise


def run_group(group, items, root=ROOT, budget=None, require_clean=False, versions=True):
    started = time.monotonic()
    directory = root / 'target/ci' / group / uuid.uuid4().hex
    directory.mkdir(parents=True)
    path = directory / 'result.json'
    data = dict(schema_version=1, group=group, status='running', steps=[], artifacts=[],
                budget_seconds=budget or BUDGETS[group], cargo_jobs=2, musl_jobs=2)
    write_result(path, data)
    try:
        # This serializes group runners; child xtasks still own the image lock.
        with (root / 'target/ci-run.lock').open('a+b') as mutex:
            fcntl.flock(mutex, fcntl.LOCK_EX | fcntl.LOCK_NB)
            data['source'] = source_identity(root)
            if require_clean and data['source']['dirty']:
                raise ValueError('CI requires a clean checkout')
            env = environment(root)
            if versions:
                data['environment'] = tool_identity(env)
            before = outputs(root)
            for index, item in enumerate(items):
                if not item.get('prepare') and 'inputs' not in data:
                    data['inputs'] = input_identity(group, root)
                record = dict(item, status='running', exit_code=None, log=f'{index:02}.log')
                trace_before = outputs(root, ('*.log',))
                data['steps'].append(record)
                write_result(path, data)
                print(f'RUN {group} {index + 1}/{len(items)}: {item["argv"]}', flush=True)
                elapsed = time.monotonic()
                try:
                    remaining = data['budget_seconds'] - (elapsed - started)
                    code = run_step(item, root, env, directory / record['log'], remaining)
                    record.update(exit_code=code, status='passed' if code == 0 else 'failed')
                except subprocess.TimeoutExpired:
                    record.update(status='timed_out', exit_code=124)
                    raise
                except KeyboardInterrupt:
                    record.update(status='interrupted', exit_code=130)
                    raise
                except OSError:
                    record.update(status='failed', exit_code=127)
                    raise
                finally:
                    record['seconds'] = round(time.monotonic() - elapsed, 3)
                    # Preserve each configuration before later steps overwrite its guest logs.
                    record['traces'] = []
                    for source, state in sorted(outputs(root, ('*.log',)).items()):
                        if trace_before.get(source) == state or source.is_symlink():
                            continue
                        saved = directory / 'traces' / f'{index:02}' / source.name
                        saved.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(source, saved)
                        record['traces'].append(dict(source=str(source.relative_to(root)),
                                                     path=str(saved.relative_to(directory)),
                                                     size=saved.stat().st_size, sha256=digest(saved)))
                write_result(path, data)
                if code != 0:
                    raise ValueError(f'command failed with exit {code}; see {record["log"]}')
            if not items:
                raise ValueError('an empty group cannot pass')
            if source_identity(root) != data['source']:
                raise ValueError('source changed during validation')
            data['inputs'] = input_identity(group, root)
            if versions:
                data['environment'] = tool_identity(env)
            data['artifacts'] = [dict(path=str(p.relative_to(root)), size=state[0], sha256=digest(p))
                                 for p, state in sorted(outputs(root).items()) if before.get(p) != state]
            data['status'] = 'passed'
    except KeyboardInterrupt:
        data.update(status='interrupted', error='validation interrupted')
    except subprocess.TimeoutExpired as error:
        data.update(status='timed_out', error=str(error))
    except Exception as error:
        data.update(status='failed', error=str(error))
    finally:
        data['seconds'] = round(time.monotonic() - started, 3)
        write_result(path, data)
        print(f'RESULT {data["status"]}: {path}', flush=True)
    return data, path


def checked_download(url, destination, expected, algorithm='sha256'):
    if destination.exists():
        verify_hash(destination, expected, algorithm)
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as directory:
        temporary = Path(directory) / 'download'
        with urllib.request.urlopen(url, timeout=90) as source, temporary.open('wb') as output:
            import shutil
            shutil.copyfileobj(source, output)
        verify_hash(temporary, expected, algorithm)
        temporary.replace(destination)


def checked_checkout(url, destination, commit):
    if not destination.exists():
        destination.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=destination.parent) as directory:
            temporary = Path(directory) / 'checkout'
            subprocess.run(['git', 'clone', '--no-checkout', url, str(temporary)], check=True)
            subprocess.run(['git', '-C', str(temporary), 'checkout', '--detach', commit], check=True)
            temporary.rename(destination)
    if git_output(destination, 'rev-parse', '--show-toplevel') != str(destination.resolve()):
        raise ValueError(f'input must be its own checkout: {destination}')
    if git_output(destination, 'rev-parse', 'HEAD') != commit or git_output(destination, 'status', '--porcelain', '--untracked-files=all'):
        raise ValueError(f'input commit/cleanliness mismatch: {destination}')


def musl_module(root):
    spec = importlib.util.spec_from_file_location('prima_musl_build', root / 'prima-user/musl/build.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def input_identity(group, root=ROOT):
    if group not in IMAGE_GROUPS and group != 'formal':
        return {}
    lock = json.loads((root / 'scripts/ci-inputs.lock.json').read_text())
    result = {'ci_lock_sha256': digest(root / 'scripts/ci-inputs.lock.json')}
    if group in IMAGE_GROUPS:
        musl = tomllib.loads((root / 'docs/superpowers/compat/musl-snapshot.lock.toml').read_text())
        snapshot = root / musl['snapshot_dir']
        musl_module(root).verify_snapshot(snapshot, musl)
        result['musl'] = dict(commit=git_output(snapshot, 'rev-parse', 'HEAD'),
                              license_sha256=digest(snapshot / musl['license_file']))
        sbase = root / 'target/sbase'
        if sbase.exists():
            checked_checkout(lock['sbase']['url'], sbase, lock['sbase']['commit'])
        if not sbase.exists() or digest(sbase / 'LICENSE') != lock['sbase']['license_sha256']:
            raise ValueError('missing or unpinned sbase input')
        result['sbase'] = dict(commit=git_output(sbase, 'rev-parse', 'HEAD'), license_sha256=digest(sbase / 'LICENSE'))
    if group == 'linux':
        elf = root / 'target/ci-inputs/busybox/usr/bin/busybox'
        kernel = root / 'target/linux-ref' / lock['linux']['filename']
        result['linux'] = dict(busybox_sha256=verify_hash(elf, lock['busybox']['elf_sha256']),
                              kernel_deb_sha512=verify_hash(kernel, lock['linux']['sha512'], 'sha512'))
        vmlinuz = root / 'target/linux-ref/unpack/boot/vmlinuz-6.8.0-31-generic'
        if vmlinuz.exists():
            result['linux']['cached_vmlinuz_sha256'] = verify_hash(vmlinuz, lock['linux']['vmlinuz_sha256'])
    if group == 'formal':
        formal = json.loads((root / 'lean/refinement/toolchain.lock.json').read_text())
        result['formal'] = {}
        for name, expected in formal['binaries'].items():
            binary = root / 'target/aeneas' / name
            if not binary.is_file() or digest(binary) != expected:
                raise ValueError(f'missing or unpinned formal tool: {binary}')
            result['formal'][name] = digest(binary)
        manifest = json.loads((root / 'lean/refinement/lake-manifest.json').read_text())
        result['formal_dependencies'] = {}
        for package in manifest['packages']:
            if package['type'] != 'git':
                continue
            destination = root / 'lean/refinement/.lake/packages' / package['name']
            if not destination.exists():
                raise ValueError(f'missing formal dependency: {package["name"]}')
            checked_checkout(package['url'], destination, package['rev'])
            result['formal_dependencies'][package['name']] = package['rev']
    return result


def formal_cache_roots(backend):
    # The locked backend uses one import per line; only its external imports need cache files.
    roots = set()
    for source in backend.rglob('*.lean'):
        if '.lake' not in source.relative_to(backend).parts:
            roots.update(re.findall(r'(?m)^(?:(?:public|meta) )*import '
                                    r'((?:Mathlib|Batteries|Aesop|Qq|Plausible|LeanSearchClient|ImportGraph|ProofWidgets)(?:\.\w+)*)\s*$',
                                    source.read_text()))
    if not roots:
        raise ValueError('locked formal backend has no recognized cache imports')
    return sorted(roots)


def prepare(group, root=ROOT):
    env = environment(root)
    env['CARGO_NET_OFFLINE'] = 'false'
    env['LAKE_NO_CACHE'] = 'false'
    def run(argv, cwd=root):
        # The outer group deadline also kills descendants, including download workers.
        subprocess.run(argv, cwd=cwd, env=env, check=True)
    if group == 'docs':
        return
    toolchain = tomllib.loads((root / 'rust-toolchain.toml').read_text())['toolchain']
    run(['rustup', 'toolchain', 'install', toolchain['channel'], '--no-self-update', '--profile', 'minimal',
         '--component', ','.join(toolchain['components']), '--target', ','.join(toolchain['targets'])])
    run(['cargo', 'fetch', '--locked'])
    if group != 'formal':
        # bootloader's build.rs installs five BIOS/UEFI crates outside Cargo.lock's graph.
        # Prepare them online; every acceptance command still runs offline.
        run(['cargo', 'build', '--locked', '--package', 'bootloader'])
    if group == 'bios':
        sysroot = subprocess.check_output(['rustc', '--print', 'sysroot'], env=env, text=True).strip()
        for manifest in [root / 'prima-user/rust-probe/Cargo.toml', Path(sysroot) / 'lib/rustlib/src/rust/library/Cargo.toml']:
            run(['cargo', 'fetch', '--locked', '--manifest-path', str(manifest)])
    lock = json.loads((root / 'scripts/ci-inputs.lock.json').read_text())
    if group in IMAGE_GROUPS:
        musl_lock = tomllib.loads((root / 'docs/superpowers/compat/musl-snapshot.lock.toml').read_text())
        snapshot = root / musl_lock['snapshot_dir']
        checked_checkout(musl_lock['mirror_url'], snapshot, musl_lock['commit'])
        musl_module(root).verify_snapshot(snapshot, musl_lock)
        sbase = root / 'target/sbase'
        checked_checkout(lock['sbase']['url'], sbase, lock['sbase']['commit'])
        if digest(sbase / 'LICENSE') != lock['sbase']['license_sha256']:
            raise ValueError('sbase license hash mismatch')
    if group == 'linux':
        package = root / 'target/ci-inputs/busybox.deb'
        checked_download(lock['busybox']['url'], package, lock['busybox']['package_sha256'])
        extracted = package.parent / 'busybox'
        run(['dpkg-deb', '-x', str(package), str(extracted)])
        if digest(extracted / 'usr/bin/busybox') != lock['busybox']['elf_sha256']:
            raise ValueError('D12 busybox ELF hash mismatch')
        kernel = lock['linux']
        checked_download(kernel['url'], root / 'target/linux-ref' / kernel['filename'], kernel['sha512'], 'sha512')
    if group == 'formal':
        formal = json.loads((root / 'lean/refinement/toolchain.lock.json').read_text())
        run(['rustup', 'toolchain', 'install', formal['rust'], '--no-self-update', '--profile', 'minimal', '--component', 'rustc-dev,rust-src'])
        run(['elan', 'toolchain', 'install', formal['lean']])
        archive = root / 'target/ci-inputs/aeneas.tar.gz'
        checked_download(formal['url'], archive, formal['sha256'])
        destination = root / 'target/aeneas'
        if not destination.exists():
            with tarfile.open(archive) as source:
                source.extractall(destination, filter='data')
        for name, expected in formal['binaries'].items():
            if digest(destination / name) != expected:
                raise ValueError(f'unpinned formal tool: {name}')
        backend = destination / 'backends/lean'
        cache = ['elan', 'run', formal['lean'], 'lake', 'exe', 'cache', 'get', *formal_cache_roots(backend)]
        run(cache, backend)
        packages = root / 'lean/refinement/.lake/packages'
        packages.parent.mkdir(parents=True, exist_ok=True)
        if not packages.exists():
            packages.symlink_to(backend / '.lake/packages', target_is_directory=True)
        run(cache, root / 'lean/refinement')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('group', choices=GROUPS)
    parser.add_argument('--plan', action='store_true')
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--prepare-inputs', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--require-clean', action='store_true')
    args = parser.parse_args()
    if args.prepare_inputs and (args.plan or args.prepare or args.require_clean):
        parser.error('--prepare-inputs cannot be combined with run/plan options')
    if args.prepare_inputs:
        prepare(args.group)
        return 0
    items = commands(args.group)
    if args.prepare:
        preparation = step(['python3', 'scripts/ci.py', args.group, '--prepare-inputs'], {'CARGO_NET_OFFLINE': 'false'})
        preparation['prepare'] = True
        items.insert(0, preparation)
    if args.plan:
        print(json.dumps(dict(group=args.group, budget=BUDGETS[args.group], steps=items), indent=2))
        return 0
    def interrupted(signum, frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    result, _ = run_group(args.group, items, require_clean=args.require_clean)
    return 0 if result['status'] == 'passed' else 1


if __name__ == '__main__':
    sys.exit(main())
