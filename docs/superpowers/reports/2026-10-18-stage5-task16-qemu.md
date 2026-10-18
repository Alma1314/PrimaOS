# 阶段 5：服务后 Task 16 的 QEMU 见证

**日期**：2026-10-18

## 实现与结果

六个服务端点完成现有探针、客户端完成数据往返后，ring3 服务管理器复用已装载的驱动 ELF 创建第七个端点。它取得 Task ID 16，在 ring3 自报；管理器销毁该 Task、销毁独立栈 Segment，并显式释放收到的凭据、栈和管理 Capability。随后用合法请求再次创建 Task，获得 `ENOSPC`（Dead ID 不复用）。该探针验证原生 Task 机制的容量，不是 C 程序或 POSIX 入口。

新增共享 trace 事件 7 验证 `id=16`、销毁、栈回收、重复创建拒绝及原服务数 6。内核收尾独立打印 `TASKCAP`：`dead=1`、`root_cleared=1`、活动 Task 11、活动 Capability 114。既有 IPC 计数增加为 queued/direct/blocks = 47/39/47；QEMU 双跑与 xtask 协议校验均通过。服务映像实际 67 页，单段上限扩至 80 页，给后续程序留 13 页；超过上限仍由装载检查拒绝。

## 验证与边界

| 命令 | 结果 |
|---|---|
| `cargo test --workspace --quiet` | PASS，含阶段 5 trace 正反例与既有工作区测试 |
| `cargo xtask lint` | PASS，含裸机内核构建与零 `unsafe` |
| `cargo xtask test` | PASS，BIOS QEMU 两轮、UEFI 镜像构建与既有启动验收 |

BLK-004 仍 Open：本次取得 Task 生命周期和 Task/Capability 收尾计数，尚未记录 Cap、Segment、消息队列和 Audit 的**全程峰值**及耗尽负例；当前审计收尾已用 638/768。下一片须补容量水位与确定性失败语义，并验证真正的 C ELF 装载。BLK-003 的 ABI 裁决与 musl 移植不在本见证范围，S5-D1–D7/X1–X3 仍未勾选。
