#!/usr/bin/env python3
"""Re-extract production Rust, reject stale Lean, then check the refinement proofs.

Prerequisites are pinned in lean/refinement/toolchain.lock.json. No downloads.
Use --regenerate after reviewed source changes; generated files are never hand-edited.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PROJECT = ROOT / "lean/refinement"
TOOLS = ROOT / "target/aeneas"
LOCK = json.loads((PROJECT / "toolchain.lock.json").read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, cwd=ROOT):
    subprocess.run([str(arg) for arg in args], cwd=cwd, check=True)


def extract(source, destination, llbc):
    run([TOOLS / "charon", "rustc", "--preset=aeneas", "--sysroot", "default",
         "--error-on-warnings", "--start-from", "crate::make_handle",
         "--start-from", "crate::next_generation", "--dest-file", llbc,
         "--", "--crate-type", "lib", "--crate-name", "prima_handle_source",
         "--edition", "2024", source])
    run([TOOLS / "aeneas", "-backend", "lean", "-dest", destination,
         "-subdir", "Refinement/Generated", "-namespace", "RustHandle",
         "-split-files", "-abort-on-error", "-warnings-as-errors", llbc])


def mutation_check():
    """Prove the pipeline rejects a real Rust bug, with production files untouched."""
    with tempfile.TemporaryDirectory(prefix="prima-handle-mutation-", dir=ROOT / "target") as tmp:
        scratch = Path(tmp)
        rust = (ROOT / "kernel/src/handle.rs").read_text()
        needle = "self.generation.checked_add(1)"
        if rust.count(needle) != 1:
            raise SystemExit("mutation fixture needs review after source changes")
        (scratch / "mutated_handle.rs").write_text(rust.replace(needle, "Some(1)"))
        wrapper = (PROJECT / "source.rs").read_text().replace(
            '../../kernel/src/handle.rs', 'mutated_handle.rs')
        (scratch / "source.rs").write_text(wrapper)
        extract((scratch / "source.rs").relative_to(ROOT), scratch, scratch / "mutant.llbc")
        (scratch / "Refinement/Handle.lean").write_bytes((PROJECT / "Refinement/Handle.lean").read_bytes())
        (scratch / "Refinement.lean").write_bytes((PROJECT / "Refinement.lean").read_bytes())
        (scratch / "lean-toolchain").write_bytes((PROJECT / "lean-toolchain").read_bytes())
        (scratch / "lakefile.lean").write_text(
            'import Lake\nopen Lake DSL\n'
            f'require formal from "{ROOT / "lean"}"\n'
            f'require aeneas from "{TOOLS / "backends/lean"}"\n'
            'package mutation where\n'
            '  leanOptions := #[⟨`autoImplicit, false⟩, ⟨`warningAsError, true⟩]\n'
            '@[default_target] lean_lib Refinement\n')
        (scratch / ".lake").mkdir()
        (scratch / ".lake/packages").symlink_to(TOOLS / "backends/lean/.lake/packages")
        subprocess.run(["elan", "run", LOCK["lean"], "lake", "update"], cwd=scratch,
                       env=dict(os.environ, MATHLIB_NO_CACHE_ON_UPDATE="1"), check=True)
        result = subprocess.run(["elan", "run", LOCK["lean"], "lake", "build"],
                                cwd=scratch, capture_output=True, text=True)
        log = result.stdout + result.stderr
        (ROOT / "target/handle-refinement-mutation.log").write_text(log)
        if result.returncode == 0 or "error: Refinement/Handle.lean:" not in log:
            raise SystemExit("mutation was not rejected by a refinement proof; inspect mutation log")
        print("PASS source mutation (generation reset to 1) rejected by Lean proof")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--regenerate", action="store_true")
    parser.add_argument("--mutation-test", action="store_true")
    args = parser.parse_args()
    for name, expected in LOCK["binaries"].items():
        binary = TOOLS / name
        if not binary.is_file() or digest(binary) != expected:
            raise SystemExit(f"missing or unpinned tool: {binary}; see toolchain.lock.json")
    with tempfile.TemporaryDirectory(prefix="prima-handle-refinement-", dir=ROOT / "target") as tmp:
        scratch = Path(tmp)
        llbc = scratch / "handle.llbc"
        # Relative paths are intentional: generated source spans remain portable.
        extract("lean/refinement/source.rs", scratch, llbc)
        generated = sorted((scratch / "Refinement/Generated").glob("*.lean"))
        if [p.name for p in generated] != ["Funs.lean", "Types.lean"]:
            raise SystemExit("unexpected extraction units (including potential external stubs)")
        manifest = {
            "schema_version": 1,
            "toolchain_lock_sha256": digest(PROJECT / "toolchain.lock.json"),
            "inputs": {name: digest(ROOT / name) for name in
                       ["kernel/src/handle.rs", "lean/refinement/source.rs"]},
            "generated": {p.name: digest(p) for p in generated},
        }
        manifest_path = PROJECT / "source-manifest.json"
        if args.regenerate:
            for path in generated:
                shutil.copyfile(path, PROJECT / "Refinement/Generated" / path.name)
            manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
        else:
            if not manifest_path.is_file() or json.loads(manifest_path.read_text()) != manifest:
                raise SystemExit("source/extraction manifest is stale; review and use --regenerate")
            for path in generated:
                if path.read_bytes() != (PROJECT / "Refinement/Generated" / path.name).read_bytes():
                    raise SystemExit(f"generated Lean differs: {path.name}; review and regenerate")
    run(["elan", "run", LOCK["lean"], "lake", "build"], PROJECT)
    audit = subprocess.run(["elan", "run", LOCK["lean"], "lake", "env", "lean", "Checks.lean"],
                           cwd=PROJECT, capture_output=True, text=True, check=True).stdout
    print(audit, end="")
    expected = set(re.findall(r"^theorem (\w+)",
                              (PROJECT / "Refinement/Handle.lean").read_text(), re.MULTILINE))
    audited = set(re.findall(r"'Refinement.Handle.(\w+)'", audit))
    if audited != expected:
        raise SystemExit("axiom audit does not cover every refinement theorem")
    allowed = {"propext", "Classical.choice", "Quot.sound"}
    for group in re.findall(r"depends on axioms: \[(.*?)\]", audit):
        if set(group.split(", ")) - allowed:
            raise SystemExit("unexpected proof axiom dependency")
    if args.mutation_test:
        mutation_check()
    print("PASS Rust source extraction, generated-code freshness, refinement and axiom audit")


if __name__ == "__main__":
    main()
