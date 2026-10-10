# SPEC-085 设计：阶段 4 独立 ring3 原生客户端与 IPC Hello 握手

> 状态：探索设计 + 实现；尚未验收
>
> 目标：闭合 S4-D1「原生运行时 API 与版本规则」的**运行侧**——把 `prima-runtime` 客户端作为**真实 ring3 任务**跑起来
>
> 前置参考：SPEC-084 API 原型（`docs/superpowers/specs/2026-09-25-stage4-runtime-api-design.md`）

## 1. 目标与边界

SPEC-084 交付了 `prima-runtime` 的 API 面、协议编解码与 `X86_64Transport` 的**编译检查**，但其报告「边界与未完成项」第 1、4 条明写：**本次没有从真实 ring3 用户任务调用它**、**还没有被独立 ring3 Rust 程序使用**。本切片补上这两项：让独立编译的 Rust ring3 程序，经 `prima-runtime` 的真实 `syscall` 传输创建共享 Segment、映射、写入版本化 PRIM Hello，并以 IPC 连同 Segment Capability 移交给 init；内核侧从共享帧**读回用户写的 wire 字节**并逐位比对。

本切片不实现服务管理器、日志/存储服务、驱动或示例应用（S4-D2..D5），不冻结 syscall 编号为公开或稳定 ABI，不宣称阶段 4 已实现或勾选 S4-D1。

## 2. 装载：把 Rust 用户映像嵌进内核镜像

- `prima-user` 是独立 `no_std` bin（`prima-smoke`），入口 `_start(self_cap_raw, send_cap_raw, initial_cap1_raw)`。
- 内核经 Cargo **artifact dependency（bindeps）** 取该 bin 的构建产物：`include_bytes!(env!("CARGO_BIN_FILE_PRIMA_USER_prima-smoke"))`。构建顺序由 cargo 保证，产物是内核镜像的一部分。
- 启动期 `elf_layout` 解析 ELF64 program headers：校验 `\x7fELF`/class=2/data=1/type=3/machine=62/phentsize=56，逐 `PT_LOAD` 检查 `memsz ≥ filesz` 且文件区间不越界，要求 `entry` 落在某个 `PT_LOAD` 的 `[vaddr, vaddr+memsz)` 内，取 `max_end` 向上取整得页数；`load_runtime_elf` 逐页清零后写入 `PT_LOAD` 文件字节，并按 `PT_DYNAMIC` 的 `R_X86_64_RELATIVE` 重定位加装载基址。
- **装载尺寸用 `const fn` 从 ELF 头求值**（`runtime_app_pages` 扫 program headers、`elf_word` 取入口），于是 `RUNTIME_APP_CODE_PAGES` 与 `RUNTIME_APP_ENTRY_OFFSET` 不再是硬编码常量，而是编译期从同一份字节导出；host 单测 `runtime_elf_matches_loader_layout` 把 `const fn` 结果与运行期 `elf_layout` 绑定为**同一真值**。
- 映像装入既有 task 3 的代码段（`USER3_CODE_VA`），复用 `prepare_task_create_caps` 的取帧序列，排在既有 bring-up 序列**之后**以免平移阶段 2 的物理帧金标。

## 3. 传输：真实 CPL3 `syscall`

`X86_64Transport` 调 `prima_ostd::entry::syscall_from_user`；框架**先检查当前 CS 的 CPL 为 3**（否则返回 `TransportError::NotUserMode`），再按 SysV 序执行 `syscall`：`rax`=号，`rdi/rsi/rdx/r10/r8/r9`=参。内核侧沿用 SPEC-048 的 `syscall` 快路径分派。这是本切片的关键——调用发生在**真实 ring3 上下文**，不是 host 模拟。

## 4. 握手：PRIM wire 与回执判据

用户程序构造 `MessageHeader { opcode: 1, request_id: 3, payload_len: OFFER_LEN(24) }` 与 `ProtocolOffer { major: 1, minor: 0, features: 1, required_features: 1, max_message_size: 4096 }`，然后：`memseg_create(1, RW)` → `memseg_map(..., USER_HELLO_VA, RW)` → `write_user_bytes(USER_HELLO_VA, header‖offer)` → `send(TaskId 1, send_cap, self_id, [segment])`。

内核在 `runtime_hello_ready` 从 `USER_HELLO_VA` **读回 6 个原始 wire word**（不依赖 Rust 结构体布局），构成 `RuntimeHelloWitness`；`evidence_ok` 逐位判定：

| word | 打包 | 期望 |
|---|---|---|
| magic | `"PRIM"`(低 32) ‖ major(32..48) ‖ minor(48..64) | `0x0000_0001_4d49_5250` |
| opcode | opcode(低 16) ‖ flags(16..32) ‖ request_id(32..64) | `0x0000_0003_0000_0001` |
| length | payload_len(低 32) ‖ major(32..48) ‖ minor(48..64) | `0x0000_0001_0000_0018` |
| features / required / max | 独立 word / 独立 word / u32 | 1 / 1 / 4096 |
| map_result（Y 页回执） | — | 0 |

这些字段是**判决性的**：三个打包 word 是 `prima-runtime::protocol::encode` 的产物，只能由 ring3 程序在真实 `syscall` 之后写入它自己映射的 Segment，再由内核从共享帧读回——任何编码漂移都会改变它们。

## 5. 代码页预算（裁决 SPEC-081 遗留）

`USER_CODE_LEN` 由 4072 提升为 **4096 = `FRAME_SIZE`**（断言 `USER_CODE_LEN == FRAME_SIZE`），用户代码独占一页。SPEC-081 曾把路径文本数据塞在代码页高偏移（`NS_PATH_DATA_OFFSET = 4040`，紧贴 `int3` 之后）；本切片把路径文本**移出代码页、落到 Y 回执页**（`NS_PATH_DATA_OFFSET = 1024`，避开 736 处的 COW 回执与接收缓冲），由 `prepare` 逐字节写入。代码页因此回到「纯指令」语义，`int3` 的收尾断言改为「不得越过单页映射」。

## 6. 判据

1. `runtime_hello_ready` 在 `cargo xtask test` 中作为**精确序列事件**被断言，`validate_runtime_hello_data` 冻结整条 data；
2. 反例单测 `rejects_runtime_hello_corruption` 证明该断言真的在判定；
3. host 单测 `runtime_elf_matches_loader_layout` 把 `const fn` 尺寸与运行期解析绑定。

首轮验收为**单向**：只证明客户端发出合法 wire、内核可读回。通过它不能作为服务互通、调度或安全证明。

## 7. 未决项

1. **服务端未参与**：init 尚未解析该 Hello 或回复；双向握手、`minor`/feature 协商与端点选择留后续切片。
2. `_start` 的第三个参数（`initial_cap1_raw`）当前未使用；服务管理器切片会用它取得初始 Authority。
3. 用户映像仍是「单页只读代码 + 固定 Y 回执页」的最小布局，不含 `.data`/堆/多页代码。
4. 本片是**回溯登记**：代码早在 `8a25e49`/`5abdd65`/`5a49a19` 入库，本设计据实追述其决策依据。

本切片不勾选 S4-D1 或任何阶段退出条件。
