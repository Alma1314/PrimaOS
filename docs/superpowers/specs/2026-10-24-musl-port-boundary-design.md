---
id: spec-musl-port-boundary
kind: spec
authority: implementation-contract
status: implemented-with-limits
lifecycle: current
updated: 2026-10-24
---

# musl 静态移植：调用边界、fd 控制与可重放构建

## 范围与上位契约

本切片补强[原 musl 计划](../plans/2026-10-18-musl-port-plan.md)、[M12 启动/退出设计](2026-10-22-stage5-m12-c-startup-exit-design.md)和[VFS / LibOS 基础](2026-10-23-vfs-libos-foundation-design.md)。这是工作实现契约，不修改冻结 [profile v0](2026-10-18-stage5-profile-v0.md)，不发布 profile v2，不新增 Prima syscall。

Linux x86_64 调用号只属于 musl 与 C personality 之间的用户态接口。除显式的 Prima clone/线程退出和信号恢复汇编外，musl 必须通过 `__syscall0..6` 进入现有适配层；未实现的调用返回原始负 errno，不能直接陷入同号的 Prima 原语。

## 汇编替换

| 位置 | 本切片行为 | 仍未建立的能力 |
|---|---|---|
| `syscall_arch.h` | 普通调用进入适配层；移除 Linux vDSO 声明 | Linux vDSO |
| `syscall_cp.s` | 先检查 pending cancel；未取消时完整转发 n + 六参数给 `__syscall6` | 异步取消、打断进行中的 IPC、信号上下文兼容 |
| `vfork.s` | libc 返回 -1/ENOSYS，绝不发送 Linux 58（Prima sigreturn） | vfork |
| `__unmapself.s` | 保留映射，经正常 thread exit 适配路径退出与 clear_child_tid | 分离线程栈回收、完整 detach |
| `restore.s` | 使用 Prima 58，绝不发送 Linux 15（Prima unmap） | libc sigaction、掩码、siginfo、Linux 信号帧 |
| `clone.s` / `__set_thread_area.s` | 继续沿用原生 clone 48/exit 49 与 FS 设置适配 | 本切片不扩大线程模型 |

构建后反汇编整个 `libc.a`。直接 `syscall` 仅允许 `clone.lo` 的 48/49 与 `restore.lo` 的 58，额外 syscall/sysenter/int 0x80 拒绝发布缓存。该检查是锁定汇编布局的回归护栏，不是通用指令数据流证明。

## fd 控制与 stdio

- `F_GETFD` / `F_SETFD` 读取和设置每个 fd 的 `FD_CLOEXEC`；其它 SETFD 位按 Linux 行为忽略。
- `dup`、`dup2(old,new)` 清除新 fd 的 CLOEXEC；`dup2(fd,fd)` 保留原值；`dup3` 只按显式 `O_CLOEXEC` 设置。
- `F_DUPFD` / `F_DUPFD_CLOEXEC` 从 arg 起选择最低空闲 fd，范围 0..18；越界 EINVAL，无槽 EMFILE。既有 open/dup 的容量错误仍沿用原 profile 的 EAGAIN，不静默改变冻结错误集。
- fd 别名继续共享服务端句柄、offset 与打开状态。`F_GETFL` 支持文件/目录、null、tty；socket 状态控制仍不支持。文件状态来自 open 时保存的 access/APPEND/DIRECTORY/LARGEFILE。
- 服务协议尚无修改 append 的动作，因此 `F_SETFL` 仅接受已支持位且 APPEND 不变的请求；改变 APPEND 或请求其它状态位返回 EINVAL。不能先 seek 再 write 来冒充原子的 append 切换。
- `fdopen` 读模式和已经以 O_APPEND 打开的追加模式进入验收；`fopen("a")` 继续走原生 append。局部替换 `__fdopen.c`，把 GETFL/SETFL 失败传播给调用者并释放 FILE 分配，原 fd 保持打开。对非 append fd 调用 `fdopen(fd,"a")` 明确失败，不能假成功后覆盖已有内容。
- CLOEXEC 目前可观察和维护，但本切片未实现 exec 时关闭，也未建立 fork close 隔离。

## 向量 I/O

`readv` / `writev` 共用一个适配实现：先检查 fd、iovcnt（0..1024）、非零数量的 NULL iov、所有长度之和是否超过 SSIZE_MAX；长度校验全部完成后才发生 I/O。零数量返回 0，零长度向量跳过。后续向量出错时返回此前传输量；短传输或 EOF 立即结束，不能继续消费后续向量。

仅显式 NULL 指针可转换为 EFAULT；其它坏用户地址仍可能使当前用户任务 fault。多条 IPC 不提供 Linux 向量 I/O 的跨线程/跨进程原子性，也不提供取消/EINTR。本实现的总长度溢出拒绝不承诺复现 Linux 对 MAX_RW_COUNT 截断的所有细节。

## 构建输入与缓存

入口保持 `sh prima-user/musl/build.sh`，内部由 Python 3.11+ 标准库读取[快照锁](../compat/musl-snapshot.lock.toml)。不下载或升级上游版本。

1. 构建和缓存命中前都校验独立 Git checkout、锁定 commit、tracked/untracked 干净状态与 COPYRIGHT/VERSION 哈希。
2. 只导出 Git 跟踪的源文件，不复制 `.git`、ignored 的 obj 或宿主 config.mak；不删除原快照内的生成文件。
3. 八个替换文件全部从锁文件列表重放。缓存 key 包含锁文件、补丁路径/内容、构建脚本、工具版本、固定参数和工作区绝对路径。
4. 固定 clang 的 bare-metal target、静态 PIC 参数和 llvm-ar；清除会污染构建的宿主编译/头文件/库路径及 make 注入变量。
5. 默认 make -j2；`PRIMA_MUSL_JOBS` 可显式设正整数。该设置只限制 musl 编译并行度，不限制 Rust 或 QEMU CPU。
6. 文件锁串行化 sysroot 构建；重建前移除旧 stamp，安装完整且反汇编检查成功才写入新 stamp。缺失/空 libc.a 或关键头文件不能命中缓存。产物仍属于本地可信构建目录，不提供任意缓存篡改完整性证明。

## 验证与边界

`cargo xtask test-musl --host-only` 运行实际适配层的宿主测试、取消点汇编参数测试、快照/缓存/反汇编拒绝测试和 Linux 同源 F1；不启动 QEMU，不重建 sysroot。`cargo xtask test-musl` 再运行 F1 与 W4 的双轮 BIOS 验收，沿用原 30 秒期限、完整 trace、退出/清理/容量及 IPC 一致性断言。全量 `cargo xtask test` 也先运行这些 host 检查。

本切片不关闭 fork 包装续执行、任意 argv 组合回归、信号掩码、线程分离/取消、动态链接、可信随机数、完整 libc-test 或 Rust std 缺口。实跑结论见[验证报告](../reports/2026-10-24-musl-port-boundary-report.md)。
