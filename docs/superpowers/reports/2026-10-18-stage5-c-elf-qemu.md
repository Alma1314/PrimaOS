# 阶段 5：首个 C ELF 的服务后 QEMU 见证

**日期**：2026-10-18；**范围**：BLK-004 容量门槛，不是 POSIX/libc 验收。

`prima-user/c/stage5_probe.c` 由 `xtask` 调用 clang 22.1.4 以 `-target x86_64-unknown-none -nostdlib -static-pie` 构建，并作为 opaque kind `0x101` 加入 boot manifest。产物是 3 页 ET_DYN 静态 PIE：`readelf` 确认无 `PT_INTERP`、无 `DT_NEEDED`、无重定位。服务映像总计 70/80 页。构建主机现需 clang 与 lld；本次没有引入 musl、sysroot 或 Linux 动态加载器。

六个原生服务及客户端数据往返完成后，ring3 服务管理器从映像目录读取 C ELF 的页偏移和入口，创建 Task 16 并映射独立栈。C `_start` 收到现有五寄存器入口参数，经 `SYS_TASK_ID` 取得自身 ID，以 `SYS_SEND` 上报 `0x5a` 标记和 ID，随后以自管理 Capability 调用 `SYS_TASK_DESTROY` 自毁。管理器收到标记后销毁栈 Segment、释放持有的句柄，再用合法请求复建，得到 `ENOSPC`。共享 trace 事件 7 验证 `[id=16, C 标记=0x5a, 自报=1, 栈回收=1, 复建拒绝=1]`；内核独立输出 `TASKCAP|id=16;dead=1;root_cleared=1;tasks=11;caps=114`，防止仅凭用户消息宣称退出。

| 资源 | 本次实测峰值 / 上限 | 收尾 |
|---|---:|---:|
| 活动 Capability | 119 / 144 | 114 |
| Segment 表槽 | 31 / 48 | 31 |
| 排队消息 | 14 / 32 | 1 |
| Audit 事件 | 629 / 768 | 629 |

原 IPC 计数更新为 queued/direct/blocks = 45/39/46。上述数值由 `xtask` 对 BIOS QEMU **两轮**逐字段核对；宿主侧 Cap、Segment、消息、Audit 耗尽负例见[前一水位报告](2026-10-18-stage5-capacity-watermarks.md)。`cargo test --workspace --quiet`、`cargo xtask lint`、`cargo xtask test` 均通过。旧阶段 1–4 的 43 个启动事件仍由同一验收器核对。

**结论**：BLK-004 所需的全量服务后新增 C Task 创建、执行、自毁、栈回收、容量水位与确定性耗尽已具备证据，按此范围关闭。此 C 入口仍是 Prima 原生五寄存器约定；没有 `argc/argv/envp/auxv`、POSIX `_exit` 状态、`write(1/2)`、ELF 页权限/W^X 验证，也没有 libc 或 Linux/Prima 同源码差分。它们留在阶段 5 的 M1/片 C 及后续交付，BLK-003 仍 Open。

后续固定探针的初始栈见[独立报告](2026-10-18-stage5-c-start-stack.md)；本报告保留当时的容量验收范围。
