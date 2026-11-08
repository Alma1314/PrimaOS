#!/usr/bin/env python3
"""Collect locked root-workspace Cargo source archives and notice text candidates."""
import argparse
import json
from pathlib import Path
import platform
import re
import subprocess
import sys
import tarfile
import tomllib

sys.dont_write_bytecode = True
import ci
import package_evidence as bundle

ROOT = Path(__file__).resolve().parent.parent
REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'
COMMAND = ['cargo', 'metadata', '--locked', '--offline', '--format-version', '1', '--all-features']
EXCLUDED = ['nested bootloader cargo-install dependency graphs', 'Rust toolchain, standard library and build-std',
            'standalone prima-rust-probe dependency graph', 'musl, sbase, glibc, busybox and Linux inputs',
            'actual binary linkage and image byte binding']
TOOLS = ('package_cargo_materials.py', 'package_evidence.py', 'ci.py')


def git_blob(repo, revision, name):
    if not isinstance(revision, str) or not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', revision):
        raise ValueError('invalid source revision')
    bundle.relative(name)
    return subprocess.check_output(['git', '-C', str(repo), 'show', f'{revision}:{name}'])


def committed_config(repo, revision):
    if not isinstance(revision, str) or not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', revision):
        raise ValueError('invalid source revision')
    return bool(subprocess.check_output(['git', '-C', str(repo), 'ls-tree', '--name-only', '-z', revision,
                                         '--', '.cargo/config.toml']))


def package_key(package):
    name, version = package['name'], package['version']
    if not isinstance(name, str) or not isinstance(version, str) or not re.fullmatch('[A-Za-z0-9_-]+', name):
        raise ValueError('invalid Cargo package name/version')
    bundle.relative(f'{name}-{version}')
    return f'{name}@{version}'


def normalize_graph(repo, metadata, revision):
    ids = {p['id']: package_key(p) for p in metadata['packages']}
    if len(ids) != len(metadata['packages']) or len(set(ids.values())) != len(ids):
        raise ValueError('duplicate or ambiguous Cargo package identity')
    packages = []
    inputs = {'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'}
    if committed_config(repo, revision) or (repo / '.cargo/config.toml').exists():
        inputs.add('.cargo/config.toml')
    for p in metadata['packages']:
        item = dict(id=ids[p['id']], name=p['name'], version=p['version'], source=p['source'])
        if p['source'] is None:
            if p['id'] not in metadata['workspace_members']:
                raise ValueError('non-workspace path input is outside this material scope')
            name = Path(p['manifest_path']).resolve().relative_to(repo.resolve()).as_posix()
            inputs.add(name)
            item['manifest'] = f'inputs/{name}'
        elif p['source'] == REGISTRY:
            item['archive'] = f"archives/{p['name']}-{p['version']}.crate"
        else:
            raise ValueError('unsupported Cargo input source')
        packages.append(item)
    nodes = []
    for node in metadata['resolve']['nodes']:
        dependencies = [dict(name=d['name'], pkg=ids[d['pkg']], dep_kinds=sorted(d['dep_kinds'], key=bundle.encoded))
                        for d in node['deps']]
        nodes.append(dict(id=ids[node['id']], features=sorted(node['features']),
                          deps=sorted(dependencies, key=bundle.encoded)))
    graph = dict(packages=sorted(packages, key=lambda p: p['id']), nodes=sorted(nodes, key=lambda n: n['id']),
                 workspace_members=sorted(ids[p] for p in metadata['workspace_members']))
    return graph, inputs


def crate_record(package, raw, checksum):
    if bundle.digest(raw) != checksum:
        raise ValueError(f"locked crate checksum mismatch: {package['id']}")
    members = bundle.tar_members(raw, source=True)
    prefix = f"{package['name']}-{package['version']}"
    for name, (member, _) in members.items():
        if (name != prefix and not name.startswith(prefix + '/')) or not (member.isdir() or member.isfile()):
            raise ValueError('invalid crate member prefix/type')
    package_toml = tomllib.loads(members[f'{prefix}/Cargo.toml'][1].decode())['package']
    if (package_toml['name'], package_toml['version']) != (package['name'], package['version']):
        raise ValueError('crate manifest identity mismatch')
    license_file = package_toml.get('license-file', package_toml.get('license_file'))
    if license_file:
        bundle.relative(license_file)
        if f'{prefix}/{license_file}' not in members or members[f'{prefix}/{license_file}'][1] is None:
            raise ValueError('declared license-file is absent/nonregular')
    texts = {}
    for name, (_, data) in sorted(members.items()):
        if data is None:
            continue
        relative = name[len(prefix) + 1:]
        if (relative == license_file or any(p.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'COPYRIGHT', 'NOTICE'))
                                            for p in Path(relative).parts)):
            texts[f'notices/{prefix}/{relative}'] = data
    vcs_name = f'{prefix}/.cargo_vcs_info.json'
    vcs = json.loads(members[vcs_name][1]) if vcs_name in members else None
    record = dict(id=package['id'], name=package['name'], version=package['version'], source=REGISTRY,
                  archive=package['archive'], archive_identity=bundle.identity(raw),
                  declared_license=package_toml.get('license'), license_file=license_file,
                  repository=package_toml.get('repository'), vcs=vcs,
                  notice_texts={name: bundle.identity(data) for name, data in texts.items()})
    return record, texts


def derive(inputs, graph, read_archive):
    lock = tomllib.loads(inputs['inputs/Cargo.lock'].decode())
    locked = {(p['name'], p['version'], p.get('source')): p for p in lock['package']}
    packages = graph['packages']
    if (len(locked) != len(lock['package']) or len({p['id'] for p in packages}) != len(packages)
            or {(p['name'], p['version'], p['source']) for p in packages} != locked.keys()):
        raise ValueError('Cargo graph does not exactly match lock packages')
    ids = {p['id'] for p in packages}
    if {n['id'] for n in graph['nodes']} != ids or len(graph['nodes']) != len(ids):
        raise ValueError('incomplete/duplicate Cargo resolve nodes')
    for node in graph['nodes']:
        if not all(isinstance(f, str) for f in node['features']):
            raise ValueError('invalid Cargo feature record')
        for dep in node['deps']:
            if dep['pkg'] not in ids or not isinstance(dep['name'], str):
                raise ValueError('unknown dependency edge')
            for kind in dep['dep_kinds']:
                if kind['kind'] not in (None, 'normal', 'build', 'dev') or not (kind['target'] is None or isinstance(kind['target'], str)):
                    raise ValueError('invalid dependency kind/target')
    payload, records, gaps = {}, [], []
    own = set()
    for p in packages:
        if p['id'] != package_key(p):
            raise ValueError('Cargo package label mismatch')
        if p['source'] is None:
            name = bundle.relative(p['manifest']).as_posix()
            if not name.startswith('inputs/') or not name.endswith('/Cargo.toml'):
                raise ValueError('invalid workspace manifest reference')
            facts = tomllib.loads(inputs[name].decode())['package']
            if (facts['name'], facts['version']) != (p['name'], p['version']):
                raise ValueError('workspace manifest identity mismatch')
            own.add(p['id'])
            records.append(dict(id=p['id'], source=None, manifest=name, manifest_identity=bundle.identity(inputs[name]),
                                declared_license=facts.get('license')))
        elif p['source'] == REGISTRY:
            name = f"archives/{p['name']}-{p['version']}.crate"
            if p['archive'] != name:
                raise ValueError('noncanonical crate archive reference')
            checksum = locked[(p['name'], p['version'], REGISTRY)]['checksum']
            bundle.check_record(dict(size=0, sha256=checksum))
            raw = read_archive(name)
            record, texts = crate_record(p, raw, checksum)
            payload[name] = raw
            payload.update(texts)
            records.append(record)
            if not texts:
                gaps.append(p['id'])
        else:
            raise ValueError('unsupported source in Cargo graph')
    if own != set(graph['workspace_members']) or len(own) != len(graph['workspace_members']):
        raise ValueError('workspace member set mismatch')
    notice = ['Cargo input license material index; not a complete image NOTICE.',
              'Scope: root Cargo.lock, unfiltered cargo metadata --all-features.', '']
    for record in records:
        notice.append(f"{record['id']}: {record.get('declared_license')}")
        if record['source']:
            notice.extend(f'  {name}' for name in record['notice_texts'])
            if not record['notice_texts']:
                notice.append('  GAP: no license/notice text candidate in published archive')
    payload['NOTICE.txt'] = ('\n'.join(notice) + '\n').encode()
    return payload, records, sorted(gaps)


def package(repo, metadata, output, source, tools):
    graph, names = normalize_graph(repo, metadata, source['revision'])
    inputs = {}
    for name in sorted(names):
        data = bundle.read_file(repo, name)
        if data != git_blob(repo, source['revision'], name):
            raise ValueError(f'Cargo input differs from committed blob: {name}')
        inputs[f'inputs/{name}'] = data
    caches = {}
    for p in metadata['packages']:
        if p['source']:
            directory = Path(p['manifest_path']).parent
            if directory.name != f"{p['name']}-{p['version']}" or directory.parent.parent.name != 'src' or directory.parents[2].name != 'registry':
                raise ValueError('raw registry cache is required; vendor/path inputs unsupported')
            caches[f"archives/{p['name']}-{p['version']}.crate"] = directory.parents[2] / 'cache' / directory.parent.name
    payload, records, gaps = derive(inputs, graph, lambda name: bundle.read_file(caches[name], Path(name).name))
    payload.update(inputs)
    payload['cargo-graph.json'] = bundle.encoded(graph)
    for name in bundle.LICENSES:
        data = bundle.read_file(repo, name)
        if data != git_blob(repo, source['revision'], name) or bundle.digest(data) != bundle.LICENSES[name][1]:
            raise ValueError('project license text mismatch')
        payload[f'project-notices/{name}'] = data
    for name in TOOLS:
        payload[f'tools/{name}'] = (ROOT / 'scripts' / name).read_bytes()
    manifest = dict(schema_version=1, kind='prima-cargo-input-materials', source=source, tools=tools,
                    command=COMMAND, packages=records, notice_text_gaps=gaps, excluded_inputs=EXCLUDED,
                    binary_images_included=False, image_release_eligible=False,
                    members={name: bundle.identity(data) for name, data in payload.items()})
    payload['manifest.json'] = bundle.encoded(manifest)
    if ci.source_identity(repo) != source:
        raise ValueError('source changed while collecting Cargo materials')
    bundle.publish(payload, output)
    return manifest


def verify(repo, path):
    path = Path(path)
    if not path.is_file() or path.stat().st_size > bundle.MAX_TOTAL:
        raise ValueError('missing or oversized material package')
    files = {n: data for n, (_, data) in bundle.tar_members(path.read_bytes()).items()}
    manifest = json.loads(files['manifest.json'])
    if (not isinstance(manifest, dict) or not isinstance(manifest.get('members'), dict)
            or manifest['schema_version'] != 1 or manifest['kind'] != 'prima-cargo-input-materials'
            or manifest['command'] != COMMAND or manifest['excluded_inputs'] != EXCLUDED
            or manifest['image_release_eligible'] is not False or manifest['binary_images_included'] is not False
            or files.keys() != manifest['members'].keys() | {'manifest.json'}
            or any(bundle.identity(files[n]) != record for n, record in manifest['members'].items())):
        raise ValueError('material package member/scope mismatch')
    inputs = {n: data for n, data in files.items() if n.startswith('inputs/')}
    for name, data in inputs.items():
        if data != git_blob(repo, manifest['source']['revision'], name.removeprefix('inputs/')):
            raise ValueError('input blob differs from Git revision')
    graph = json.loads(files['cargo-graph.json'])
    derived, records, gaps = derive(inputs, graph, lambda n: files[n])
    if records != manifest['packages'] or gaps != manifest['notice_text_gaps'] or any(files[n] != data for n, data in derived.items()):
        raise ValueError('material declarations/notices differ from locked archives')
    expected_inputs = {'inputs/Cargo.lock', 'inputs/Cargo.toml', 'inputs/rust-toolchain.toml'}
    expected_inputs.update(p['manifest'] for p in graph['packages'] if p['source'] is None)
    if committed_config(repo, manifest['source']['revision']):
        expected_inputs.add('inputs/.cargo/config.toml')
    if set(inputs) != expected_inputs:
        raise ValueError('unexpected/missing input manifests')
    referenced = set(derived) | set(inputs) | {'manifest.json', 'cargo-graph.json'}
    for name in bundle.LICENSES:
        member = f'project-notices/{name}'
        if files[member] != git_blob(repo, manifest['source']['revision'], name) or bundle.digest(files[member]) != bundle.LICENSES[name][1]:
            raise ValueError('project notice text mismatch')
        referenced.add(member)
    referenced.update(f'tools/{n}' for n in TOOLS)
    if files.keys() != referenced:
        raise ValueError('unreferenced material package members')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--verify', type=Path)
    args = parser.parse_args()
    if bool(args.output) == bool(args.verify):
        parser.error('choose --output or --verify')
    try:
        if args.verify:
            result = verify(args.repo, args.verify)
        else:
            source = ci.source_identity(args.repo)
            env = ci.environment(args.repo)
            metadata = json.loads(subprocess.check_output(COMMAND, cwd=args.repo, env=env))
            tools = dict(python=platform.python_version(), cargo=subprocess.check_output(['cargo', '--version'], env=env, text=True).strip())
            result = package(args.repo, metadata, args.output, source, tools)
        registry = sum(p['source'] == REGISTRY for p in result['packages'])
        print(f'PASS cargo-materials packages={len(result["packages"])} registry={registry} notice_text_gaps={len(result["notice_text_gaps"])} images=0')
    except (ValueError, OSError, EOFError, KeyError, TypeError, tarfile.TarError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'ERROR {error}\n')


if __name__ == '__main__':
    main()
