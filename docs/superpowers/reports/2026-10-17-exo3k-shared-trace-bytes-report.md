# SPEC-135 / EXO-3K：共享页原始 trace 字节流

**日期**：2026-10-17
**基线**：SPEC-133 / EXO-3J
**范围**：阶段 4 后续硬化，不改变阶段退出判定

## 实现与验收

servicemgr 在既有客户端共享 Segment 的页内偏移 512 开始，写入 8 字节小端记录数和 13 条定长 48 字节记录（`id` 加 5 个原始参数）。页内前 512 字节继续承载既有服务协议字段；编译期断言保证 trace 区不会越过 4 KiB 页。写方先写完整记录，再更新计数。内核在 init 仍映射该页时仅按有界长度把字节转为 `TRACEBYTES|` 十六进制串口行，不解释 ID 或字段。`xtask` 独立解码、校验长度/十六进制/单行性，并要求全部 13 条与既有 `TRACE|` 服务记录逐条相同。负例包括缺行、数量损坏、非十六进制、非 ASCII 字节和字节分歧。

增加字节写入后，QEMU 曾报告任务 8 在 `USER3_STACK_VA - 16` 触发用户态缺页。任务 8 是 servicemgr；其栈段原只分配并映射一页。将该任务的独立栈段及 init 映射扩为连续两页，保持原栈顶，故障消失。先前误把任务 9 客户端栈加宽的试验已撤回，诊断输出也已移除。

| 命令 | 结果 |
|---|---|
| `cargo test --workspace --offline --quiet` | 全通过；`xtask` 210 项，含共享字节流损坏负例与 init 生成代码哈希更新 |
| `cargo xtask lint` | fmt、Clippy、裸机内核构建、`kernel/src` 零 `unsafe` 四项通过 |
| `cargo xtask test` | BIOS QEMU 两次通过，stage1 boot slice 通过；共享字节流验收在两次启动中执行 |
| `cargo xtask compat` | 首次 QEMU 在 30 秒阈值超时；直接重跑通过 `cow-write-fault` 差分 |
| `cargo xtask tcb` | trusted 1262、mechanism 20247、ratio 5.87%、kernel unsafe 0；`include_bytes=0`、`prima_user_dep=false`、`service_name_lines=1`、`protocol_lines=94` |

## 边界

这是共享页上的**有界二进制记录流**，不是循环 ring，也尚未支持任意长度消息。13 条服务记录目前仍同时调用 `SYS_TRACE_EMIT`，内核静态 trace 缓冲及 PRIMA witness 组装尚在；`runtime_hello` 的两条记录仍只走 syscall。`xtask` 的双通道逐条相等检查使后续去掉静态缓冲前有可执行的迁移门槛。正式阶段晋级仍受 S0-X1 独立复核约束。
