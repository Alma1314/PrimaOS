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
compiler = os.environ["PRIMA_REAL_RUSTC"]
os.execv(compiler, [compiler, *args, "--sysroot", os.environ["PRIMA_RUST_SYSROOT"]])
