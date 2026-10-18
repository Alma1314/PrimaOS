# 阶段 5：musl M0 依赖盘点

**日期**：2026-10-18。**范围**：锁定 musl 1.2.5 快照，按首版 profile 的 sbase `echo`
闭包做“公开符号 → 内部函数 → Linux syscall → Prima 机制”追溯，并登记阻塞、测试 ID 与
决策。本片**不移植 musl、不修改内核、不新增 QEMU 证据**，也不把任何 API 升为
`supported`/`partial`。

配套证据：[快照锁](../compat/musl-snapshot.lock.toml)、[兼容状态台账](../compat/compat-state.md)、
[符号追溯表](../compat/symbol-trace.md)。上位计划：[musl 移植计划](../plans/2026-10-18-musl-port-plan.md)。

## 1. 快照与目标锁定

| 项 | 观测值 | 核验命令 |
|---|---|---|
| musl 版本 | `1.2.5`（tag `v1.2.5`） | `cat target/musl-1.2.5/VERSION` |
| commit | `0784374d561435f7c787a555aeab8ede699ed298` | `git -C target/musl-1.2.5 rev-parse HEAD` |
| 两个镜像的 tag 一致性 | `ifduyue/musl` 与 `kraj/musl` 均为同一 commit | `curl -s https://api.github.com/repos/<mirror>/git/ref/tags/v1.2.5` |
| 归档 SHA-256 | `83ff394502d1c334b040ea9bc66ec48bba453585e25b05f4bde3741d8245d883` | `sha256sum target/musl-v1.2.5.tar.gz` |
| `COPYRIGHT` SHA-256 | `f9bc4423732350eb0b3f7ed7e91d530298476f8fec0c6c427a1c04ade22655af` | `sha256sum target/musl-1.2.5/COPYRIGHT` |
| `VERSION` SHA-256 | `fe2b7c237b5a6e42818aa3e4f7fc5820f49c32289c32c96260ef89a8c8f6d3d7` | `sha256sum target/musl-1.2.5/VERSION` |

上游 `git.musl-libc.org` 在本机直连超时，未直接验证；实际 clone 走
`https://ghfast.top/https://github.com/ifduyue/musl.git`，并用 `kraj/musl` 的 tag ref
互证 commit。**commit 是权威身份**；GitHub 归档哈希只记录传输工件。源码与归档位于
git-ignored 的 `target/`，不入库。

sbase 仍为 profile v0 锁定的 `c546c3a5724c81cee9a11d816a38ccdf17472129`。本次重新
clone 并逐项核验 `LICENSE`、`echo.c`、`libutil/{putword,fshut,eprintf}.c`、`util.h`、
`arg.h`、`compat.h` 的 SHA-256，**全部与 profile v0 一致**。

## 2. 方法

1. 按锁定 commit 阅读 musl 源，沿 `echo` 的实际对象闭包追踪：`echo.o`、`putword.o`、
   `fshut.o`、`eprintf.o`，再到 stdio/启动/TLS/退出所引用的 musl 对象。
2. 用锁定 sbase 源编译四个对象并取 `nm -u`，确定**对象级**未解析符号（不是目录级猜测）。
3. 用 musl 自己的 `alltypes.h`/`syscall.h` 生成规则，在宿主编译一个只读探针，实测
   `sizeof(struct pthread)` 与 `builtin_tls`。
4. 对照内核 `elf_layout`/`load_runtime_elf` 与当前静态 PIE 产物，确定装载/重定位约束。

## 3. 关键发现

### 3.1 P5-LIB-01 的符号清单不完整

`sbase` 的 `fshut.c` 在错误路径调用 `weprintf`，后者与 `xvprintf` 同处 `eprintf.o`，
因此对象闭包还包含：

```
fprintf  vfprintf  perror  strlen  strncmp  exit  putc
```

`putc` 来自 `-O2` 对 `putchar` 的降级（GCC 与 Clang 均如此）；`putchar` 与 `putc` 都必须
可用。`echo` 的**正常路径**不需要 `vfprintf`/`fprintf`/`perror`，但链接器必须解析它们；
它们不能被称为“链接成功即支持”。

按 profile 的变更控制，P5-LIB-01 的冻结行不能静默改写。**建议发布 v1**，把上述符号增列为
`planned`，并在注记中标 `UNTEST`（仅链接、不承诺），同时在 P5-IO-01 的后端注记里写明
musl stdio 使用 `writev`。在 v1 发布前，本片不修改 profile v0，也不宣称这些符号可用。

### 3.2 stdio 的真实后端是 `writev`，不是 `write`

`fputs` → `fwrite` → `__fwritex` → `stdout` 的 `write`（`__stdout_write`）→
`__stdio_write`，后者调用：

```c
cnt = syscall(SYS_writev, f->fd, iov, iovcnt);   // src/stdio/__stdio_write.c:15
```

首次 stdout 写还会调用 `ioctl(fd, TIOCGWINSZ)`；失败不是致命错误，只把 `lbf` 置为 `-1`
（转为全缓冲）。`fclose(stdout)` 会调用 `fflush` 与 `close(1)`；`stdout` 是 `F_PERM`，
`fclose` 在关闭后提前返回，不会 `free`。因此适配层必须：

- 实现 `writev`（逐 iovec 发送，返回总字节；失败返回原始负 errno）；
- `ioctl` 返回 `-ENOSYS`（避免引入未冻结的 `ENOTTY`）；
- `close(1/2)` 返回成功；其他 fd 返回 `-EBADF`；
- `write(fd, …)` 也按 P5-IO-01 实现，供直接调用与错误路径使用。

### 3.3 TLS：echo 不需要 `mmap`，但必须要 FS base

实测（宿主，使用 musl 1.2.5 自己的头文件生成规则）：

```
pthread=200 builtin=336 min_align=8 space=128
```

x86_64 不使用 `TLS_ABOVE_TP`。无 `PT_TLS` 时，`__init_tls` 计算
`libc.tls_size = round_down(2*8 + 200 + 0 + 8 + 7, 8) = 224`，小于 `builtin_tls=336`，
所以**走静态 `builtin_tls`，不调用 `mmap`**。`builtin_tls` 余量 112 字节；在
`main_tls.align=8` 时，应用 TLS 约 ≤105 字节仍可放入。`echo` 没有 TLS 变量，预计成立；
M3 必须用链接产物复核。

但 `__init_tp` 仍会调用 `__set_thread_area(TP_ADJ(p))`。x86_64 的实现是
`src/thread/x86_64/__set_thread_area.s`，直接发 Linux `arch_prctl(ARCH_SET_FS)`；
它绕过 `syscall_arch.h`，无法被 C 适配层截获。必须替换该汇编点，并提供一个
per-task FS base 机制。QEMU 当前 CPU 是 `qemu64`（`xtask/src/qemu.rs:212`），不应依赖
`wrfsbase`/FSGSBASE。

### 3.4 静态 PIE 与重定位：必须避免双重重定位

内核 `elf_layout` 只接受 `e_type=3`（ET_DYN，`kernel/src/user.rs`），因此 musl 产物必须是
静态 PIE。内核的 `apply_relative_relocations` 只处理 `R_X86_64_RELATIVE`，并在装载时
应用 `load_base + addend`；遇到其他类型会拒绝。

musl 的 `rcrt1.c`（静态 PIE 的 crt）会在启动时**再次**应用 `RELATIVE`（`ldso/dlstart.c`）。
如果内核已经应用，二者会叠加，指针错误。因此建议：

- 用 **Prima 自建 crt** 取代 `rcrt1`：保存入口五寄存器到 `.bss` 的 `prima_ctx`，
  按 SysV 栈布局调用 `__libc_start_main`，**不执行自重定位**；
- 继续由内核应用 `RELATIVE`；
- M3 链接后用 `readelf -d/-r` 核验只有 `R_X86_64_RELATIVE`。若出现 `DT_RELR`
  （打包相对重定位）或其他类型，需在链接时关闭（如 `-z nopack-relative-relocs`）
  或扩展内核装载器，不能静默跳过。

当前 `stage5-probe.elf` 是 ET_DYN、入口 `0x1824`、**无任何重定位**；它不能证明 musl
静态 PIE 的重定位形态。

### 3.5 启动/auxv：最小 auxv 可跑 echo，但通用 libc 需要完整 auxv

`__init_libc` 读取 auxv 后调用 `__init_tls`；无 `AT_PHDR/PHNUM/PHENT` 时找不到
`PT_TLS`，但无 TLS 的 echo 仍可走 `builtin_tls`。`AT_UID/EUID/GID/EGID` 缺省为 0，
`AT_SECURE` 为 0，因此 `__init_libc` 走非 secure 早退，不会调用 `poll`/`open`。当前初始栈
已有 `AT_PAGESZ=4096` 与 `AT_NULL`，所以 **E1–E6 的 musl 启动在理论上不依赖完整 auxv**。

但这只是 echo 的特例，不足以支撑通用 libc：

- 应用一旦有 `PT_TLS`，就需要 `AT_PHDR/PHNUM/PHENT`；
- `AT_EXECFN`、`AT_ENTRY`、`AT_BASE` 对通用启动/诊断有价值；
- `AT_RANDOM` 只在启用 SSP 时需要；本阶段建议 Prima 构建关闭 SSP，因此不提供。

**建议**：在 P5-BOOT-01 中补齐 auxv（值由服务管理器从 ELF 头与装载基址计算），但不要把
它列为 E1–E6 的硬阻塞。初始栈的 `argc/argv/envp` 仍需按用例注入，见 3.7。

### 3.6 退出观察需要上下文，不能沿用探针的栈参数

musl 的 `_Exit(status)` 只有状态，没有 `probe_ctx`。适配层的 `exit_group` 需要管理器 id、
发送 cap 与 self cap。建议由 Prima crt 在调用 `__libc_start_main` 前，把入口五寄存器
（`rdi=self_cap`、`rsi=send_cap`、`rdx=code_cap`、`rcx=name`、`r8=manager_id`）写入
`.bss` 的 `prima_ctx`；适配层读写该全局。现有可写 `PT_LOAD`/BSS 私有装载已支持这一点。

退出消息可复用现有 `STAGE5_C_PROBE_TAG` 形态（状态 + task id），但服务管理器的回收逻辑
目前绑定 C 探针的固定资源；需要泛化为“任一被管理的 C 任务退出”。这是 M2/P5-PROC-01 的
接线，不是新的内核对象。

### 3.7 echo 的 E1–E6 需要按用例注入 argv

`echo` 的六个冻结用例参数不同。当前初始栈只构造 `argc=2` 的固定探针。可复用的最小方案是
在 boot manifest 增加一个**参数 blob kind**（NUL 分隔的 argv/envp），内核仍只按 opaque
kind 透传，服务管理器从映像目录读到 blob 后构造初始栈。这样不需要改内核语义，也不把
“参数”变成内核概念。代价是 manifest/xtask 与服务管理器的构造逻辑变化，并且需要在
profile v1 中冻结“任意 argv”的边界。没有它，S5-X3 无法逐用例运行 echo。

### 3.8 单线程锁与 malloc：只链接、不执行

`libc.need_locks` 初始为 0；`__lock` 在 `need_locks==0` 时立即返回，`__unlock` 只在
`l[0] < 0` 时唤醒。`stdout`/`stderr` 的 `.lock=-1`，连 `__lockfile` 都不调用。因此
E1–E6 不会进入 futex。

`fclose`（非 `F_PERM` 文件）与 `atexit` 扩容会引用 `free`/`calloc`，从而把 mallocng
链接进来；mallocng 使用 `brk`/`mmap`/`munmap`/`madvise`。echo 的正常路径不调用
malloc/free，但**链接闭包包含它**。适配层应让这些 syscall 确定返回 `-ENOSYS`，并用
T-MUSL-05 断言 E1–E6 期间计数为 0；不得因此宣称 malloc 可用。

### 3.9 构建环境差异

锁定 sbase 源用 Ubuntu GCC `-O2 -std=c99` 编译时，默认 fortify/SSP 会引入
`__fprintf_chk`、`__vfprintf_chk`、`__stack_chk_fail`；这属于宿主 glibc 的构建硬化，
不是源码依赖。Prima 构建建议使用 clang，并显式：

```
-O2 -std=c99 -fno-stack-protector
```

以避免 glibc 专有 `*_chk` 符号与 `AT_RANDOM` 依赖。代价是**没有 SSP canary**，本阶段不把
它当作安全承诺，需在 M3 的已知边界中登记。宿主的 `clang` 有两个版本：Lean clang 22.1.4
（资源目录缺 `stddef.h`，宿主构建不可用）与 `/usr/bin/clang-18`（1ubuntu1）。M3 必须锁定
一个编译器并记录版本/资源目录哈希；本片不替它做选择。

### 3.10 对 DOC-002 替换点与构建开关的核对

候选规格的“5 个替换点”需要按实际源码校正，不能直接当作可执行清单：

| 候选点 | 实际情况 | 本阶段结论 |
|---|---|---|
| E1 `syscall_arch.h` | C 代码的 `__syscall0..6` 共用该头，确实是单点；但 x86_64 还有直接发 `syscall` 的汇编：`__set_thread_area.s`、`syscall_cp.s`、`clone.s`、`__unmapself.s`、`signal/restore.s`、`vfork.s` | echo 只碰到 `__set_thread_area.s`；其余在引入线程/信号/fork 前必须逐个替换，不能只改头文件就宣称“单点完成” |
| E2 errno | `__errno_location` 返回 `&__pthread_self()->errno_val`，没有单独的 x86_64 errno 汇编点；真正依赖是 TCB + FS base | 与 E3 合并处理；不需要伪造 `__thread` |
| E3 线程/TLS | `__init_tp` 调用 `__set_thread_area`；单线程下 `libc.need_locks=0`，锁为空操作 | echo 不需要 `pthread_create`；不得提供“成功返回”的假线程实现。对外线程仍为 `unsupported` |
| E4 动态加载 | 静态闭包不使用 `ld.so`；静态 PIE 的 `rcrt1` 负责自重定位 | 用 Prima crt 取代 `rcrt1`；动态链接仍归阶段 7 候选 |
| 第五点（POSIX 转发层 / fd↔cap dtable） | 不在 musl 源码替换点内，属于适配层 fd 表与转发 | echo 只需要 fd 1/2 的固定映射；通用 dtable 属 P5-FD-01，未实现 |

构建开关：`configure` 的 `CFLAGS_NOSSP` 只作用于早期对象，不是全局关闭 SSP；静态构建应使用
`--disable-shared`，并在 Prima 侧显式追加 `-fno-stack-protector` 与关闭 fortify。没有
“切换到 Prima syscall ABI”的 configure 选项，必须通过补丁替换 E1/E3 的源文件。

## 4. 决策与代价

| ID | 决策 | 推荐 | 代价/边界 |
|---|---|---|---|
| M0-D1 | TLS FS base 机制 | 追加 `SYS_TASK_SET_FS_BASE=38`，per-task 保存 FS base，`sched::resume` 切换时写 `IA32_FS_BASE`（MSR `0xC000_0100`） | 内核任务状态 + 调度切换 + syscall + runtime 包装 + ring3 测试；需 profile v1 登记。`wrfsbase` 方案被 `qemu64` CPU 与额外特性依赖否决 |
| M0-D2 | 静态 PIE 重定位 | Prima crt 跳过 `rcrt1` 自重定位，继续由内核应用 `RELATIVE` | 需要自建 crt、链接脚本/标志；M3 必须核验无 `DT_RELR`/非 `RELATIVE`，否则扩装载器或改链接标志 |
| M0-D3 | C 系统边界 | C 适配层提供 `__syscall0..6`，Linux 号只在适配层翻译，错误返回原始负 errno | `writev`/`close`/`ioctl`/`set_tid_address`/`exit` 的语义测试；不得让 Linux 号进入内核 |
| M0-D4 | 退出观察 | Prima crt 保存 `prima_ctx`，`exit_group` 上报状态后 `SYS_TASK_DESTROY` | 服务管理器回收逻辑泛化；T-MUSL-07 |
| M0-D5 | auxv | 在 P5-BOOT-01 补齐 `AT_PHDR/PHNUM/PHENT/ENTRY/BASE/EXECFN` 等；`AT_RANDOM` 不提供（SSP 关闭） | 服务管理器解析 ELF 头；E1–E6 非硬阻塞，但通用 libc 必需 |
| M0-D6 | argv 注入 | boot manifest 增加 NUL 分隔参数 blob kind，服务管理器构造初始栈 | manifest/xtask/servicemgr 改动；profile v1 冻结任意 argv 边界 |
| M0-D7 | profile v1 | 增列 3.1 的符号、登记 `SYS_TASK_SET_FS_BASE=38`、注记 `writev` | 需组织级复核；不影响本片代码 |
| M0-D8 | 构建硬化 | clang + `-fno-stack-protector`，关闭 fortify | 无 SSP canary；作为已知安全边界登记 |
| M0-D9 | 容量 | 先构建并测量 echo ELF 页数，再决定是否扩 `MAX_SEGMENT_FRAMES`/vspace | 当前 82/96 页、audit 余量 11；不得预扩掩盖真实需求 |

## 5. 测试 ID（M2/M3 落地时执行）

| ID | 断言 |
|---|---|
| T-MUSL-00 | 快照 replay：commit/`COPYRIGHT`/归档哈希与锁一致；不一致即停止 |
| T-MUSL-01 | 适配层 `__syscall`：未知号 `-ENOSYS`；`writev` 返回总字节；`close(1/2)=0`；非法 fd `-EBADF`；返回值保持 musl 期望的原始负 errno |
| T-MUSL-02 | TLS：`SYS_TASK_SET_FS_BASE` 后 `%fs:0` 可读；两个任务切换后各自的 FS base 不串扰；无 TLS 时 `builtin_tls` 路径成立 |
| T-MUSL-03 | 重定位：M3 产物 ET_DYN，只有 `R_X86_64_RELATIVE`；Prima crt 不执行 `rcrt1`；内核应用一次 |
| T-MUSL-04 | stdio：E1–E6 stdout 逐字节与 profile 冻结值一致；退出码一致；`writev` 路径覆盖 `hello\n` 的两个 iovec |
| T-MUSL-05 | no-malloc：E1–E6 期间 `brk`/`mmap`/`munmap`/`madvise`/`futex` 调用计数为 0 |
| T-MUSL-06 | auxv：初始栈含预期键值；缺 `AT_PHDR` 时无 TLS 仍可启动，补全后应用 `PT_TLS` 可初始化 |
| T-MUSL-07 | 退出：状态被管理器观察；任务自毁；栈/数据 cap 回收，无泄漏 |
| T-MUSL-08 | 错误路径：`fprintf`/`vfprintf`/`perror` 的失败可确定观察；不得静默成功 |
| T-MUSL-09 | 容量：记录 echo 的段页数、cap、vspace、audit 峰值；逐项登记与旧值的差 |

## 6. 非承诺与下一步

- 本片没有移植 musl、没有编译 `libc.a`、没有运行 echo，也没有新 QEMU 见证。
- P5-LIB-01 仍为 `planned`；P5-BOOT-01/P5-PROC-01/P5-IO-01 仍为 `partial`；S5-D*/S5-X* 未勾选。
- 建议下一片进入 **M2**：先落 `SYS_TASK_SET_FS_BASE=38` + per-task FS base（T-MUSL-02），
  再做 C 适配层骨架与 `writev`/`close`/`ioctl`（T-MUSL-01/04），最后接 Prima crt 与
  静态 PIE 核验（T-MUSL-03）。P5-BOOT-01 的 argv blob 与完整 auxv 可在 M2 并行准备，
  但必须在 S5-X3 前完成。

## 7. 复现

```sh
# 快照（本机实测；上游直连超时，使用镜像）
git clone --depth 1 --branch v1.2.5 \
  https://ghfast.top/https://github.com/ifduyue/musl.git target/musl-1.2.5
git -C target/musl-1.2.5 rev-parse HEAD
curl -L -o target/musl-v1.2.5.tar.gz \
  https://ghfast.top/https://github.com/ifduyue/musl/archive/refs/tags/v1.2.5.tar.gz
sha256sum target/musl-v1.2.5.tar.gz target/musl-1.2.5/COPYRIGHT target/musl-1.2.5/VERSION

# sbase 锁定对象
git clone --depth 1 https://git.suckless.org/sbase target/sbase
sha256sum target/sbase/LICENSE target/sbase/echo.c \
  target/sbase/libutil/putword.c target/sbase/libutil/fshut.c \
  target/sbase/libutil/eprintf.c target/sbase/util.h \
  target/sbase/arg.h target/sbase/compat.h

# 对象级未解析符号
cd target/sbase
/usr/bin/clang-18 -O2 -std=c99 -fno-stack-protector -I. -c echo.c -o /tmp/sbase-echo.o
/usr/bin/clang-18 -O2 -std=c99 -fno-stack-protector -I. -c libutil/putword.c -o /tmp/sbase-putword.o
/usr/bin/clang-18 -O2 -std=c99 -fno-stack-protector -I. -c libutil/fshut.c -o /tmp/sbase-fshut.o
/usr/bin/clang-18 -O2 -std=c99 -fno-stack-protector -I. -c libutil/eprintf.c -o /tmp/sbase-eprintf.o
nm -u /tmp/sbase-{echo,putword,fshut,eprintf}.o | sort -u

# TLS 尺寸探针（使用 musl 自己的头生成规则）
cd target/musl-1.2.5
mkdir -p obj/include/bits
sed -f tools/mkalltypes.sed arch/x86_64/bits/alltypes.h.in include/alltypes.h.in \
  > obj/include/bits/alltypes.h
cp arch/x86_64/bits/syscall.h.in obj/include/bits/syscall.h
sed -n -e s/__NR_/SYS_/p < arch/x86_64/bits/syscall.h.in >> obj/include/bits/syscall.h
cat > /tmp/musl_size.c <<'EOF'
#include <stddef.h>
#include <stdio.h>
#include "pthread_impl.h"
struct builtin_tls { char c; struct pthread pt; void *space[16]; };
int main(void) {
    printf("pthread=%zu builtin=%zu min_align=%zu space=%zu\n",
        sizeof(struct pthread), sizeof(struct builtin_tls),
        offsetof(struct builtin_tls, pt), sizeof(void *)*16);
    return 0;
}
EOF
/usr/bin/clang-18 -std=c99 -D_XOPEN_SOURCE=700 -fno-stack-protector \
  -I arch/x86_64 -I arch/generic -I src/include -I src/internal \
  -I obj/include -I include -o /tmp/musl_size /tmp/musl_size.c
/tmp/musl_size
# 实测输出：pthread=200 builtin=336 min_align=8 space=128
```

本片为文档/静态盘点，未改动引导或 syscall 路径；验证仅需宿主检查（实际运行结果）：

```sh
cargo test --workspace   # PASS；xtask 198 tests + 其余 crate 全绿
cargo xtask lint         # PASS；fmt / clippy -D warnings / 裸机内核 / 零 unsafe
```

未运行 `cargo xtask test`：本片没有改变引导、ELF 装载或 syscall 路径。
