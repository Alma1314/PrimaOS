# 阶段 5 / M17b-2a：COW 页表原语实现报告

**日期**：2026-10-23
**范围**：fork 的 COW 页表原语（无 syscall/故障接线）。设计见
[M17 设计](../specs/2026-10-23-stage5-m17-fork-exec-design.md)。
**结论**：`vspace::clone_root_cow` 与 `resolve_cow` 在 host 页表模型上通过
用例：父子共享叶帧、可写叶两侧置 COW、写故障复制后恢复可写且父页不变；
只读叶共享、supervisor 项原样复制。QEMU 接线（general user COW 故障处理、
帧归属、SYS_FORK/W6）留后续片。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/vspace.rs` | `PT_COW`（PTE bit 9）；`clone_root_cow` 深拷贝用户页表层级、共享叶帧、可写叶父子两侧清 `PT_WRITABLE`+置 `PT_COW`；`resolve_cow` 校验 present/user/COW、复制 4 KiB、恢复可写并清 COW |
| host 用例 | `cow_clone_shares_frames_and_resolve_copies_on_write`、`cow_clone_keeps_readonly_shared_and_copies_supervisor_entries` |

## 2. 语义

- 克隆前 `dst_root` 必须清零；非 user 的 PML4 项原样复制（内核高半区共享）。
- 可写 4 KiB 叶：父子 PTE 都改为只读 + `PT_COW`，共享同一数据帧；写故障经
  `resolve_cow` 分配新帧、复制内容、把该侧 PTE 恢复可写，另一侧仍是 COW
  旧帧（写隔离）。
- 只读叶直接共享，不置 COW；大页（当前用户映射中不存在）按原样共享。

## 3. 证据

```text
cargo test -p kernel cow_clone     → 2 passed
cargo test --workspace             → 全绿
cargo xtask lint / fmt             → PASS
```

## 4. 边界与下一步

- 未接 general user COW 故障分派（现 `cow::dispatch` 只处理固定见证 VA）；
- 未决定复制帧的归属（段池/专用 COW 段），因此未接 `SYS_FORK`/W6；
- 深拷贝实验已证明全量复制不可行（约 1.8 MB `.bss` 耗尽保留页表帧池），
  COW 是必由路径。
- 下一步：general user COW 故障（`resolve_cow` + 帧归属 + invlpg + 重试）、
  fork syscall、W6（COW 隔离/退出码/ECHILD）。

## 5. 本报告不代表

- 不表示 fork 用户态可用；
- 不表示阶段 5 退出条件满足。
