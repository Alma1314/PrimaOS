#!/usr/bin/env python3
"""Package a Git source snapshot and consistent nine-group CI evidence, not images."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import platform
import posixpath
import re
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
GROUPS = {'host', 'lint', 'docs', 'bios', 'linux', 'gate', 'gate-audit', 'domains', 'formal'}
MAX_FILE = 64 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
MAX_MEMBERS = 10000
LICENSES = {
    'LICENSE': ('GPL-3.0-or-later', '3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986'),
    'prima-ostd/LICENSE-MPL': ('MPL-2.0', '86cf9656479f1edb82245b985f4a2cc0d503b945766ebeda4269c60b307699ae'),
    'prima-user/musl/COPYRIGHT': ('MIT', 'f9bc4423732350eb0b3f7ed7e91d530298476f8fec0c6c427a1c04ade22655af'),
}
MPL_FILES = ('prima-ostd/src/align.rs', 'prima-ostd/src/pod.rs', 'prima-ostd/src/sync/lock.rs')
PENDING = ['binary component and third-party NOTICE inventory',
           'external corresponding source and build materials', 'image bytes bound to acceptance evidence']


def digest(data):
    return hashlib.sha256(data).hexdigest()


def identity(data):
    return dict(size=len(data), sha256=digest(data))


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode()


def relative(value):
    if not isinstance(value, str) or not value or '\\' in value or '\0' in value:
        raise ValueError('invalid relative evidence path')
    path = PurePosixPath(value)
    if path.is_absolute() or '..' in path.parts or value != path.as_posix() or value == '.':
        raise ValueError(f'unsafe evidence path: {value}')
    return path


def read_file(root, name):
    path = root
    for part in relative(name).parts:
        path = path / part
        if path.is_symlink():
            raise ValueError(f'evidence symlink: {name}')
    if not path.is_file() or path.stat().st_size > MAX_FILE:
        raise ValueError(f'missing, nonregular or oversized evidence: {name}')
    data = path.read_bytes()
    if len(data) > MAX_FILE:
        raise ValueError('evidence grew beyond size limit')
    return data


def tar_members(raw, source=False):
    files = {}
    total = 0
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:*') as archive:
        for index, member in enumerate(archive):
            if index >= MAX_MEMBERS:
                raise ValueError('too many archive members')
            name = relative(member.name.rstrip('/') if member.isdir() else member.name).as_posix()
            if name in files:
                raise ValueError(f'duplicate archive member: {name}')
            if source and member.isdir():
                files[name] = (member, None)
                continue
            if source and member.issym():
                target = posixpath.normpath(posixpath.join(posixpath.dirname(name), member.linkname))
                if not member.linkname or member.linkname.startswith('/') or target == '..' or target.startswith('../'):
                    raise ValueError('source symlink escapes snapshot')
                files[name] = (member, None)
                continue
            if not member.isfile() or member.size < 0 or member.size > MAX_FILE:
                raise ValueError('nonregular or oversized archive member')
            total += member.size
            if total > MAX_TOTAL:
                raise ValueError('archive exceeds total size limit')
            files[name] = (member, archive.extractfile(member).read())
    return files


def snapshot(repo, revision):
    commit = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', '--verify', '--end-of-options', f'{revision}^{{commit}}'], text=True).strip()
    raw = subprocess.check_output(['git', '-C', str(repo), 'archive', '--format=tar', commit])
    if len(raw) > MAX_FILE:
        raise ValueError('source archive exceeds file limit')
    members = tar_members(raw, source=True)
    value = hashlib.sha256()
    files = {}
    for name in sorted(members, key=os.fsencode):
        member, data = members[name]
        if member.isdir():
            continue
        value.update(os.fsencode(name) + b'\0')
        if member.issym():
            value.update(b'link\0' + os.fsencode(member.linkname))
        else:
            value.update(str(member.mode & 0o111).encode() + b'\0')
            value.update(hashlib.sha256(data).digest())
            files[name] = data
    return raw, dict(revision=commit, dirty=False, files_sha256=value.hexdigest()), files


def licenses(files):
    notices = []
    for name, (license_name, expected) in LICENSES.items():
        if name not in files or digest(files[name]) != expected:
            raise ValueError(f'missing or changed license text: {name}')
        notices.append(dict(path=name, declared_license=license_name, **identity(files[name])))
    declarations = []
    for name, data in sorted(files.items()):
        if name.endswith('Cargo.toml'):
            package = tomllib.loads(data.decode()).get('package')
            if package:
                declarations.append(dict(path=name, name=package['name'], declared_license=package.get('license')))
    exceptions = []
    for name in MPL_FILES:
        if name not in files or not files[name].startswith(b'// SPDX-License-Identifier: MPL-2.0\n'):
            raise ValueError(f'missing MPL file declaration: {name}')
        exceptions.append(dict(path=name, declared_license='MPL-2.0', **identity(files[name])))
    return dict(texts=notices, crate_declarations=declarations, file_declarations=exceptions)


def check_record(record):
    if (not isinstance(record, dict) or type(record.get('size')) is not int or record['size'] < 0
            or not isinstance(record.get('sha256'), str) or not re.fullmatch('[0-9a-f]{64}', record['sha256'])):
        raise ValueError('invalid recorded file identity')


def validate_run(raw, source, read):
    manifest = json.loads(raw)
    if (not isinstance(manifest, dict) or manifest.get('schema_version') != 1
            or not isinstance(manifest.get('group'), str) or manifest['group'] not in GROUPS
            or manifest.get('status') != 'passed' or manifest.get('source') != source
            or not isinstance(manifest.get('steps'), list) or not manifest['steps']):
        raise ValueError('failed, incomplete or mismatched CI manifest')
    files = {'result.json': raw}
    for step in manifest['steps']:
        if (not isinstance(step, dict) or step.get('status') != 'passed'
                or type(step.get('exit_code')) is not int or step['exit_code'] != 0
                or not isinstance(step.get('argv'), list) or not step['argv']
                or not all(isinstance(arg, str) and arg for arg in step['argv'])
                or not isinstance(step.get('traces'), list)):
            raise ValueError('CI step did not pass a real command')
        log = relative(step['log']).as_posix()
        if '/' in log or not log.endswith('.log') or log in files:
            raise ValueError('invalid or duplicate command log')
        files[log] = read(log)
        for trace in step['traces']:
            check_record(trace)
            name = relative(trace['path']).as_posix()
            if not name.startswith('traces/') or name in files:
                raise ValueError('invalid or duplicate trace path')
            data = read(name)
            if identity(data) != {key: trace[key] for key in ('size', 'sha256')}:
                raise ValueError(f'trace identity mismatch: {name}')
            files[name] = data
    seen = set()
    if not isinstance(manifest.get('artifacts'), list):
        raise ValueError('invalid artifact identity list')
    for artifact in manifest['artifacts']:
        check_record(artifact)
        name = relative(artifact['path']).as_posix()
        if not name.startswith('target/prima-os/') or name in seen:
            raise ValueError('invalid or duplicate artifact identity')
        seen.add(name)
    return manifest, files


def package(repo, evidence_root, revision, output):
    source_tar, source, source_files = snapshot(repo, revision)
    license_facts = licenses(source_files)
    payload = {'source.tar': source_tar, 'tools/package_evidence.py': Path(__file__).read_bytes()}
    for name in LICENSES:
        payload[f'licenses/{name}'] = source_files[name]
    groups = {}
    evidence_root = evidence_root.resolve()
    for path in sorted(evidence_root.rglob('result.json')):
        raw = read_file(evidence_root, path.relative_to(evidence_root).as_posix())
        manifest, files = validate_run(raw, source, lambda name: read_file(path.parent, name))
        group = manifest['group']
        if group in groups:
            raise ValueError(f'duplicate CI group: {group}')
        groups[group] = dict(steps=len(manifest['steps']), traces=sum(len(s['traces']) for s in manifest['steps']),
                             artifact_identities=manifest['artifacts'], image_bytes_verified=False)
        for name, data in files.items():
            payload[f'evidence/{group}/{name}'] = data
    if groups.keys() != GROUPS:
        raise ValueError('evidence package requires exactly nine CI groups')
    manifest = dict(schema_version=1, kind='prima-source-validation-evidence', source=source,
                    packaging_tools=dict(python=platform.python_version(), git=subprocess.check_output(['git', '--version'], text=True).strip()),
                    binary_images_included=False, image_release_eligible=False, pending_binary_materials=PENDING,
                    licenses=license_facts, groups=groups, members={name: identity(data) for name, data in payload.items()})
    payload['manifest.json'] = encoded(manifest)
    if len(payload) > MAX_MEMBERS or sum(map(len, payload.values())) > MAX_TOTAL:
        raise ValueError('package exceeds member/size limit')
    if any(len(data) > MAX_FILE for data in payload.values()):
        raise ValueError('package member exceeds size limit')
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=output.parent, prefix='.prima-evidence-', delete=False) as temporary:
        name = Path(temporary.name)
        try:
            with gzip.GzipFile(fileobj=temporary, filename='', mode='wb', mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode='w') as archive:
                    for member, data in sorted(payload.items()):
                        info = tarfile.TarInfo(member)
                        info.size, info.mode = len(data), 0o644
                        archive.addfile(info, io.BytesIO(data))
            temporary.flush()
            os.fsync(temporary.fileno())
            os.link(name, output)  # Exclusive publication: never replace an existing bundle.
        finally:
            name.unlink()
    return manifest


def verify(repo, path):
    path = Path(path)
    if not path.is_file() or path.stat().st_size > MAX_TOTAL:
        raise ValueError('missing or oversized package')
    files = {name: data for name, (_, data) in tar_members(path.read_bytes()).items()}
    manifest = json.loads(files['manifest.json'])
    if not isinstance(manifest, dict) or not isinstance(manifest.get('members'), dict):
        raise ValueError('invalid package manifest')
    expected = manifest['members']
    if files.keys() != expected.keys() | {'manifest.json'} or any(identity(files[name]) != record for name, record in expected.items()):
        raise ValueError('package member set or identity mismatch')
    source_tar, source, source_files = snapshot(repo, manifest['source']['revision'])
    if (manifest.get('schema_version') != 1 or manifest.get('kind') != 'prima-source-validation-evidence'
            or manifest['source'] != source or files['source.tar'] != source_tar
            or manifest.get('binary_images_included') is not False or manifest.get('image_release_eligible') is not False
            or manifest.get('pending_binary_materials') != PENDING or manifest.get('licenses') != licenses(source_files)
            or not isinstance(manifest.get('groups'), dict) or manifest['groups'].keys() != GROUPS):
        raise ValueError('package source, scope or license facts mismatch')
    referenced = {'manifest.json', 'source.tar', 'tools/package_evidence.py'}
    referenced.update(f'licenses/{name}' for name in LICENSES)
    for name in LICENSES:
        if files[f'licenses/{name}'] != source_files[name]:
            raise ValueError('packaged license text mismatch')
    for group, summary in manifest['groups'].items():
        prefix = f'evidence/{group}/'
        run, evidence = validate_run(files[prefix + 'result.json'], source, lambda name: files[prefix + name])
        if (run['group'] != group or summary != dict(steps=len(run['steps']), traces=sum(len(s['traces']) for s in run['steps']),
                                                   artifact_identities=run['artifacts'], image_bytes_verified=False)):
            raise ValueError('CI group summary mismatch')
        referenced.update(prefix + name for name in evidence)
    if referenced != files.keys():
        raise ValueError('unreferenced package members')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--revision', default='HEAD')
    parser.add_argument('--evidence-root', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--verify', type=Path)
    args = parser.parse_args()
    if args.verify:
        if args.evidence_root or args.output or args.revision != 'HEAD':
            parser.error('--verify cannot be combined with build options')
    elif not args.evidence_root or not args.output:
        parser.error('building requires --evidence-root and --output')
    try:
        result = verify(args.repo, args.verify) if args.verify else package(args.repo, args.evidence_root, args.revision, args.output)
        print(f'PASS source-evidence revision={result["source"]["revision"]} groups=9 traces={sum(g["traces"] for g in result["groups"].values())} images=0')
    except (ValueError, OSError, EOFError, KeyError, TypeError, tarfile.TarError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'ERROR {error}\n')


if __name__ == '__main__':
    main()
