#!/usr/bin/env python3
"""Host checks for the musl port, without QEMU or a sysroot rebuild."""
import hashlib
from pathlib import Path
import subprocess
import tempfile
import unittest
import sys

sys.dont_write_bytecode = True
import build

HERE = Path(__file__).resolve().parent


class PortTests(unittest.TestCase):
    def test_crt_requires_the_versioned_adapter_anchor(self):
        with tempfile.TemporaryDirectory(prefix="prima-crt-contract-") as directory:
            root = Path(directory)
            shim = root / "shim.c"
            stubs = "int main(void){return 0;} void prima_exec_restore(void){} int __libc_start_main(void){return 0;}\n"
            command = ["cc", "-nostdlib", "-fPIC", "-fno-stack-protector", "-shared", "-Wl,--no-undefined",
                       str(HERE / "../c/prima_crt.c"), str(shim), "-o", str(root / "crt.so")]
            # Link-only fixture: this shared host object is never executed or
            # advertised as a Prima dynamic library.
            shim.write_text(stubs + "const unsigned prima_libc_contract_v0_0[11]={0};\n")
            rejected = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("prima_libc_contract_v1_0", rejected.stderr)
            shim.write_text(stubs + '#include "' + str(HERE / "../c/prima_contract.h") + '"\n'
                            'const volatile uint32_t prima_libc_contract_v1_0[11]=PRIMA_CONTRACT_WORDS;\n')
            subprocess.run(command, check=True)

    def test_adapter_and_cancellation_argument_boundary(self):
        with tempfile.TemporaryDirectory(prefix="prima-musl-test-") as directory:
            for name, sources in [("adapter", ["test_adapter.c"]), ("cp", ["test_cp.c", "patches/syscall_cp.s"])]:
                binary = str(Path(directory) / name)
                subprocess.run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", *[str(HERE / source) for source in sources], "-o", binary], check=True)
                subprocess.run([binary], check=True, timeout=10)

    def test_snapshot_identity_and_dirty_rejection(self):
        with tempfile.TemporaryDirectory(prefix="prima-musl-lock-") as directory:
            root = Path(directory)
            def git(*args):
                return build.output("git", "-C", directory, *args)
            git("init", "-q")
            for name in ["COPYRIGHT", "VERSION"]:
                (root / name).write_text(name)
            (root / ".gitignore").write_text("obj/\n")
            git("add", ".")
            git("-c", "user.name=Port Test", "-c", "user.email=port@example.invalid", "commit", "-qm", "fixture")
            lock = {"commit": git("rev-parse", "HEAD"), "license_file": "COPYRIGHT",
                    "license_sha256": hashlib.sha256(b"COPYRIGHT").hexdigest(),
                    "version_file_sha256": hashlib.sha256(b"VERSION").hexdigest()}
            build.verify_snapshot(root, lock)
            (root / "obj").mkdir()
            (root / "obj/host.o").write_bytes(b"ignored host object")
            build.verify_snapshot(root, lock)
            exported = root / "obj/exported"
            build.export_snapshot(root, exported)
            self.assertTrue((exported / "VERSION").is_file())
            self.assertFalse((exported / "obj").exists())
            with self.assertRaisesRegex(ValueError, "commit"):
                build.verify_snapshot(root, {**lock, "commit": "0" * 40})
            with self.assertRaisesRegex(ValueError, "hash"):
                build.verify_snapshot(root, {**lock, "license_sha256": "0" * 64})
            (root / "unexpected.h").write_text("dirty")
            with self.assertRaisesRegex(ValueError, "dirty"):
                build.verify_snapshot(root, lock)
            (root / "unexpected.h").unlink()
            (root / "VERSION").write_text("changed")
            with self.assertRaisesRegex(ValueError, "dirty"):
                build.verify_snapshot(root, lock)

    def test_incomplete_install_never_hits_cache(self):
        with tempfile.TemporaryDirectory(prefix="prima-musl-cache-") as directory:
            prefix = Path(directory)
            (prefix / ".stamp").write_text("current\n")
            self.assertFalse(build.cache_valid(prefix, "current"))
            for name in ["lib/libc.a", "include/stdio.h", "include/pthread.h", "include/bits/syscall.h"]:
                path = prefix / name
                path.parent.mkdir(exist_ok=True, parents=True)
                path.write_bytes(b"installed")
            self.assertTrue(build.cache_valid(prefix, "current"))
            self.assertFalse(build.cache_valid(prefix, "stale"))
            (prefix / "lib/libc.a").write_bytes(b"")
            self.assertFalse(build.cache_valid(prefix, "current"))

    def test_archive_rejects_linux_or_unlisted_raw_traps(self):
        allowed = "restore.lo:     file format elf64-x86-64\n mov $0x3a,%eax\n syscall\nclone.lo:     file format elf64-x86-64\n mov $0x30,%al\n syscall\n"
        build.audit_disassembly(allowed)
        for bad in [allowed.replace("0x3a", "0xf"), allowed + "unknown.lo:     file format elf64-x86-64\n syscall\n", allowed + " int $0x80\n", allowed + " mov $0x31,%al\n syscall\n"]:
            with self.assertRaises(ValueError):
                build.audit_disassembly(bad)


if __name__ == "__main__":
    unittest.main()
