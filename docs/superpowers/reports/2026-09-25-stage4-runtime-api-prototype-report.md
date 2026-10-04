# SPEC-084 报告：阶段 4 原生运行时 API 初始实现

> 状态：API 原型；不构成 S4-D1 验收或稳定 ABI
>
> 设计依据：[原生运行时 API 与版本规则设计](../specs/2026-09-25-stage4-runtime-api-design.md)

## 交付

新增 workspace crate `prima-runtime`，目前为 `no_std`、`forbid(unsafe_code)`；依赖 `prima-ostd` 与仅含常量的内部 `prima-abi`。

- `Runtime<T>` 通过公开 `SyscallTransport` trait 隔离传输实现；应用不接触 syscall 编号与寄存器约定。传输调用必须同步完成，不能在返回后继续使用用户缓冲区指针。
- `X86_64Transport` 调用 `prima-ostd::entry::syscall_from_user`；框架先检查 CS 的 CPL 为 3，再按 RAX/RDI/RSI/RDX/R10/R8/R9 约定执行 `syscall`。非用户态调用返回 `TransportError::NotUserMode`。
- 首批封装 Capability 派生/显式丢弃、Segment 创建/映射/解除映射/销毁、消息发送/接收、Task 创建/销毁/让出与 ID 查询、Namespace 创建/绑定/解析/挂载/销毁，以及 Timer 创建/取消/销毁与时间查询。
- `Capability` 包装检查当前槽索引/generation 编码，不声称 token 存活或具有权限；不通过 Rust `Drop` 隐式释放 Authority。
- 服务协议提供显式字节编码/解码（固定 20 字节 `PRIM` 消息头、24 字节 Hello offer，little-endian）以及 `ProtocolOffer`/`negotiate`：主版本必须相同且非零，次版本取较小值，feature 取交集，必需 feature 缺失时拒绝协商，消息上限取较小值。
- 错误类型保留已知内核 errno 的分类和未知 errno 数值。成功值只在精确匹配本原型 errno 编码时转为已知错误，其他高位值仍保留为成功结果。

## 边界与未完成项

1. x86_64 指令入口已实现并通过编译检查，但本次没有从真实 ring3 用户任务调用它；MSR 配置、syscall/sysret 往返及内存缓冲可达性仍需 QEMU 端到端验收。
2. syscall 编号存于 `prima-abi`，内核与运行时共用单一映射；该 crate 只供仓库内部使用，编号仍是实验性映射，不承诺跨版本稳定，也不是应用应直接依赖的 API。
3. 服务握手规则和首版固定 wire format 已实现为纯编码/协商接口；IPC 传输、客户端/服务端互通证据和真实服务端点仍待后续切片。
4. `TaskSpec`、IPC 与 Namespace 包装已加入，但还没有被独立 ring3 Rust 程序或真实服务端使用。
5. Capability 当前包装为可复制 token；内核单位置模型仍是 Authority 真值，复制 Rust 值不会复制内核 Capability 位置。

## 验证

`cargo check --workspace`、`cargo check -p prima-runtime --target x86_64-unknown-none`、`cargo clippy -p prima-runtime --all-targets -- -D warnings`、`cargo fmt --all --check`、`cargo xtask lint`、`cargo xtask build` 与 `cargo xtask tcb` 通过（2026-09-25）；镜像命令生成 BIOS 与 UEFI 镜像但未启动 QEMU。lint 包含内核 x86_64-unknown-none 构建和 `kernel/src` 零 `unsafe` 门禁。TCB 输出 `trusted_lines=1252 mechanism_lines=18665 ratio=6.29%`，`unsafe_tokens trusted=55 mechanism=0`。本轮未运行测试，也不据此宣称 QEMU 往返、服务互通或阶段退出条件通过。
