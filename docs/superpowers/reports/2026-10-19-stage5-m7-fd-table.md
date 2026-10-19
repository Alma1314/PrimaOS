# 阶段 5 M7：完整本地 fd 表、`dup` 与 `getpid`

**日期**：2026-10-19
**范围**：把 M4 的固定 fd 3..6 扩为本地 fd 表，加入 `dup`/`dup2`/`dup3` 与 `getpid`。
**结论**：Linux 与 Prima 同源探针逐字节一致，双轮 QEMU PASS。`P5-FD-01` 与
`P5-PROC-02` 仍为 `partial`（边界见 §4）；未勾选任何 S5-D*/S5-X*。

## 1. 实现

### 1.1 本地 fd 表（适配层）

- `prima-user/c/prima_syscall.c`：fd 3..10 的本地表，项为 `{kind, object}`；
  - `kind=FILE` 的 `object` 是存储服务句柄（3..6），适配层另有每句柄引用计数；
  - `kind=TTY` 的 `object` 是 1 或 2（`dup(1)`/`dup2(1, fd)` 的结果）。
- `open`/`openat` 先确认本地表有空位，再向服务请求句柄；返回最低空闲本地 fd。
- `dup` 分配新本地 fd 并增加句柄引用；`dup2`/`dup3` 目标只接受 3..10。
- `close` 减少引用，最后一个引用才向服务发 `CLOSE`——因此 `dup` 出的多个 fd
  **共享同一服务端 offset**（Linux 的打开文件描述语义）。
- `read`/`write`/`lseek`/`writev` 全部经本地表解析；`writev` 不再硬编码只接受
  fd 1/2，但 tty 读仍不支持。
- `getpid` 映射到原生 `SYS_TASK_ID=7`（Linux 号 39）。

### 1.2 musl 的 `open` 路径

新探针使用 libc `open()` 后暴露：musl 1.2.5 在 x86_64 上走 **`SYS_open=2`**，
不是 `openat=257`（`__sys_open_cp` 的 `#ifdef SYS_open` 分支），并自动附加
`O_LARGEFILE`。适配层此前只翻译 257，`open()` 得到 `-ENOSYS`。本片追加
`PRIMA_LINUX_SYS_open=2`，与 `openat(AT_FDCWD, ...)` 落到同一冻结子集。
这是 M5/M6 未暴露的缺口：它们的文件授权 harness 直接使用 `openat`。

## 2. 同源探针

`prima-user/c/stage5_fd.c`（Linux glibc / Prima 静态 musl 同一源码）覆盖：

| 位 | 检查 |
|---|---|
| `pid` | `getpid()>0`，且在文件 IPC/yield 前后不变 |
| `open` | 第一次 open 返回 3..10 |
| `dup` | 新 fd 在范围内且不同于原 fd |
| `share` | 经原 fd 读 3 字节后，dup fd 的 `SEEK_CUR` 为 3 |
| `dup2` | `dup2(fd,9)==9`；经 9 写入后 dup fd 的偏移为 4 |
| `dup3` | `dup3(fd,fd,0)`/非法 flags 为 `EINVAL`；`dup3(fd,9,0)==9` |
| `close` | 关闭原 fd 后 dup fd 仍可读回 `abcZef` |
| `self` | `dup2(fd,fd)` 返回 fd |
| `reuse` | 第二次 open 复用 fd 号、偏移从 0、只读句柄写返回 `EBADF` |
| `bad` | `dup(-1)`、`dup(closed)`、`lseek(closed)`、`dup2(99,5)` 均为 `EBADF` |

探针自行执行**两次成功 open**，因此不叠加 M5/M6 的 `echo_harness.c`；NsWitness
仍为 `binds=4;resolves=5;evidence_ok=1`。

Prima 输出：

```
FD|ok=1;pid=1;open=1;dup=1;share=1;dup2=1;dup3=1;close=1;self=1;reuse=1;bad=1
```

`xtask` 的托管程序证据行：`ECHO|status=0;len=78;hash=1828944680`。

## 3. 验证

- `cargo test --workspace`：PASS。
- `cargo xtask lint`：PASS。
- `cargo xtask test`：PASS；新增 F1 两轮（`PASS qemu bios F1 run=1/2`），
  既有 probe 两轮、echo E1–E6 各两轮、T1 两轮全部 PASS，既有字节流未变。
- Linux 参考：`check_stage5_linux_fd` 用宿主 `cc` 重建同一源码，在一次性 cwd
  中运行，stdout 与 Prima 逐字节一致、退出码 0。
- F1 双轮 IPC 计数一致：`queued=45;direct=267;blocks=275`。
- 内核侧：`TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132`；
  `namespace_ready ... resolves=5;evidence_ok=1`；`preempt_ready` 仍
  `ticks=4;pre1=2;pre2=2`。
- 容量：`caps_live=132;caps_peak=141;segment_slots=41;messages_peak=14;
  audit_used=1969;audit_limit=2048`。审计水位距上限 79 条，是本片新增
  `dup`/文件 IPC 的真实增长；仍是未超限的实测值，不是断言放宽。
- 产物：`target/prima-os/stage5-fd.elf` SHA-256
  `8049c1615ffbba754bdf4dfe58735e21c305cc25f067097051896a20d3ad78bc`。

## 4. 冻结边界（`P5-FD-01` / `P5-PROC-02` partial）

- 本地 fd 3..10（8 项）；无 fd 0，不支持 stdin。
- 文件句柄仍只有 4 个并发 open（服务端表）；本地表满返回 `EAGAIN`，不引入
  未冻结的 `EMFILE`。
- `dup2`/`dup3` 的目标只接受 3..10；目标为 0..2 返回 `EBADF`（Linux 会成功）。
- 无 exec，`O_CLOEXEC`/`dup3` 的 `O_CLOEXEC` flag 被接受但无语义效果。
- tty 项可 `dup`，但 `read` 仍不支持；`lseek` 对 tty 项返回 `EBADF`（Linux 为
  `ESPIPE`，该错误码未冻结）。
- 进程退出不会自动关闭其 fd（服务端句柄属于客户端会话，不随 Task 销毁清理）；
  M4 的该边界不变。
- `getpid` 返回当前 Prima Task id：任务存活期间稳定；id 由内核单调分配，u32
  回绕前不复用。无 Process/Thread 区分，无 `getppid`、无进程组。
