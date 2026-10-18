# 阶段 5 启动：首个新 Task 容量与 profile 草案

**日期**：2026-10-18

**范围**：启动阶段 5 的代码切片和准入工件；未签署阶段 5 交付项或退出条件。

## 改动与依据

- `kernel::bootstrap::MAX_TASKS` 从 16 扩至 17，允许既有 Task 1–15 占位之后分配 ID 16；`TaskTable::with_init` 按常量初始化，避免手写数组长度漂移。
- 调度器直接复用该常量，保证 Task 表与上下文表范围一致。现有内核栈池覆盖 ID 3–18，不需扩容。
- `stage5_has_one_task_id_after_existing_services` 验证 ID 2–15 占用后可分配 16，再次分配被拒绝。
- [profile v0](../specs/2026-10-18-stage5-profile-v0.md) 已冻结首批 C/POSIX 契约、BLK-003 裁决、sbase `echo` 锁定快照与 E1–E6 冻结用例。

## 已验证

| 命令 | 结果 | 范围 |
|---|---|---|
| `cargo test --workspace --quiet` | PASS；kernel 395 项及其余 workspace 测试全绿 | host 单测；含新边界测试和 `BootstrapState` 尺寸守卫 |
| `cargo xtask lint` | PASS；fmt、Clippy、bare-metal build、kernel 零 `unsafe` | 静态检查与交叉构建 |
| `cargo xtask test` | PASS；BIOS QEMU run 1/2、UEFI 镜像构建、阶段 1 启动切片 | 既有 boot 路径无回归 |

## 未闭合边界

BLK-004 保持 Open：QEMU 双跑执行了既有服务，但尚未在全部服务存活时**实际创建、运行、退出并回收** ID 16，也未记录 Cap/Segment/消息/Audit 峰值和容量失败语义。下一片应加入独立 ring3 Task 及机制层见证，再据实决定是否关闭。BLK-003 也保持 Open；musl 未移植，S5-D1–D7/X1–X3 均不勾选。
