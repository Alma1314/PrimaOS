# 阶段 5：服务后静态容量水位

**日期**：2026-10-18

在 [Task 16 QEMU 见证](2026-10-18-stage5-task16-qemu.md)基础上，内核对 Capability 活动槽和排队消息记录全程高水位；Segment 表槽不复用、Audit 只追加，两者收尾计数即高水位。`CAPACITY` 串口行在 `boot_complete` 前输出，xtask 对 BIOS QEMU 两轮逐字段验收。

| 池 | 实测峰值 / 上限 | 收尾状态 | 耗尽负例 |
|---|---:|---:|---|
| Capability 活动槽 | 120 / 144 | 114 | host：槽满后 `CapacityExceeded` |
| Segment 表槽 | 31 / 48 | 31（单调分配） | host：表满时原子拒绝 `CapacityExceeded` |
| 排队消息 | 14 / 32 | 1 | host：满队列返回 `QueueFull`，出队后峰值保留 |
| Audit 事件 | 638 / 768 | 638（只追加） | host：日志满时 Task 迁移返回 `AuditFull` 且状态不变 |

Task ID 16 的 ring3 创建、自报、销毁、根清理及重复创建 `ENOSPC` 由前述 QEMU 见证覆盖。服务映像 67 / 80 页；为首个 C ELF 预留的 13 页仍需用实际产物验证。

验证：`cargo test --workspace --quiet`、`cargo xtask lint`、`cargo xtask test` 均 PASS。**BLK-004 保持 Open**，直到首个静态 C ELF 在全量服务场景中实际创建、运行、退出并回收；届时复测上表水位。此报告不证明长期运行时 Audit 永不耗尽，亦不签署 S5-D1–D7/X1–X3。
