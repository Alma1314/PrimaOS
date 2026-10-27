#!/usr/bin/env python3
"""Run the pinned compiler against the isolated, locally patched source sysroot."""
import os
import sys

args = []
skip = False
for arg in sys.argv[1:]:
    if skip:
        skip = False
    elif arg == "--sysroot":
        skip = True
    elif not arg.startswith("--sysroot="):
        args.append(arg)
# Build-std has several independent libc dependency edges. Redirect the
# compiler input for the one locked crate, so all edges use the same explicit
# Prima ABI bindings. The overlay digest is part of the Cargo fingerprint.
if "PRIMA_LIBC_SOURCE" in os.environ and "--crate-name" in args:
    if args[args.index("--crate-name") + 1] == "libc":
        for index, arg in enumerate(args):
            if arg.endswith("/libc-0.2.189/src/lib.rs"):
                args[index] = os.environ["PRIMA_LIBC_SOURCE"]
compiler = os.environ["PRIMA_REAL_RUSTC"]
os.execv(compiler, [compiler, *args, "--sysroot", os.environ["PRIMA_RUST_SYSROOT"]])
