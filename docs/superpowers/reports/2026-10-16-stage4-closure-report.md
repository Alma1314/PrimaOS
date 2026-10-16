# SPEC-130：阶段 4 原生应用、最小权限与故障隔离收口

**日期**：2026-10-16–17（最终复验 2026-10-17）
**阶段**：阶段 4（S4-D1–D5、S4-X1–X4 的实现证据）
**基线**：`be0e7ef`；本报告记录基于该基线完成的阶段 4 收口实现

## 实现与判决性证据

`prima-smoke` 的 `MemorySegment` 扩为 4 页。第 0 页保留 Hello 和原有见证字段；第 1–2 页在独立 VA 连续映射，传输 **8192 字节**；第 3 页放取消后的哨兵。客户端和服务管理器各自把同一段页 1–3 映射到自己的页表根。IPC 只传操作、长度及回复。两次数据请求的 FNV-1a 值分别为 `0xf1b1ddc5`、`0xe822ddc5`；待执行请求先取消再执行返回 `CANCELLED`，哨兵保持不变；长度 16384 被拒后，同一会话重试成功。`EVENT_NATIVE_APP=6` 原始记录为 `a=0x2000;b=0xf1b1ddc5;c=0xe822ddc5;d=0x201010101;e=0xf`，`xtask` 检查唯一性与每个字段。

`prima-driver` 在完成两条原有 I/O 应答后，从自己的 ring3 地址空间向客户端数据 VA `0xa1000000000` 写入。该 VA 只映射在客户端和服务管理器中，CPU 给出真实 `#PF`：`P=0,W=1,U=1`（错误码 `0x6`）。内核把驱动由 `Running` 转为 `Faulted`、设置唯一 `pending_exc` 并记录状态审计，随后调度其他 Ready 任务。QEMU 输出 `FAULT|task=15;addr=0xa1000000000;err=0x6;faulted=1`；同次启动的客户端随后完成两次 8 KiB 数据校验并到达 `boot_complete`。原先的用户 COW 错误码单独保存，避免驱动后续故障覆盖该见证。

服务管理器为端点创建一个**空 Namespace**，各端点的结构性 `root_ns_cap` 从它派生，仅有 COSIR §8.1 要求的 `NsResolve`；空域没有绑定或挂载。管理器仍持原有父 Namespace 以完成 `svc` 绑定，只向 Namespace 服务运行期移交可解析该路径的凭据。新建空域时还修复了一个既有对象身份问题：Namespace ID 不再复用仍被历史 Capability 引用的值，否则已销毁对象的旧凭据会在新对象出现时被误判为 `target_live`。回归测试直接验证旧目标不会复活。

### 服务初始 Capability 与后续授予

下表的 `self` 是内核按 COSIR §8.1 为每个新 Task 铸造的自管理凭据，权限固定为 `{Derive, TaskSend, TaskManageVSpace, TaskDestroy}`，目标仅为自身。普通端点的结构性根均是**空隔离 Namespace** 的 `{NsResolve}`；它不是管理器用于 `svc` 绑定的父域。

| Task | `initial_cap0` | `initial_cap1` | 结构性根 | 运行期额外凭据 |
|---|---|---|---|---|
| `prima-servicemgr` | 指向 init 的 `{TaskSend}` | 服务映像 `{Derive, SegMapR, SegMapX}` | 父 Namespace `{NsResolve}` | init 移交的父 Namespace 管理凭据、客户端共享段 |
| `prima-smoke` 客户端 | 指向服务管理器的 `{TaskSend}` | 无 | 创建时的 `{NsResolve}` | 自建 4 页共享段 |
| `alpha` / `beta` | 指向管理器的 `{TaskSend}` | 代码段 `{SegMapR}` 角色令牌 | 空隔离域 `{NsResolve}` | 按需收到客户端 Hello 段 |
| `prima-log` / `prima-store` | 指向管理器的 `{TaskSend}` | 代码段 `{SegMapR}` 角色令牌 | 空隔离域 `{NsResolve}` | 各自一份持久化段 `{SegMapR, SegMapW}` |
| `prima-ns` | 指向管理器的 `{TaskSend}` | 代码段 `{SegMapR}` 角色令牌 | 空隔离域 `{NsResolve}` | 单独收到父域 `{NsResolve}`，用于代理 `svc` 解析 |
| `prima-driver` | 指向管理器的 `{TaskSend}` | 代码段 `{SegMapR}` 角色令牌 | 空隔离域 `{NsResolve}` | 单独收到 COM1 范围的 `Device(IOPortRange)` |

该表只列创建时和本阶段协议实际授予的能力；代码页、栈页的映射由创建者凭据完成。自管理凭据的固定权限以及结构性 `NsResolve` 是现行 COSIR §8.1 的基线，故最小权限的判定是在该语义下，且由**独立空域**限制普通端点可解析的对象。

## 验证

- `cargo xtask test`：6/6 PASS，包括 BIOS QEMU 两次启动。`boot_complete`、原生应用 trace、硬件故障 trace 与既有 43 条 PRIMA 事件均通过；`ipc_ready` 为 `45/38/45`，少一次驱动故障后的阻塞接收。
- `cargo test --workspace --quiet`：799 passed，0 failed；新增 Namespace 旧凭据不复活和故障 trace 负例。
- `cargo xtask lint`：4/4 PASS（首次运行发现格式差异及 Clippy 建议，修正后重跑通过）。
- `cargo xtask tcb`：`trusted=1262`、`mechanism=20234`、机制层 `unsafe=0`，PASS。
- `git diff --check`：PASS。

## 阶段边界

S4-D1–D5、S4-X1–X4 的**实现与可复核测试证据**已齐。X3 的取消发生在执行前；退出条件只要求取消与错误恢复，并未要求执行中的并发取消。X4 覆盖单核 ring3 服务的未映射页写故障与其他应用继续运行；尚未扩展到其他 CPU 异常、多核、异常订阅者通知或故障服务自动重启。

仓库当前阶段仍按跟踪文档保持**阶段 0**：S0-X1 的组织级第三方独立复核尚未完成。因此本报告关闭阶段 4 的技术退出条件，不冒称已经完成跨阶段的正式晋级。
