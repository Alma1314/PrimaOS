# 阶段 5：C Task 初始栈见证

**日期**：2026-10-18。**范围**：M1 的固定 C 探针启动栈；不是通用 POSIX `exec` 或 libc 验收。

服务管理器为 Task 16 自建一页栈段，先映射到自身地址空间，写入 16 字节对齐的入口栈，再解除临时映射并映射给新 Task。栈从 `ENDPOINT_STACK_VA+0xf00` 开始，依次是 `argc=2`、`argv={"stage5-probe","stack-ready",NULL}`、`envp={"PRIMA_ABI=1",NULL}`、`auxv={(AT_PAGESZ,4096),(AT_NULL,0)}`。其他原生服务仍使用原入口栈。

C ELF 的汇编 `_start` 从实际 `rsp` 获取栈地址，再按 x86_64 C 调用约定交给无 libc 的 C 函数。C 程序逐项核验对齐、指针、字符串和终止项；任何不符都会阻止原有 `0x5a` 标记，故 QEMU 验收无法误判启动成功。随后它验证原生 ABI、上报 Task ID 并自毁。`readelf` 确认产物为 x86_64 ET_DYN、无重定位；既有容量和内核自毁见证继续成立。

`cargo test --workspace --quiet`、`cargo xtask lint`、`cargo xtask test` 通过；后者在 BIOS QEMU 双轮核验原有阶段 1–4 trace、C 标记、Task 16 回收和容量水位。

边界：目前只构造这一个探针的固定参数；未实现通用参数/env 传递、完整 auxv、退出码、fd 字节流、libc 或 ELF 各段权限验收。BLK-003 保持 Open，S5-D1–D7/X1–X3 均未勾选。

后续固定探针的 `main` 返回码见[独立报告](2026-10-18-stage5-main-return-status.md)；本报告保留当时的启动栈验收范围。
