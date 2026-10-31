---
id: spec-stage6-streamed-elf-input
kind: specification
authority: scoped-loading-contract
status: implementation-contract
lifecycle: current
updated: 2026-10-31
---

# S6-M2：按需读取原生 exec 的 ELF 输入

## 范围与上位关系

本切片推进[阶段 6–7 计划](../plans/2026-10-27-stage6-7-refinement.md)的 S6-M2，
局部细化[M17c-2 原生 exec 契约](2026-10-23-stage5-m17c2-exec-syscall.md)的输入读取。
延续[装载器硬化](../reports/2026-10-28-stage6-loader-slice.md)与
[初始栈后继](../reports/2026-10-29-stage6-start.md)，不发布 Linux profile 或新 syscall。
本契约适用于共用 `exec_call` 的原生 55/62/69；VFS 文件缓存与 LibOS 仍由用户态负责。
实现和有界证据见[本轮报告](../reports/2026-10-31-stage6-streamed-elf-input.md)。

## 输入与失败

- `image_len` 是文件的声明长度，供程序头表与文件段边界检查；不再要求 ≤1 MiB。
  `image_ptr + image_len` 必须不溢出；溢出返回 EFAULT。
- 先读取完整的 64 字节 ELF 头，再逐条读取完整的 56 字节程序头；不假定程序头表
  落在首个 4 KiB，也不复制程序头之间的空洞或整个文件。程序头数仍由 ELF64 的 u16
  字段界定，单次元数据缓冲大小不随文件长度、偏移或程序头数增长。
- 在读程序头之前检查 `phoff + phnum * 56` 的算术与声明文件边界；越界、截断和
  现有 ELF 格式错误返回 ENOEXEC。实际源页不可读返回 EFAULT，保留两类错误的区别。
- 页表按每个实际读范围验证。未被头部或段复制覆盖的文件空洞无需映射；声明长度不是
  访问其它 Task、隐藏页、设备页或内核页的授权。
- 段数据沿用现有逐页、最多 4096 字节的 copy-in。全部新映射先在私有根构造；输入读取
  失败不能替换旧根、旧用户映像或返回现场。失败后沿用既有 staging 回收路径。
- 现有静态 ET_EXEC/x86_64、无 PT_INTERP、段同余、W^X、入口、重叠、8 个 PT_LOAD、
  argv/envp、帧归属与资源准入检查继续适用；文件长度不等于需要分配的映像页数。

## 验证与边界

host 检查任意程序头偏移、声明边界、读取故障身份及停止读取；W7 新增稀疏输入：
程序头跨越两个非首页源页，代码/数据偏移超过 2 MiB，未使用的文件空洞在 Prima 上不可读。
新程序必须读取第一载入页、从第二载入页执行，并得到指定退出码。原生负例在同一 Task
中破坏程序头页和第二载入页，验证 EFAULT 后旧程序仍运行，恢复后能够 exec。
Linux 同源只比较成功的合成 ELF 装载；原生页表故障负例不冒充 Linux 差分。

这不扩大 `MAX_SEGMENT_FRAMES=149`、全局帧归属/逻辑映射容量或 8 页初始栈，
也不提供大文件的 VFS 来源。稀疏合成 ELF 仅需两页段数据，不能替代白名单 busybox 的
526 页真实装载、Gate 递送或 Linux LibOS 验收。共享 root、异步输入读取、动态 ELF 或
扩展物理容量时须重审此契约和事务边界。当前阶段与退出条件只见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。
