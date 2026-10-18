# 阶段 5 POSIX profile v0 草案

**状态**：草案，尚未冻结；S5-D1/X1–X3 未通过。权威目标见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)，实施顺序见[修订计划](../plans/2026-10-18-stage5-revised-plan.md)。

原生 Prima syscall ABI 已先冻结一项机器契约：保留调用号 1–36，新增 `SYS_ABI_CHECK=37`。参数为 `major, minimum_minor, reserved0..3`；内核接受 `major=1`、`minimum_minor<=0` 且保留参数全零，返回 `(major<<32)|minor = 0x0000000100000000`；其余请求返回 `-EINVAL`。未登记号继续返回 `-ENOSYS`。这与 `prima-runtime::API_MAJOR=0` 的 Rust 源码 API 版本相互独立。它只验证原生调用表的兼容性，**不**代表本表的 POSIX profile 已冻结。

首个应用候选为 [sbase `echo`](https://git.suckless.org/sbase/file/echo.c.html)（[MIT 许可证](https://git.suckless.org/sbase/file/LICENSE.html)）。它需要 `argc/argv`、`strcmp`、`stdout`/`putchar`/刷新及退出状态。锁定上游 commit、源码哈希和冻结用例之后，才能把它定为 S5-X3 的目标程序；自写 C 探针仅验证启动链路。

| API / 契约 | Prima 后端与待定边界 | 状态 | 拟定测试 |
|---|---|---|---|
| C 入口 `argc/argv/envp/auxv` | 通用 ELF 装载、初始栈、段权限；入口寄存器与 auxv 尚未冻结 | planned | P5-BOOT-01 |
| `_exit` / `exit` | Task 自毁已有；退出码观察、父子关系和资源回收契约待定 | planned | P5-PROC-01 |
| `getpid` | 可映射现有 `SYS_TASK_ID`，PID 稳定性和错误边界待定 | planned | P5-PROC-02 |
| `write` | fd→Capability 与用户态 tty 字节流服务待建；短写和错误码待冻结 | planned | P5-IO-01 |
| `read` | 字节流服务、EOF/阻塞/中断语义待建 | planned | P5-IO-02 |
| `close` | 用户态 fd 生命周期、Capability drop/共享对象关系待定 | planned | P5-FD-01 |
| `openat` | 路径/目录服务待建；Namespace 仅作为授权入口 | planned | P5-FS-01 |
| `lseek` | 服务端共享偏移及不可 seek 对象错误待定 | planned | P5-FS-02 |
| `mmap` / `munmap` / `mprotect` | Segment 可复用；匿名页、flags、权限转换和 W^X 尚无完整契约 | planned | P5-MM-01 |
| `clock_gettime` / `nanosleep` | 现有 `SYS_TIME_NOW` 为 PIT tick；单位/精度/超时待定义 | planned | P5-TIME-01 |
| `pthread_create` / `pthread_join`、mutex/cond | 无真实 Thread/TLS 与等待语义；不得以成功空实现占位 | unsupported | P5-THREAD-01 |
| `socket` / `bind` / `sendto` / `recvfrom` | 无基础网络服务及地址族承诺 | unsupported | P5-NET-01 |

`strcmp` 等纯 C 函数和 stdio 符号由[静态 musl 移植计划](../plans/2026-10-18-musl-port-plan.md)逐项追溯；表中 `planned` 只表示目标，不表示实现。`partial` 必须列出支持的参数/flag、错误和未覆盖行为；每行在变为 `supported` 前须补 Linux 参考输出、Prima QEMU 正反例和可重复命令。BLK-003 的 ABI 编号、命名、错误映射及版本握手未裁决，本草案不分配新 syscall 号。
