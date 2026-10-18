# 阶段 5：同源 C `main` 返回码见证

**日期**：2026-10-18。**范围**：固定 C 探针的 `main` 返回码及用户态观察者；不代表通用 POSIX `_exit`/`wait`。

同一份 `prima-user/c/stage5_probe.c` 在 Linux 宿主以 clang 编译，`cargo xtask test` 自动运行三组输入：`stack-ready` 与 `PRIMA_ABI=1` 返回 37；缺参数返回 2；缺环境变量返回 3。Prima 的无 libc 汇编 `_start` 读取初始栈，C 入口核验栈和原生 ABI 后调用同一个 `main`。成功返回的低 8 位通过已有 `SYS_SEND` 发送给服务管理器，服务管理器从消息解析 37 并写入共享 trace；随后 C Task 自毁。QEMU 验收核对事件 7 的 `[Task ID=16, 标记=0x5a, 返回码=37, 栈回收=1, 容量拒绝=1]`，还独立核对内核 `TASKCAP` 的 Dead 与根清理。

`cargo test --workspace --quiet`、`cargo xtask lint`、`cargo xtask test` 均通过；BIOS QEMU 两轮通过，原阶段 1–4 事件仍在同一验收器内核对。`readelf -r` 确认 C ELF 无重定位。

此处状态由 C 程序在自毁前发送，观察者为服务管理器。尚无可供任意程序调用的 `_exit`、可查询的进程状态、父子等待、异常退出状态或 Linux guest 基线；只有成功输入在 Prima QEMU 执行，失败输入目前由 Linux 宿主验证。BLK-003 继续 Open，S5-D1–D7/X1–X3 均未勾选。
