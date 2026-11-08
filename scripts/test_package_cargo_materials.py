#!/usr/bin/env python3
"""Small locked registry graph: provenance, tamper rejection and notice gaps."""
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True
import ci
import package_cargo_materials as materials
import package_evidence as bundle


class CargoMaterialTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix='prima-cargo-material-test-')
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        for name in bundle.LICENSES:
            target = self.root / name; target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((bundle.ROOT / name).read_bytes())
        (self.root / '.gitignore').write_text('target/\n')
        (self.root / '.cargo').mkdir()
        (self.root / '.cargo/config.toml').write_text('[build]\njobs=2\n')
        (self.root / 'Cargo.toml').write_text('[workspace]\nmembers=["own"]\n')
        (self.root / 'rust-toolchain.toml').write_text('[toolchain]\nchannel="nightly-2026-07-30"\n')
        (self.root / 'own').mkdir()
        (self.root / 'own/Cargo.toml').write_text('[package]\nname="own"\nversion="0.1.0"\nlicense="GPL-3.0-or-later"\n')
        self.directory = self.root / 'target/cargo/registry/src/local/dep-1.0.0'
        self.directory.mkdir(parents=True)
        (self.directory / 'Cargo.toml').write_text('[package]\nname="dep"\nversion="1.0.0"\nlicense="untrusted extracted declaration"\n')
        (self.directory / 'LICENSE').write_text('untrusted extracted license')
        self.archive = self.root / 'target/cargo/registry/cache/local/dep-1.0.0.crate'
        self.archive.parent.mkdir(parents=True)
        self.install_archive(self.raw_archive())
        self.git('init', '-q'); self.commit()
        self.source = ci.source_identity(self.root)
        self.metadata = dict(packages=[
            dict(id='own-id', name='own', version='0.1.0', source=None, manifest_path=str(self.root / 'own/Cargo.toml')),
            dict(id='dep-id', name='dep', version='1.0.0', source=materials.REGISTRY, manifest_path=str(self.directory / 'Cargo.toml'))],
            workspace_members=['own-id'], resolve=dict(nodes=[
                dict(id='own-id', features=['default'], deps=[dict(name='dep', pkg='dep-id', dep_kinds=[dict(kind='build', target='cfg(unix)')])]),
                dict(id='dep-id', features=[], deps=[])]))
        self.output = self.root / 'target/materials.tar.gz'

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], stderr=subprocess.DEVNULL)

    def commit(self):
        self.git('add', '.')
        self.git('-c', 'user.name=Material Test', '-c', 'user.email=test@example.invalid',
                 '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')

    def raw_archive(self, version='1.0.0', notice=True, extra=None):
        files = {'dep-1.0.0/Cargo.toml': f'[package]\nname="dep"\nversion="{version}"\nlicense="MIT"\n'.encode()}
        if notice: files['dep-1.0.0/LICENSES/MIT.txt'] = b'published license bytes\n'
        files.update(extra or {})
        result = io.BytesIO()
        with tarfile.open(fileobj=result, mode='w:gz') as archive:
            for name, data in files.items():
                member = tarfile.TarInfo(name); member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        return result.getvalue()

    def install_archive(self, raw):
        self.archive.write_bytes(raw)
        (self.root / 'Cargo.lock').write_text(f'version=4\n[[package]]\nname="own"\nversion="0.1.0"\n'
            f'[[package]]\nname="dep"\nversion="1.0.0"\nsource="{materials.REGISTRY}"\nchecksum="{bundle.digest(raw)}"\n')

    def build(self, output=None, metadata=None):
        return materials.package(self.root, metadata or self.metadata, output or self.output, self.source, dict(cargo='fixture', python='fixture'))

    def test_exact_archives_nested_notices_and_graph_are_reproducible(self):
        result = self.build()
        self.assertEqual(materials.verify(self.root, self.output), result)
        second = self.root / 'target/second.tar.gz'; self.build(second)
        self.assertEqual(second.read_bytes(), self.output.read_bytes())
        registry = next(p for p in result['packages'] if p['source'])
        self.assertEqual(registry['declared_license'], 'MIT')
        self.assertEqual(result['notice_text_gaps'], [])
        files = {n: data for n, (_, data) in bundle.tar_members(self.output.read_bytes()).items()}
        self.assertEqual(files['archives/dep-1.0.0.crate'], self.archive.read_bytes())
        self.assertEqual(files['notices/dep-1.0.0/LICENSES/MIT.txt'], b'published license bytes\n')
        graph = json.loads(files['cargo-graph.json'])
        self.assertIn(dict(kind='build', target='cfg(unix)'), next(n for n in graph['nodes'] if n['id']=='own@0.1.0')['deps'][0]['dep_kinds'])
        self.assertFalse(result['image_release_eligible'])

    def test_missing_notice_is_a_visible_gap_not_a_license_pass(self):
        self.install_archive(self.raw_archive(notice=False)); self.commit(); self.source = ci.source_identity(self.root)
        result = self.build()
        self.assertEqual(result['notice_text_gaps'], ['dep@1.0.0'])
        self.assertFalse(result['image_release_eligible'])
        self.assertEqual(materials.verify(self.root, self.output), result)

    def test_missing_changed_or_linked_cache_cannot_publish(self):
        raw = self.archive.read_bytes()
        self.archive.write_bytes(raw + b'tampered')
        with self.assertRaises(ValueError): self.build()
        self.archive.unlink()
        with self.assertRaises(ValueError): self.build()
        other = self.archive.with_name('other.crate'); other.write_bytes(raw)
        self.archive.symlink_to(other)
        with self.assertRaises(ValueError): self.build()
        self.assertFalse(self.output.exists())

    def test_input_graph_version_and_source_mismatches_are_rejected(self):
        for mutation in ('node', 'edge', 'source', 'version'):
            metadata = copy.deepcopy(self.metadata)
            if mutation=='node': metadata['resolve']['nodes'].pop()
            elif mutation=='edge': metadata['resolve']['nodes'][0]['deps'][0]['dep_kinds'][0]['kind']='unknown'
            elif mutation=='source': metadata['packages'][1]['source']='git+https://example.invalid/repo'
            else: metadata['packages'][1]['version']='2.0.0'
            with self.subTest(mutation=mutation):
                with self.assertRaises(ValueError): self.build(metadata=metadata)
        (self.root / 'Cargo.toml').write_text('[workspace]\nmembers=[]\n')
        with self.assertRaises(ValueError): self.build()
        self.assertFalse(self.output.exists())

    def test_locked_wrong_version_or_escape_member_is_rejected(self):
        for raw in (self.raw_archive(version='2.0.0'), self.raw_archive(extra={'../escape': b'no'})):
            self.install_archive(raw); self.commit(); self.source = ci.source_identity(self.root)
            with self.assertRaises(ValueError): self.build()
        self.assertFalse(self.output.exists())

    def test_verify_rederives_materials_even_if_outer_hash_is_updated(self):
        self.build()
        payload = {n: data for n, (_, data) in bundle.tar_members(self.output.read_bytes()).items()}
        for name, replacement in [('NOTICE.txt', b'forged index'), ('archives/dep-1.0.0.crate', self.raw_archive(version='9.0.0')),
                                  ('inputs/.cargo/config.toml', None)]:
            changed = dict(payload)
            manifest = json.loads(changed['manifest.json'])
            if replacement is None:
                del changed[name]; del manifest['members'][name]
            else:
                changed[name] = replacement; manifest['members'][name] = bundle.identity(replacement)
            changed['manifest.json'] = bundle.encoded(manifest)
            path = self.root / f'target/forged-{len(name)}.tar.gz'; bundle.publish(changed, path)
            with self.assertRaises(ValueError): materials.verify(self.root, path)

    def test_existing_output_and_failed_creation_preserve_old_bytes(self):
        self.output.write_bytes(b'precious')
        with self.assertRaises(FileExistsError): self.build()
        self.assertEqual(self.output.read_bytes(), b'precious')
        self.assertEqual(list(self.output.parent.glob('.prima-evidence-*')), [])


if __name__ == '__main__':
    unittest.main()
