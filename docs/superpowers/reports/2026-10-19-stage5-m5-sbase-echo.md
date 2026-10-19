# 阶段 5 · M5 首切片：sbase `echo` E1–E6

**日期**：2026-10-19
**性质**：实施证据报告。M5 的目标是“选定开源程序源码重编译并通过冻结功能测试”；本切片完成
profile v0 §5 冻结的 sbase `echo` 六用例，不改变 profile 边界，也不代表阶段 5 退出。

## 1. 结论

- sbase `echo` 以锁定 commit `c546c3a5724c81cee9a11d816a38ccdf17472129` 的**未修改源码**
  用 Prima musl sysroot 重新编译、静态链接，在 QEMU 中双轮通过 E1–E6。
- 每个用例的 stdout 逐字节、退出码与 Linux 宿主参考一致。Prima 侧证据同时包含：
  管理器从 C 任务逐字节转发得到的 `len`/FNV-1a、退出码，以及串口原始字节。
- 既有探针路径（`cargo xtask test` 的前两轮 BIOS）保持原数值：IPC `45/152/160`、
  Namespace `resolves=5`、`CAPACITY` 与 `TASKCAP` 逐字段不变。
- S5-X3 的**证据已具备**，但按阶段退出程序仍需组织级复核后勾选，本报告不自行勾选。

## 2. 构建与运行方式

`xtask` 在每个 echo 用例下**用 `echo.elf` 替换 C 探针映像**（manifest kind 仍为
`C_PROBE`），并追加一个参数载体条目（kind `ECHO_ARGS`）：

- `echo` 源码：`target/sbase/{echo.c,libutil/{putword,fshut,eprintf}.c}`；构建前用
  `git -C target/sbase rev-parse HEAD` 校验冻结 commit。
- 编译：`clang --target=x86_64-unknown-none -O2 -std=c11 -Werror -ffreestanding
  -fPIC -static-pie -nostdinc -isystem target/musl-prefix/include -lc -lgcc`；产物经
  `validate_static_pie` 校验（ET_DYN、无 `PT_INTERP`、仅 `R_X86_64_RELATIVE`）。
- 参数：内核要求 manifest 条目均为合法 ELF，故 argv 块放在一个单页 ET_DYN 的固定偏移
  `0x100` 之后；服务管理器按 `[count][len, bytes]...` 解析并构造初始栈。
- 初始栈仍为冻结布局：`argc/argv/NULL/envp/NULL/auxv`，envp 固定
  `PRIMA_ABI=1`，auxv 仅 `AT_PAGESZ` + `AT_NULL`。

冻结源码哈希（本轮复核）：

```
117c10e0… echo.c
5c1c8756… libutil/putword.c
a5ee3071… libutil/fshut.c
9547578e… libutil/eprintf.c
```

Prima 产物：`target/prima-os/echo.elf`（SHA-256 `a880d3607c5f8888c2d2787cddddea677b02919c79985774dd739c22c27edadf`）。

## 3. 证据

```
cargo test --workspace     全部 test result: ok（含 200 个 xtask 测试）
cargo xtask lint           fmt / clippy -D warnings / 裸机 deny(unsafe_code) / 零 unsafe 全 PASS
cargo xtask test           PASS stage1 boot slice（退出码 0）
```

`cargo xtask test` 的实际轮次：

```
PASS qemu bios run=1                         # 既有探针，数值不变
PASS qemu bios run=2
PASS linux sbase echo E1-E6 stdout/exit references
PASS qemu bios E1 run=1 / run=2
PASS qemu bios E2 run=1 / run=2
PASS qemu bios E3 run=1 / run=2
PASS qemu bios E4 run=1 / run=2
PASS qemu bios E5 run=1 / run=2
PASS qemu bios E6 run=1 / run=2
```

每轮判定：

- `EVENT_STAGE5_CAPACITY = [17, 0x5a, 0, 1, 1]`：任务 17 退出码 0、栈与数据段已回收、
  容量拒绝仍为 `ENOSPC`。
- `TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132`。
- `ECHO|status=0;len=<N>;hash=<FNV-1a>`：由管理器对 C 任务发来的每个 fd 1/2 字节累加，
  `<N>` 与 FNV 值必须等于该用例冻结 stdout。
- 串口原始流必须包含冻结 stdout；echo 轮不得出现探针专用的
  `CIO|`/`TTYIN|`/`TTYOUT|`。
- Linux 参考用冻结命令 `cc -O2 -std=c99 echo.c libutil/{putword,fshut,eprintf}.c -I.`
  编译，并对同一组 argv 断言 stdout/退出码。

## 4. 设计取舍

| 事项 | 取舍 |
|---|---|
| 每次启动一个 C 任务 | `MAX_TASKS=18` 且 Dead 记录不复用 ID，因此一个用例一次启动；六用例各自构建 ramdisk、各跑两轮。 |
| argv 注入 | xtask 生成参数块；内核只接受合法 ELF 条目，故用单页 ET_DYN 承载，管理器从固定偏移读取。 |
| 探针证据保留 | echo 本身不做文件 I/O，而 `NsWitness` 冻结了 M4 的两次 `open("f")` 授权解析。为**不放宽内核守卫**，`prima_crt` 在 echo 构建下先跑一个静默 harness（两次原始 `openat`/`close`），再进入 `__libc_start_main`；harness 不写 stdout/stderr，失败退出码 71–74。 |
| 输出归因 | 管理器对 echo 的字节流独立计数/哈希；tty 对 echo 字节使用独立 tag，不输出探针诊断行。 |
| 串口收尾 | echo 退出后管理器向 tty 发排空屏障，收到 ACK 后才输出 `ECHO|` 并收尾，避免字节尚未落串口就结束。 |
| IPC 高水位 | 依赖程序实际字节数，不再是固定探针值。第一轮校验结构并记录 `(queued,direct,blocks)`，第二轮必须与第一轮逐值相同；探针路径仍固定 `45/152/160`。 |

## 5. 边界（不得据此宣称的结论）

- 仅覆盖 profile 冻结的 E1–E6；没有通用 `execve`/shell/多程序调度，也不代表任意
  POSIX 程序可用。
- argv 上限为 8 项、参数块 32 字节（冻结用例远低于上限）；仍只有最小 auxv，不是完整
  `AT_*` 集合。
- `echo` 的正文未修改；但构建时宏重命名未使用，前置 harness 属于 OS 测试/CRT 层，
  不是 `echo` 的文件 I/O。
- 未验证 sbase 其他工具、stdio 文件 API（`fopen/fread/fwrite`）、线程、信号、网络、
  动态链接。
- `NsWitness` 的 `resolves=5`、探针 IPC 计数与容量高水位仍是本阶段冻结的**探针应用**
  期望；echo 轮通过前置 harness 与逐轮一致性检查保持同一守卫，而不是放宽内核断言。
