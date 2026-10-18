# 阶段 5 · musl M2：C 系统边界与 per-task FS base

**日期**：2026-10-18

**性质**：M2 首个完整纵向切片及系统边界收尾；实现内核 `SYS_TASK_SET_FS_BASE=38`、
per-task FS base、C 适配层骨架、同源探针，并补齐 Linux `exit(60)` 与 `__syscall4..6`
参数路径。**未移植 musl、未构建 `libc.a`、未运行 echo**；S5-D*/S5-X*
仍全部未勾选，profile v0 未修改。

**上位计划**：[musl 移植计划](../plans/2026-10-18-musl-port-plan.md)；
**前序**：[M0 盘点](2026-10-18-stage5-musl-m0-inventory.md)。

## 1. 本片交付

### 1.1 内核：per-task FS base（T-MUSL-02）

- `prima-abi` 追加 `SYS_TASK_SET_FS_BASE = 38`（不复用空洞；ABI 版本仍 1.0，属向后兼容追加）。
- `prima-ostd::arch::load_fs_selector`：CPL0 下装载 `USER_DS = 0x2b`。
- `kernel/src/sched.rs`：每 Task 保存 FS base；`resume` 在写 CR3 之后、`iretq` 之前
  **先装 FS 选择子，再写 `IA32_FS_BASE`**；无 TLS 的任务写 0，任务间不继承基址；
  `clear_task` 清零，避免 id 复用继承旧 TCB。
- `SYS_TASK_SET_FS_BASE` 只接受 `[0, 2^47)` 的规范低半区地址；非规范值确定性 `-EINVAL`
  （写 MSR 会 `#GP`）。成功返回 0，并立即写 MSR（syscall 不经过 `resume`）。
- `prima-runtime::Runtime::task_set_fs_base` 提供 Rust 侧包装。

### 1.2 C 适配层（T-MUSL-01）

新增 `prima-user/c/prima_abi.h`、`prima_syscall.h`、`prima_syscall.c`：

- `__syscall0..6` 接受 Linux x86_64 号，在适配层翻译为 Prima 原生号；**Linux 号不进入内核**。
- 失败返回原始负 errno（不设置 `errno`），符合 musl syscall 约定。
- 已实现并测过的翻译：
  - `write` / `writev`：fd 1/2 逐字节 `SYS_SEND` 到管理器，每次 `SYS_YIELD`；
  - `close`：fd 1/2 返回 0，其余 `-EBADF`；
  - `read` / `ioctl`：`-ENOSYS`（未承诺，见边界）；
  - `set_tid_address`：返回 `SYS_TASK_ID`；
  - `arch_prctl`：`SET_FS` → `SYS_TASK_SET_FS_BASE`，`GET_FS` 读回适配层缓存；
  - `exit`(60) / `exit_group`(231) / `_exit`：上报退出状态后 `SYS_TASK_DESTROY(self)`。
- 未列出的号一律 `-ENOSYS`；`__syscall4..6` 把全部参数透传给翻译层（不再静默丢弃
  `a3..a5`），因此 musl 的取消点调用链 `__syscall_cp` → `__syscall6` 在适配层侧已可用。
- `stage5_start.S` 作为 Prima crt 骨架：把入口五寄存器
  （`rdi=self`、`rsi=send`、`rdx=code`、`rcx=name`、`r8=manager`）写入 `.bss` 的
  `prima_ctx`，再调用 C 入口。**尚未接 `__libc_start_main`**（M3）。

### 1.3 静态 PIE 核验（T-MUSL-03 的第一步）

`xtask` 在打包 C 探针前解析 ELF 并**使构建失败**，除非：

- `e_type == ET_DYN`；
- 无 `PT_INTERP`；
- 所有 `SHT_RELA` 条目的类型都是 `R_X86_64_RELATIVE`（8）；
- 无 `SHT_REL`、无 `SHT_RELR`。

实测 C 探针为 `ET_DYN`，`.rela.dyn` 仅 2 条 `R_X86_64_RELATIVE`。这验证的是**探针 +
Prima crt 骨架**，不是 musl 产物。

### 1.4 边界收尾：`exit(60)` 与 `__syscall4..6`

- `PRIMA_LINUX_SYS_exit=60` 与 `exit_group=231` 映射到同一退出路径；此前 60 落在默认分支
  返回 `-ENOSYS`，与[符号追溯表](../compat/symbol-trace.md)第 4 节不一致。
- `prima_translate` 改为接收六个参数，`__syscall4/5/6` 不再丢弃 `a3..a5`。musl x86_64 的
  取消点标准路径是 `__syscall_cp` → `sccp` → `__syscall(6 args)` → `__syscall6`（仅链接
  `pthread_cancel.o` 时才改走 `__syscall_cp_asm`），所以本改动是取消点在适配层侧的接线；
  musl `syscall_arch.h` 的替换仍属 M3。
- 同源探针新增 4/5/6 参数未知号断言（两端均为 `-ENOSYS`），成功路径改用 Linux `exit(60)`
  退出并要求其不返回；若映射缺失，探针以 7 退出，Linux 参考仍为 37。
- 成功路径改用 60 后，单次验收不再以 231 退出。两者共用同一个 `prima_exit_group`，差异只在
  switch 标签；231 仍由既有失败路径的 `_exit` 定义保留。若要求 231 也作为成功路径证据，
  需另加探针变体或第二成功用例，本片不做。

## 2. 同源 C 探针

`main` 在两端使用同一组 Linux 原始调用号：

- Linux：glibc `syscall()`，适配层把 `-1/errno` 转回原始负值；
- Prima：`prima_syscall.c` 的 `__syscallN`。

成功路径逐字节不变：fd 1 写 `h`，fd 2 写 `i`，再用两个 iovec 写 `\nxy` + `z\n`
（共 5 字节）。新增断言全部通过返回码 37 门控：

| 断言 | Linux 参考 | Prima |
|---|---|---|
| `write(1/2)` 返回 1 | 是 | 是 |
| `writev` 两 iovec 返回 5 | 是 | 是 |
| 非法 fd 的 `write`/`writev`/`close` | `-EBADF` | `-EBADF` |
| 未知号 999（3 参数） | `-ENOSYS` | `-ENOSYS` |
| 未知号 999（4/5/6 参数） | `-ENOSYS` | `-ENOSYS` |
| `exit`(60) 成功路径 | 退出码 37 | 退出码 37 |
| `set_tid_address` | > 0 | > 0 |
| `ioctl` | < 0（Linux `-ENOTTY`） | `-ENOSYS` |
| NULL 缓冲区 `write` | 不跑（宿主基线不稳定） | `-EFAULT` |
| 未授权 `SYS_SEND`（代码段 cap） | 不适用 | `-EACCES` |
| FS base `SET/GET`、`%fs:0` 往返 | 不跑（会破坏 glibc TLS） | 是（含一次 yield 后复查） |

Linux 参考的环境稳定性说明：本机 glibc `syscall()` 返回 `-1/errno`，探针转回原始负值；
宿主内核对 `write(1, ptr=1, 1)` 的返回与环境相关（实测为 1，而非 `-EFAULT`），因此
NULL 缓冲区断言只在 Prima 侧执行。`arch_prctl(SET_FS)` 会替换 glibc 的 TLS 基址，
Linux 参考侧整体跳过 FS 检查。

## 3. 证据

### 3.1 命令与结果

```
cargo test --workspace
  cosir 146, cosir-state 42, lean-backtrace 3, kernel 400,
  prima-initgen 2, prima-ostd 21, prima-runtime 1, xtask 200 all PASS

cargo xtask lint
  PASS fmt --all --check
  PASS clippy -D warnings
  PASS kernel builds under deny(unsafe_code) for x86_64-unknown-none
  PASS kernel kernel/src has zero `unsafe` (27 files scanned)

cargo xtask test
  PASS stage5 C probe is static PIE with R_X86_64_RELATIVE only
  PASS build kernel
  PASS image bios
  PASS image uefi
  PASS linux stage5 C main exits 37/2/3 and streams h/i plus 5-byte xyz via write(1/2)
  PASS qemu bios run=1
  PASS qemu bios run=2
  PASS stage1 boot slice
```

QEMU 既有见证**逐字节未变**：`CIO|task=17;fd=1;ch=104;fd=2;ch=105`、
`TTYIN|...`、`TTYOUT|task=17;in_magic=1;out_of_range=1;out_ok=1`、原始 `h`/`i`/`xyz`
行、`EVENT_STAGE5_CAPACITY` 的 `[17, 0x5a, 37, 1, 1]`、IPC 45/56/64 与
`audit_used=757` 全部由原校验器通过。新增断言只改变 C 探针内部的执行路径与退出条件，
不新增/改写 QEMU 事件。

### 3.2 FS 恢复的变异验证

把 `sched::activate_fs` 临时改为**永远写 0**（保留选择子装载），重建后 QEMU 在
`TTYOUT` 之后输出 `PRIMA|v=1|seq=12|event=panic|status=error`，验收失败；恢复实现后
双轮 PASS。这证明 ring3 的 `%fs:0`（在两次 write 的 yield 之后）确实依赖调度器的
per-task 恢复，而不是碰巧保留的 MSR 值。

### 3.3 SSE 根因记录

首次接入适配层时 C 探针在 `main` 的 iovec 初始化处 `movaps` 触发 `#UD`，随后
`#PF`→`#GP`→`#DF` 三故障，QEMU 退出码 0。根因是内核 CR4 只有 PAE（`CR4=0x20`），
未置 `OSFXSR/OSXMMEXCPT`，SSE 指令非法。本片不扩内核 FPU 契约，Prima C 构建改用
`-mgeneral-regs-only`，实测产物无任何 XMM 指令。M3 需在「启用 SSE + per-task XMM
保存」与「继续限制通用寄存器」之间裁决。

## 4. 测试 ID 状态

| ID | 状态 | 证据 |
|---|---|---|
| T-MUSL-00 | 未重跑 | 本片未改 musl 快照 |
| T-MUSL-01 | **部分通过** | 适配层 write/writev/close/exit(60)/未知号（3–6 参数）/非法 fd/原始负 errno，Linux+Prima 同源；read/ioctl 仅断言失败 |
| T-MUSL-02 | **通过** | 内核存储单测 2 条；ring3 SET/GET/`%fs:0` + yield 后复查；变异 RED |
| T-MUSL-03 | **部分通过** | Prima crt 骨架保存 `prima_ctx`；探针 ELF 构建期校验 ET_DYN/RELATIVE-only |
| T-MUSL-04–09 | 未执行 | 依赖 musl/echo、完整 auxv/argv、退出回收泛化 |

## 5. 已知边界（不得据此宣称支持）

- `read`、`ioctl` 返回 `-ENOSYS`；无通用 fd 表，只有 fd 1/2 的固定映射；
  `close(1/2)` 是适配层成功返回，不产生内核 fd 生命周期。
- `__syscall4..6` 已全参数透传，但当前冻结的系统调用没有一个消费 4–6 个参数，所以证据只
  覆盖“路径不丢返回值/未知号仍 `-ENOSYS`”，不能证明具体参数语义。musl 侧
  `syscall_arch.h` 替换与 `syscall_cp` 接线仍属 M3。
- `arch_prctl(GET_FS)` 读的是适配层缓存，不是硬件 MSR 读回；只有适配层会改 FS base。
- `prima_ctx` 是单任务可写全局，不覆盖多线程。
- 无 SSE；无 `__libc_start_main`；无 auxv/argv 扩展。
- 未授权发送的负值依赖内核既有授权模型；本片未改授权语义。

## 6. 裁决

- **M2-D1**：FS 选择子每次 `resume` 都重新装载（`USER_DS = 0x2b`），随后写
  `IA32_FS_BASE`；顺序不可交换——装选择子会把基址归零。代价：每次任务切换多一条
  `mov fs` 与一次 `wrmsr`；换来无 TLS 任务确定性地看到基址 0。
- **M2-D2**：只接受低半区规范 FS base（`< 2^47`），不额外要求页对齐。代价：非规范值
  在 syscall 层返回 `-EINVAL`，与 Linux `arch_prctl` 的 `#GP` 行为不同，但更安全。
- **M2-D3**：M2 不启用 SSE，Prima C 构建固定 `-mgeneral-regs-only`。代价：编译器不能用
  XMM 拷贝/浮点；M3 若 musl 路径需要 SSE，必须先补 FPU 契约并重跑证据。
- **M2-D4**：fd 1/2 固定映射到管理器 tty 字节流；通用 fd→Capability 表留到 P5-FD-01。
  代价：`read`/其他 fd 只能返回 `-ENOSYS`/`-EBADF`。
- **M2-D5**：Linux 参考侧使用 glibc `syscall()` 并转回原始负值；跳过 FS 与 NULL 缓冲区
  断言。代价：两端差异必须逐条写在报告里，不能把「Linux 参考通过」当成 FS/EFAULT 证据。
- **M2-D6**：静态 PIE 校验放在镜像构建路径，失败即构建失败；不只看测试退出码。
  代价：未来若产物改用 strip 掉节头，需要改为解析 `PT_DYNAMIC`。
- **M2-D7**：Linux `exit`(60) 与 `exit_group`(231) 共用退出路径。代价：无；两者在 Prima 上
  都没有线程组语义。
- **M2-D8**：`__syscall4..6` 立即透传全部参数，即使当前冻结调用集没有 4–6 参数消费者。
  代价：当前证据只能覆盖路径与错误码，参数语义要等 M3/M4 的首个消费者再补测试。
- **M2-D9**：成功路径以 `exit(60)` 退出，231 只保留定义与失败路径的 `_exit` 使用。
  代价：单次验收不再同时以 231 成功退出；两者共用处理函数，switch 标签差异未单独见证。
