#!/usr/bin/env python3
"""Exercise evidence failure/identity rules with real child processes, no QEMU."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
import ci


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='prima-ci-test-')
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        (self.root / '.gitignore').write_text('target/\n')
        (self.root / 'rust-toolchain.toml').write_text('[toolchain]\nchannel="nightly-2026-07-30"\n')
        (self.root / 'source.txt').write_text('original')
        for args in [('init', '-q'), ('add', '.'), ('-c', 'user.name=CI Test', '-c', 'user.email=ci@example.invalid', 'commit', '-qm', 'fixture')]:
            subprocess.run(['git', '-C', str(self.root), *args], check=True)

    def execute(self, scripts, **options):
        return ci.run_group('host', [ci.step([sys.executable, '-c', source]) for source in scripts],
                            root=self.root, versions=False, **options)

    def test_failure_stops_commands_and_never_relabels_old_artifacts(self):
        directory = self.root / 'target/prima-os'
        directory.mkdir(parents=True)
        (directory / 'old.img').write_bytes(b'old evidence')
        result, path = self.execute(['raise SystemExit(7)', 'open("target/should-not-run", "w").close()'])
        self.assertEqual(result['status'], 'failed')
        self.assertEqual([s['exit_code'] for s in result['steps']], [7])
        self.assertEqual(result['artifacts'], [])
        self.assertFalse((self.root / 'target/should-not-run').exists())
        self.assertEqual(json.loads(path.read_text())['status'], 'failed')

    def test_success_records_only_new_outputs_and_preserves_earlier_results(self):
        failed, old_path = self.execute(['raise SystemExit(3)'])
        directory = self.root / 'target/prima-os'
        directory.mkdir(parents=True)
        (directory / 'old.img').write_bytes(b'old')
        (directory / 'gate-probe').mkdir()
        (directory / 'gate-probe/old.img').write_bytes(b'old probe')
        result, new_path = self.execute([
            'from pathlib import Path; Path("target/prima-os/new.img").write_bytes(b"new"); '
            'Path("target/prima-os/gate-probe/new.img").write_bytes(b"probe")'])
        self.assertEqual(failed['status'], 'failed')
        self.assertNotEqual(old_path, new_path)
        self.assertEqual(json.loads(old_path.read_text())['status'], 'failed')
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['artifacts'], [
            dict(path='target/prima-os/gate-probe/new.img', size=5, sha256=hashlib.sha256(b'probe').hexdigest()),
            dict(path='target/prima-os/new.img', size=3, sha256=hashlib.sha256(b'new').hexdigest())])

    def test_environment_pollution_is_removed_for_each_command(self):
        with patch.dict(os.environ, PRIMA_GATE_CASE='G4', PRIMA_STAGE6_DOMAINS='1',
                        PRIMA_LINUX_ELF='/wrong/elf', RUSTFLAGS='wrong-flags', CARGO_BUILD_JOBS='99',
                        LAKE_NO_CACHE='false', MATHLIB_NO_CACHE_ON_UPDATE='0'):
            keys = [*ci.SELECTORS, 'CARGO_BUILD_JOBS', 'RUSTUP_TOOLCHAIN',
                    'LAKE_NO_CACHE', 'MATHLIB_NO_CACHE_ON_UPDATE']
            result, path = self.execute([f'import json, os; print(json.dumps({{k:os.environ[k] for k in {keys!r} if k in os.environ}}))'])
        observed = json.loads((path.parent / result['steps'][0]['log']).read_text())
        for key in ci.SELECTORS:
            self.assertNotIn(key, observed)
        self.assertEqual(observed['CARGO_BUILD_JOBS'], '2')
        self.assertEqual(observed['RUSTUP_TOOLCHAIN'], 'nightly-2026-07-30')
        self.assertEqual(observed['LAKE_NO_CACHE'], 'true')
        self.assertEqual(observed['MATHLIB_NO_CACHE_ON_UPDATE'], '1')
        gate = ci.commands('gate')
        self.assertEqual(gate[0]['env'], {})
        self.assertEqual(len(gate), 9)
        self.assertEqual([g['env'] for g in gate[-5:]],
                         [{'PRIMA_GATE_CASE': c} for c in ('G4', 'G5', 'N1', 'I1', 'C1')])

    def test_guest_logs_survive_overwrite_and_failure_without_collecting_stale_logs(self):
        directory = self.root / 'target/prima-os'
        directory.mkdir(parents=True)
        (directory / 'stale.log').write_bytes(b'old')
        scripts = ['from pathlib import Path; Path("target/prima-os/guest.log").write_bytes(b"first"); '
                   'Path("target/prima-os/link.log").symlink_to("../../source.txt")',
                   'from pathlib import Path; Path("target/prima-os/guest.log").write_bytes(b"failure"); raise SystemExit(7)']
        result, path = self.execute(scripts)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['artifacts'], [])
        for record, expected in zip(result['steps'], (b'first', b'failure')):
            self.assertEqual(len(record['traces']), 1)
            trace = record['traces'][0]
            self.assertEqual(trace['source'], 'target/prima-os/guest.log')
            self.assertEqual((path.parent / trace['path']).read_bytes(), expected)
            self.assertEqual(trace['sha256'], hashlib.sha256(expected).hexdigest())
            self.assertEqual(trace['size'], len(expected))
        self.assertEqual([s['status'] for s in result['steps']], ['passed', 'failed'])

    def test_guest_log_archive_error_cannot_pass(self):
        (self.root / 'target/prima-os').mkdir(parents=True)
        with patch('ci.shutil.copyfile', side_effect=OSError('archive write denied')):
            result, _ = self.execute([
                'from pathlib import Path; Path("target/prima-os/guest.log").write_bytes(b"new")'])
        self.assertEqual(result['status'], 'failed')
        self.assertIn('archive write denied', result['error'])
        self.assertEqual(result['steps'][0]['traces'], [])
        self.assertEqual(result['steps'][0]['status'], 'passed')
        self.assertEqual(result['steps'][0]['exit_code'], 0)

    def test_source_change_and_dirty_ci_checkout_cannot_pass(self):
        result, _ = self.execute(['open("source.txt", "w").write("changed")'])
        self.assertEqual(result['status'], 'failed')
        self.assertIn('source changed', result['error'])
        result, _ = self.execute(['raise SystemExit(0)'], require_clean=True)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['steps'], [])

    def test_timeout_terminates_child_group(self):
        source = ('import subprocess, sys, time; from pathlib import Path; '
                  'p=subprocess.Popen([sys.executable,"-c","import time; time.sleep(60)"]); '
                  'Path("target/child.pid").write_text(str(p.pid)); time.sleep(60)')
        result, _ = self.execute([source], budget=0.5)
        self.assertEqual(result['status'], 'timed_out')
        self.assertEqual(result['steps'][0]['exit_code'], 124)
        child = int((self.root / 'target/child.pid').read_text())
        status = Path(f'/proc/{child}/stat')
        if status.exists():
            self.assertEqual(status.read_text().split()[2], 'Z')  # dead; init owns reaping

    def test_second_group_and_corrupt_cache_fail_closed(self):
        (self.root / 'target').mkdir()
        with (self.root / 'target/ci-run.lock').open('a+b') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result, _ = self.execute(['raise SystemExit(0)'])
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['steps'], [])
        cached = self.root / 'target/input'
        cached.write_bytes(b'corrupt')
        with patch('urllib.request.urlopen') as network:
            with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                ci.checked_download('https://example.invalid/input', cached, '0' * 64)
        network.assert_not_called()
        self.assertEqual(cached.read_bytes(), b'corrupt')

    def test_checkout_rejects_dirty_input_without_resetting_it(self):
        destination = self.root / 'target/snapshot'
        ci.checked_checkout(self.root.as_uri(), destination, ci.git_output(self.root, 'rev-parse', 'HEAD'))
        (destination / 'source.txt').write_text('local changes')
        with self.assertRaisesRegex(ValueError, 'mismatch'):
            ci.checked_checkout(self.root.as_uri(), destination, ci.git_output(self.root, 'rev-parse', 'HEAD'))
        self.assertEqual((destination / 'source.txt').read_text(), 'local changes')

    def test_formal_missing_dependencies_fail_without_downloading(self):
        lock = self.root / 'lean/refinement'
        lock.mkdir(parents=True)
        (lock / 'toolchain.lock.json').write_text(json.dumps({'binaries': {}}))
        (lock / 'lake-manifest.json').write_text(json.dumps({'packages': [dict(
            type='git', name='mathlib', url='https://example.invalid/mathlib', rev='0' * 40)]}))
        (self.root / 'scripts').mkdir()
        (self.root / 'scripts/ci-inputs.lock.json').write_text('{}')
        with patch('subprocess.run') as command:
            with self.assertRaisesRegex(ValueError, 'missing formal dependency'):
                ci.input_identity('formal', self.root)
        command.assert_not_called()
        backend = self.root / 'target/backend'
        backend.mkdir(parents=True)
        (backend / 'Main.lean').write_text('public import Mathlib.Data.Nat.Basic\nimport Aeneas.Std\n')
        cached = backend / '.lake'
        cached.mkdir()
        (cached / 'Ignored.lean').write_text('import Mathlib\n')
        self.assertEqual(ci.formal_cache_roots(backend), ['Mathlib.Data.Nat.Basic'])

    def test_preparation_builds_nested_bootloader_inputs_before_offline_commands(self):
        (self.root / 'rust-toolchain.toml').write_text(
            '[toolchain]\nchannel="nightly-2026-07-30"\ncomponents=["rust-src"]\n'
            'targets=["x86_64-unknown-none", "x86_64-unknown-uefi"]\n')
        (self.root / 'scripts').mkdir()
        (self.root / 'scripts/ci-inputs.lock.json').write_text('{}')
        for group in ('host', 'lint'):
            with self.subTest(group=group), patch('subprocess.run') as command:
                ci.prepare(group, self.root)
                self.assertEqual([call.args[0] for call in command.call_args_list][1:], [
                    ['cargo', 'fetch', '--locked'],
                    ['cargo', 'build', '--locked', '--package', 'bootloader'],
                ])
                for call in command.call_args_list:
                    self.assertEqual(call.kwargs['env']['CARGO_NET_OFFLINE'], 'false')
                    self.assertEqual(call.kwargs['cwd'], self.root)
            self.assertEqual(ci.environment(self.root)['CARGO_NET_OFFLINE'], 'true')
        with patch('subprocess.run') as command:
            ci.prepare('docs', self.root)
        command.assert_not_called()


if __name__ == '__main__':
    unittest.main()
