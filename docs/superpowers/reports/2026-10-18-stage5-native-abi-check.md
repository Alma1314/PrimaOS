# 阶段 5：原生 ABI 版本握手见证

**日期**：2026-10-18。**范围**：BLK-003 中的调用号与版本握手子项；POSIX profile 未冻结。

`prima-abi` 保留原生 syscall 号 1–36，追加 `SYS_ABI_CHECK=37`。调用参数依次为期望 major、最低 minor、四个必须为零的保留字；内核当前版本为 1.0。兼容请求返回 `0x0000000100000000`，major 不同、最低 minor 过高或保留字非零均返回 `-EINVAL`。未知调用号继续返回 `-ENOSYS`。该版本只描述 Prima 原生 syscall 协议，与 `prima-runtime` 的 Rust 源码 API 版本分开。

宿主单测覆盖成功及三种拒绝；无 libc 的 C Task 16 在任何身份上报前执行相同四项检查，只有全通过才向服务管理器发原有 `0x5a` 标记。双轮 BIOS QEMU 验收继续核对标记、Task 自毁、资源水位和旧阶段 trace。验证命令：`cargo test --workspace --quiet`、`cargo xtask lint`、`cargo xtask test`，均通过。

这仅确认实际 ring3 C 程序能协商原生调用表。C 启动栈、退出状态、POSIX 错误映射、fd/字节流与 libc 仍待实现；BLK-003 保持 Open，S5-D1–D7/X1–X3 未勾选。
