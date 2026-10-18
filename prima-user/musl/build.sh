#!/bin/sh
# M3：从锁定快照构建 Prima 的静态 musl 基座。
#
# 前置：target/musl-1.2.5 是 M0 锁定的 musl 1.2.5 快照（见
# docs/superpowers/compat/musl-snapshot.lock.toml）。产物在 target/musl-prefix，
# 由补丁内容与快照 commit 的哈希做缓存标记；不变则直接复用。
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
snapshot="$root/target/musl-1.2.5"
work="$root/target/musl-prima"
build="$root/target/musl-build"
prefix="$root/target/musl-prefix"

stamp_src="$here/patches/syscall_arch.h $here/patches/__set_thread_area.s"
stamp=$(
    {
        git -C "$snapshot" rev-parse HEAD 2>/dev/null || echo unknown
        cat $stamp_src
    } | sha256sum | cut -d' ' -f1
)

if [ -f "$prefix/.stamp" ] && [ "$(cat "$prefix/.stamp")" = "$stamp" ]; then
    exit 0
fi

if [ ! -d "$snapshot" ]; then
    echo "musl snapshot missing: $snapshot" >&2
    echo "restore it from docs/superpowers/compat/musl-snapshot.lock.toml" >&2
    exit 1
fi

rm -rf "$work" "$build" "$prefix"
cp -a "$snapshot" "$work"
cp "$here/patches/syscall_arch.h" "$work/arch/x86_64/syscall_arch.h"
cp "$here/patches/__set_thread_area.s" "$work/src/thread/x86_64/__set_thread_area.s"

mkdir -p "$build"
cd "$build"
CC="clang --target=x86_64-unknown-none" \
CFLAGS="-O2 -fno-stack-protector -fno-omit-frame-pointer -fPIC" \
    "$work/configure" --target=x86_64 --disable-shared --enable-static --prefix="$prefix"
make -j"$(nproc)" AR=llvm-ar RANLIB="llvm-ar s"
make install AR=llvm-ar RANLIB="llvm-ar s"
echo "$stamp" > "$prefix/.stamp"
