#!/usr/bin/env python3
"""Offline evidence/source binding, tamper rejection and deterministic publication."""
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
import ci
import package_evidence as pack


class PackageTests(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix='prima-package-test-')
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        for name in (*pack.LICENSES, *pack.MPL_FILES):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((pack.ROOT / name).read_bytes())
        (self.root / '.gitignore').write_text('target/\n')
        (self.root / 'Cargo.toml').write_text('[package]\nname="fixture"\nversion="0.1.0"\nlicense="GPL-3.0-or-later"\n')
        (self.root / 'source.txt').write_text('original')
        (self.root / 'executable').write_text('#!/bin/sh\nexit 0\n')
        (self.root / 'executable').chmod(0o755)
        (self.root / 'license-link').symlink_to('LICENSE')
        self.git('init', '-q')
        self.commit()
        self.source = ci.source_identity(self.root)
        self.evidence = self.root / 'target/evidence'
        self.evidence.mkdir(parents=True)
        self.originals = {}
        for group in pack.GROUPS:
            directory = self.evidence / group
            directory.mkdir()
            (directory / '00.log').write_bytes(b'passed command\n')
            (directory / 'traces').mkdir()
            trace = directory / 'traces/guest.log'
            trace.write_bytes(b'raw guest receipt\n')
            manifest = dict(schema_version=1, group=group, status='passed', source=self.source,
                            steps=[dict(status='passed', exit_code=0, argv=['cargo', 'test'], log='00.log',
                                        traces=[dict(path='traces/guest.log', source='target/prima-os/guest.log',
                                                     **pack.identity(trace.read_bytes()))])],
                            artifacts=[dict(path='target/prima-os/final.img', size=5, sha256='a' * 64)])
            self.originals[group] = manifest
            self.write(group, manifest)
        self.output = self.root / 'target/bundle.tar.gz'

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], stderr=subprocess.DEVNULL)

    def commit(self):
        self.git('add', '.')
        self.git('-c', 'user.name=Package Test', '-c', 'user.email=test@example.invalid',
                 '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')

    def write(self, group, manifest):
        (self.evidence / group / 'result.json').write_bytes(pack.encoded(manifest))

    def build(self, output=None, revision=None):
        return pack.package(self.root, self.evidence, revision or self.source['revision'], output or self.output)

    def test_bundle_binds_git_source_and_traces_and_is_deterministic(self):
        raw, source, _ = pack.snapshot(self.root, 'HEAD')
        self.assertEqual(source, self.source)  # Includes executable bits and a relative symlink.
        (self.root / 'source.txt').write_text('uncommitted change')
        (self.root / 'private-untracked').write_text('must not be packaged')
        (self.evidence / 'unreferenced-secret').write_text('must not be packaged')
        first = self.build()
        other = self.root / 'target/second.tar.gz'
        self.build(other)
        self.assertEqual(self.output.read_bytes(), other.read_bytes())
        self.assertEqual(pack.verify(self.root, self.output), first)
        files = {name: data for name, (_, data) in pack.tar_members(self.output.read_bytes()).items()}
        self.assertEqual(files['source.tar'], raw)
        self.assertEqual(first['source'], self.source)
        self.assertEqual(set(first['groups']), pack.GROUPS)
        self.assertEqual(sum(g['traces'] for g in first['groups'].values()), 9)
        self.assertFalse(first['image_release_eligible'])
        self.assertFalse(any(name.endswith('.img') for name in files))
        self.assertNotIn('unreferenced-secret', files)
        self.assertEqual(len(first['licenses']['texts']), 3)
        self.assertEqual(len(first['licenses']['file_declarations']), 3)

    def test_failed_dirty_mixed_or_duplicate_manifests_cannot_publish(self):
        for change in ('schema', 'status', 'dirty', 'revision', 'hash', 'empty', 'step', 'exit', 'argv', 'duplicate', 'unknown', 'trace_type', 'artifact_type'):
            with self.subTest(change=change):
                manifest = copy.deepcopy(self.originals['linux'])
                if change == 'schema': manifest['schema_version'] = 2
                elif change == 'status': manifest['status'] = 'interrupted'
                elif change == 'dirty': manifest['source']['dirty'] = True
                elif change == 'revision': manifest['source']['revision'] = 'f' * 40
                elif change == 'hash': manifest['source']['files_sha256'] = '0' * 64
                elif change == 'empty': manifest['steps'] = []
                elif change == 'step': manifest['steps'][0]['status'] = 'failed'
                elif change == 'exit': manifest['steps'][0]['exit_code'] = 130
                elif change == 'argv': manifest['steps'][0]['argv'] = []
                elif change == 'duplicate': manifest['group'] = 'host'
                elif change == 'unknown': manifest['group'] = 'unknown'
                elif change == 'trace_type': manifest['steps'][0]['traces'] = [None]
                else: manifest['artifacts'] = [None]
                self.write('linux', manifest)
                with self.assertRaises(ValueError): self.build()
                self.assertFalse(self.output.exists())
                self.write('linux', self.originals['linux'])
        (self.evidence / 'linux/result.json').unlink()
        with self.assertRaises(ValueError): self.build()
        self.assertFalse(self.output.exists())

    def test_missing_changed_or_unsafe_traces_are_rejected(self):
        log = self.evidence / 'linux/00.log'
        log.unlink()
        with self.assertRaises(ValueError): self.build()
        log.write_bytes(b'passed command\n')
        path = self.evidence / 'linux/traces/guest.log'
        path.write_bytes(b'changed')
        with self.assertRaises(ValueError): self.build()
        path.unlink()
        with self.assertRaises(ValueError): self.build()
        path.symlink_to(self.evidence / 'host/traces/guest.log')
        with self.assertRaises(ValueError): self.build()
        path.unlink()
        path.write_bytes(b'raw guest receipt\n')
        directory = self.evidence / 'linux/traces'
        moved = self.evidence / 'linux/saved-traces'
        directory.rename(moved)
        directory.symlink_to(moved, target_is_directory=True)
        with self.assertRaises(ValueError): self.build()
        directory.unlink()
        moved.rename(directory)
        for bad in ('../00.log', '/tmp/escape', 'traces/../00.log', 'traces//guest.log', 'traces\\guest.log'):
            manifest = copy.deepcopy(self.originals['linux'])
            manifest['steps'][0]['traces'][0]['path'] = bad
            self.write('linux', manifest)
            with self.assertRaises(ValueError): self.build()
            self.assertFalse(self.output.exists())

    def test_changed_license_or_git_revision_cannot_reuse_old_evidence(self):
        (self.root / 'source.txt').write_text('new committed source')
        self.commit()
        with self.assertRaises(ValueError): self.build(revision='HEAD')
        (self.root / 'LICENSE').write_text('changed license')
        self.commit()
        with self.assertRaisesRegex(ValueError, 'license text'): self.build(revision='HEAD')
        (self.root / 'LICENSE').unlink()
        self.commit()
        with self.assertRaisesRegex(ValueError, 'license text'): self.build(revision='HEAD')

    def test_existing_output_is_preserved_and_temporary_file_removed(self):
        self.output.write_bytes(b'precious')
        with self.assertRaises(FileExistsError): self.build()
        self.assertEqual(self.output.read_bytes(), b'precious')
        self.assertEqual(list(self.output.parent.glob('.prima-evidence-*')), [])

    def test_verify_rejects_tampered_members_and_forged_image_scope(self):
        self.build()
        payload = {name: data for name, (_, data) in pack.tar_members(self.output.read_bytes()).items()}
        def archive(files):
            with tarfile.open(self.output, 'w:gz') as output:
                for name, data in files.items():
                    member = tarfile.TarInfo(name); member.size = len(data)
                    output.addfile(member, io.BytesIO(data))
        changed = dict(payload)
        changed['evidence/linux/traces/guest.log'] = b'tampered'
        archive(changed)
        with self.assertRaises(ValueError): pack.verify(self.root, self.output)
        changed = dict(payload)
        changed['source.tar'] = b'forged source snapshot'
        manifest = json.loads(changed['manifest.json']); manifest['members']['source.tar'] = pack.identity(changed['source.tar'])
        changed['manifest.json'] = pack.encoded(manifest)
        archive(changed)
        with self.assertRaises(ValueError): pack.verify(self.root, self.output)
        changed = dict(payload)
        manifest = json.loads(changed['manifest.json']); manifest['image_release_eligible'] = True
        changed['manifest.json'] = pack.encoded(manifest)
        archive(changed)
        with self.assertRaises(ValueError): pack.verify(self.root, self.output)
        changed = dict(payload)
        changed['extra.img'] = b'not allowed'
        manifest = json.loads(changed['manifest.json']); manifest['members']['extra.img'] = pack.identity(changed['extra.img'])
        changed['manifest.json'] = pack.encoded(manifest)
        archive(changed)
        with self.assertRaises(ValueError): pack.verify(self.root, self.output)

    def test_archive_paths_types_and_limits_are_checked_without_extraction(self):
        def raw(names, kind=tarfile.REGTYPE):
            result = io.BytesIO()
            with tarfile.open(fileobj=result, mode='w') as output:
                for name in names:
                    info = tarfile.TarInfo(name); info.type = kind
                    info.size = 2 if kind == tarfile.REGTYPE else 0
                    info.linkname = '/outside'
                    output.addfile(info, io.BytesIO(b'ab'))
            return result.getvalue()
        for data in (raw(['../escape']), raw(['/escape']), raw(['one', 'one']), raw(['link'], tarfile.SYMTYPE)):
            with self.assertRaises(ValueError): pack.tar_members(data)
        for name, value in (('MAX_FILE', 1), ('MAX_TOTAL', 1), ('MAX_MEMBERS', 1)):
            with patch.object(pack, name, value):
                with self.assertRaises(ValueError): pack.tar_members(raw(['one', 'two']))
        (self.root / 'escape-link').symlink_to('../outside')
        self.commit()
        with self.assertRaises(ValueError): pack.snapshot(self.root, 'HEAD')


if __name__ == '__main__':
    unittest.main()
