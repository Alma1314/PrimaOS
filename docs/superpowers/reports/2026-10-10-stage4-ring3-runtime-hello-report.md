# SPEC-085 报告：独立 ring3 原生客户端与 IPC Hello 握手

> 状态：探索实现；不构成 S4-D1 验收或稳定 ABI
>
> 设计依据：[独立 ring3 原生客户端与 IPC Hello 握手设计](../specs/2026-10-10-stage4-ring3-runtime-hello-design.md)
>
> 实现提交：`8a25e49`（crate 骨架、`prima-ostd::entry::syscall_from_user`、ELF 嵌入式装载）、`5abdd65`（ELF 装载与代码页）、`5a49a19`（Hello 校验与移交 cap 保留）

## 1. 本片要回答的问题

SPEC-084 的 API 原型报告在「边界与未完成项」中明写两条未完成：**没有从真实 ring3 用户任务调用它**、**还没有被独立 ring3 Rust 程序使用**。本片把这两条落实为可复查证据：一个独立编译的 `no_std` Rust 程序在 CPL3 运行，经 `prima-runtime` 的 `X86_64Transport` 走真实 `syscall`，完成「创建共享 Segment → 映射 → 写版本化 PRIM Hello → IPC 移交」，内核从共享帧读回用户写的 wire 字节并逐位比对。

## 2. 判决性见证（`runtime_hello_ready`，本片新增，落在 seq 25）

```
map=0x0;magic=0x14d495250;opcode=0x300000001;length=0x100000018;features=0x1;required=0x1;max=4096
```

- `map=0x0`：ring3 客户端经 `memseg_map` 把自己创建的 Segment 映射到 `USER_HELLO_VA` 成功（回执读自 Y 页）；若映射失败，见证在读取前就返回全零。
- `magic=0x14d495250` = `"PRIM"`（低 32）‖ major 1 ‖ minor 0——**该 word 由 `prima-runtime::protocol::encode` 在 ring3 侧生成**，内核只是从共享帧读回；
- `opcode=0x300000001` = opcode 1 ‖ flags 0 ‖ request_id 3；
- `length=0x100000018` = payload_len 24 ‖ major 1 ‖ minor 0（`OFFER_LEN = 24`）；
- `features=1`/`required=1`/`max=4096`：Hello offer 的协商字段。

**判决性在于「谁写的」**：这 6 个 word 只能是 ring3 程序在真实 `syscall` 之后写入自己映射的 Segment、再由内核从同一物理帧读回的——它同时证明了三件事：(1) 独立 Rust 程序真的被装进 task 并跑到 CPL3；(2) 传输确实是 `syscall` 指令而非 host 桩；(3) 客户端与内核共享同一份 wire 编码（`prima-runtime` 的编码 → 内核的字段解析一致）。

## 3. 实现要点

| 面 | 落点 |
|---|---|
| 用户程序 | `prima-user/src/main.rs`：`Runtime::x86_64()` → `task_id`/`time_now`/`memseg_create`/`memseg_map`/`write_user_bytes`/`send` |
| 嵌入 | `kernel/src/user.rs`：`include_bytes!(env!("CARGO_BIN_FILE_PRIMA_USER_prima-smoke"))`（cargo bindeps） |
| 装载 | `elf_layout` + `load_runtime_elf`：校验 ELF64 头与 `PT_LOAD`、相对重定位、逐页清零后写文件字节 |
| 装载尺寸 | `const fn runtime_app_pages`/`elf_word` 直接从 ELF 头求值，`RUNTIME_APP_CODE_PAGES`/`RUNTIME_APP_ENTRY_OFFSET` 不再是硬编码 |
| 传输 | `prima_ostd::entry::syscall_from_user`：先验 CPL=3，再按 SysV 序 `syscall` |
| 见证 | `kernel/src/user.rs::runtime_hello_witness` 读回 6 word；`kernel/src/protocol.rs::runtime_hello_ready` 序列化 |

## 4. 回归面

本片对既有事件的**语义改动恰为两处**：

1. **新增 1 条事件** `runtime_hello_ready`（seq 25）；其后 seq 顺移一位（当时的 `boot_complete` 37→38）。该事件由 `5a49a19` 引入，`EXPECTED_EVENTS` 37→38。
2. **`task_ready.caps`/`caps_after` 43→44**：Rust 客户端移交的共享 Segment Capability 成为 task 1 的存活槽位（`5abdd65` 登记，同步 xtask 常量 `TASK_READY_LIVE_CAPS`）。

其余为**镜像布局类**变化：用户 ELF 被嵌入内核镜像（`8a25e49`）使 `bootstrap_ready.image_len` 等布局字段增大、`preempt_ready` 栈地址后移——这些是构造性后果、非语义改动，其当前数值与完整逐条 diff 已记录在 `docs/superpowers/reports/2026-10-10-stage3-audit-trace-restore-report.md` §4 与 tracker 的 TCB 行。

**本片为回溯登记**：代码已在 `8a25e49`/`5abdd65`/`5a49a19` 入库，此后 `d6e880d`（SPEC-083 恢复）在其之上新增了 `audit_ready`（seq 29），未触碰本片语义（`runtime_hello_ready` 的 seq 与 data 均未变）。

## 5. 验证（2026-10-10，当前 master）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 365、prima-ostd 21、xtask 192（`prima-abi`/`prima-runtime` 无单测） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1250 mechanism_lines=19671 ratio=5.97%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**39 条事件**，含 seq 25 `runtime_hello_ready`） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例：xtask 单测 `rejects_runtime_hello_corruption` 篡改 wire data 后报 `runtime_hello_ready wire data differs`；host 单测 `runtime_elf_matches_loader_layout` 把 `const fn` 装载尺寸与运行期 `elf_layout` 绑定（删任一侧的校验都会失败）。

## 6. 未决与登记

1. **单向握手**：init 尚未解析该 Hello 或回复；服务端解析、`minor`/feature 协商与端点选择留后续切片。故本片不构成 S4-D1 的「服务互通」证据。
2. **代码页是硬约束**：`USER_CODE_LEN == FRAME_SIZE == 4096` 已定死；本片把路径文本移出代码页正是为守住它。后续切片若再需 ring3 见证空间，应划分新用户页，而非继续压缩既有代码。
3. **`_start` 第三参数未用**：`initial_cap1_raw` 当前被忽略，留给服务管理器切片。
4. **阶段 4 的交付项与退出条件一律不勾选**——沿用组织级独立性门槛，闭合判断交复核者。
