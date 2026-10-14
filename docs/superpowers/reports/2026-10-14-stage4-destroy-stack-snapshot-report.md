# SPEC-107 报告：销毁路径 vspace 栈快照治本（滚动拆除取代栈上 VA 快照）

> 状态：探索实现；**不构成 S4-D2/D3 验收或稳定 ABI**，不勾选任何阶段 4 交付项/退出条件
>
> 依据：[SPEC-106 报告](2026-10-14-stage4-three-base-services-report.md) §5.4（`SYS_TASK_DESTROY`
> 栈上快照数组随 `MAX_VSPACE_ENTRIES` 扩容致 16 KiB 内核栈越界）与 §7.3（治标登记）
>
> 前置：SPEC-106
>
> 基线：SPEC-106 HEAD `46c2733`

## 1. 本片做了什么

消除**「容量常量放大即挤压内核栈」**这条隐式耦合在 vspace 拆除路径上的那一支。

- **两处 `[0u64; MAX_VSPACE_ENTRIES]` 栈快照 → 「找到一个、拆一个」的滚动循环**：
  `BootstrapState::task_destroy_self`（§8.2 自毁路径）与 `teardown_vspace`（`task_destroy` /
  内核回收路径共用）原先各自在栈上开一个 `[u64; MAX_VSPACE_ENTRIES]`（96 × 8 = **768 B**）
  收集目标任务的映射 VA，再逐个 `unmap_4k` + `remove`。现按 **SPEC-107** 改为滚动：
  每轮取该任务的**首个**映射 VA，拆掉后再取下一个，直至无剩余。
- **新增 `TaskVspaceTable::first_mapping`**：返回该任务的最小下标映射 VA。拆除顺序与旧快照
  一致（`for_each_entry` 顺序 = arena 下标序），故**语义等价**（旧实现也是按同序拆）。
- **代价（deliberate，带 `ponytail:` 注释）**：扫描由 O(n) 变 O(n²)，n ≤ `MAX_VSPACE_ENTRIES` = 96，
  且只在内核销毁路径（非热路径）。以 9216 次比较换掉 768 B 栈占用，取 O(n²) 而非「静态 scratch +
  `unsafe`」——机制层保持零 `unsafe`。
- **本片不触碰** `destroy_task_body` 的两处小栈数组（`[u32; MAX_MESSAGES]` = 128 B +
  `[Handle; MAX_MESSAGES × 2]` = 512 B，见 §3）。

## 2. 判决性见证

- **43 条 QEMU 事件逐字节冻结**：`cargo xtask test` 六条 PASS，`xtask/src/protocol.rs` 的
  `VALID` 事件流夹具与全部负例**一字未改**——本切片无任何事件字段变化（无需登记）。
- **栈数组消失**：`grep -n "\[0u64; MAX_VSPACE_ENTRIES\]" kernel/src/` 无匹配；
  `MAX_VSPACE_ENTRIES` 现仅出现在 `VSPACE_ARENA` 声明与 `insert` 的容量检查中（静态存储，不占栈）。
- TCB 行数：`trusted_lines=1262`（不变）、`mechanism_lines` 21196 → **21189**（-7，滚动实现比
  两处快照更短）、`total_lines` 22458 → **22451**、`ratio=5.62%` 不变、机制层 `unsafe=0`。

## 3. 反向验证（**负面结果，如实登记**）

把 `KSTACK_SIZE` 临时降回 **16 KiB** 重跑 `cargo xtask test`：**仍在 seq 38 复现
`panic|status=error`**（与 SPEC-106 §5.4 完全同一症状），且 `frames_ready` 因 BSS 缩小而顺移
（`universe` 31358 → 31428、`image` 1122 → 1121）。

**结论**：16 KiB 内核栈下的溢出源**不止** vspace 快照一处——本片只消除了其中 768 B×2 的一支。
该路径上至少还有：`destroy_task_body` 的 640 B 小栈数组、syscall 分派层与 `sched::resume` 的
现场保存帧、以及被指向的 `MAX_VSPACE_ENTRIES` 扩容所放大的其它随容量常量增长的占用。
故 **`KSTACK_SIZE` = 32 KiB 仍然保留**（不降回），本片**不宣称**「整体治本完成」。
（16 KiB 试验后已恢复 32 KiB；最终提交态为 32 KiB，事件流冻结如 §2。）

## 4. 未决与登记

1. **整体治本未完成**：§3 的负面结果说明销毁路径仍有随容量常量增长的栈占用。若要真正去掉
   「扩容即越界」，须继续把 `destroy_task_body` 的 `dropped_caps`/`dropped_message_ids`
   与 syscall/resume 栈帧一并下沉到静态/arena，此后 `KSTACK_SIZE` 才有条件回落。
2. `first_mapping` 的 O(n²) 仅在 n 显著增长（≫ 96）时才值得改回批量收集；当前刻意保留。
3. SPEC-105 §5.2 的 `#PF` 分派器固定 3 字节跳过（`cow.rs::dispatch`）仍未改（承接 SPEC-106 §7.4）。
4. 本片**不勾选**任何 S4-D2/D3 或阶段 4 交付项/退出条件；闭合判断交复核者。

## 5. 验证（2026-10-14）

- `cargo test --workspace`：PASS（**788 项**：cosir 146 + cosir_state 42 + lean 3 + kernel 369 +
  prima-ostd 21 + prima-runtime 1 + xtask 206）。
- `cargo xtask lint`：四条 PASS（fmt / `clippy -D warnings` / 内核 `deny(unsafe_code)` /
  「机制层零 `unsafe`」27 文件扫描）。
- `cargo xtask tcb`：见 §2。
- `cargo xtask test`：六条 PASS（事件流逐字节不变）。
- QEMU 直跑：`boot_complete`，退出码 33。
