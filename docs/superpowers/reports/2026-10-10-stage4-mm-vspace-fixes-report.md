# SPEC-086 报告：阶段 4 期内的 mm/vspace 正确性与性能修复批次

> 状态：**回溯登记**——代码已于 2026-10-05 经 `7b49d1a` 合入 master；不勾选任何交付项或退出条件
>
> 说明：本批六提交（`4b0b039`/`dd1c08b`/`def4f91`/`f23ca86`/`cde811b`/`234b7e2`）来自分支 `codex/optimize-frame-allocation`，合并时**无提交说明、无设计/报告**，理由与前置条件只存在于代码注释里。本报告据代码与测试回溯记录其缺陷、决策、前置条件与证据。

## 1. 背景与方法边界

- 该分支自 `f4f3441` 分出，经 merge 提交 `7b49d1a` 与 stage4（`8a25e49`）**一并**合入 master（`git log --graph` 可见双亲结构）。
- **因此未重建 pre-merge 事件流 diff**：该 diff 会与同时合入的 stage4（嵌入用户 ELF）引起的镜像布局变化纠缠，无法干净归因。当前 39 条事件基线是冻结真值，本批的内部 mm/vspace 改动已折叠其中。**这是方法边界，如实登记。**
- 六项均带 host 单测（下逐条列出）。

## 2. 逐项

### 2.1 `def4f91` 帧归还：先做可能失败的步骤

- **缺陷**：`release_frame`/`release_table_frame` 先压缩 `owners`/`table_frames` 数组并递减计数，**再**调分配器的 `release`。若分配器拒绝（帧不在池、重复归还），归属记录已丢失而帧未真正归还 → 状态分叉（帧"消失"、池计数不符）。
- **决策**：把 `alloc.release` 提到归属变更**之前**；失败返回 `FrameError::NotOwned` 且结构体逐字段不变。
- **容量**：`MAX_RECYCLED_FRAMES` 256 → **320**（= `MAX_FRAME_OWNERS` 192 + `MAX_TABLE_FRAMES` 128）——"全部持有帧同时归还"必须有回收位，否则归还被误拒。
- **测试**：`rejected_release_preserves_frame_ownership`（拒绝后 `registry == before`）、`all_live_frames_can_be_returned_without_filling_the_recycle_pool`。

### 2.2 `f23ca86` 启动内存区：拒绝溢出与反向区间

- **缺陷**：`align_up` 用 `div_ceil`，接近 `u64::MAX` 时回绕；`region.start > region.end` 无检查；`kernel_addr + kernel_len` 未检溢出。
- **决策**：`align_up -> Option<u64>`（`checked_add`），**不可表示的尾区跳过**；`start > end` 判 `RegionsNotSortedOrOverlapping`/`InvalidRange`；镜像区间溢出报新错误 `MemError::InvalidImageRange`。
- **测试**：`rejects_reversed_regions_and_skips_unrepresentable_tail`（反向区间 + `u64::MAX` 附近尾区）、`rejects_image_address_overflow`。

### 2.3 `cde811b` `leaf_phys_or_none`：大页 PAT 与页内偏移

- **缺陷**：①2 MiB/1 GiB 大页项的 **bit 12 是 PAT** 而非物理基址位，未掩掉会把 PAT 混入基址；②4 KiB 叶返回**帧基址**而非含页内偏移的翻译结果。
- **决策**：大页基址掩 `page_size - 1`；叶返回 `(entry & PT_ADDR_MASK) + (va & (FRAME_SIZE - 1))`。
- **语义变更（登记）**：返回值由"帧基址"变为"翻译后的物理地址"。全部调用方（`user.rs` 三处、`bootstrap.rs` 一处）都只用 `.is_some()` 做**页对齐**存在性判定（偏移恒为 0），故当前无破坏——但语义已变，后续新调用方须按"翻译地址"使用。
- **测试**：`leaf_phys_or_none_ignores_huge_page_pat_bit`（2 MiB 与 1 GiB 两例）、`leaf_phys_or_none_resolves_two_mib_huge_pages`（新增 `Some(0x60_1234)` 断言，钉住页内偏移）。

### 2.4 `234b7e2` 4 KiB 走表：路径上有大页即拒绝

- **缺陷**：`count_missing`/`unmap_4k`/`walk_to_leaf_table` 只判 present；遇 PDPT/PD 大页会把大页项当**页表指针**继续下走（把大页所描述的物理区当页表读）。
- **决策**：新增 `PT_HUGE`（bit 7）；三条走表路径遇大页返回 `BadRoot`；`memseg_map` 的重叠前置由 `read_leaf(..)+present` 改为 `leaf_phys_or_none(..).is_some()`（前者对大页路径会误判）。
- **测试**：`four_k_walk_rejects_a_huge_page_in_the_path`——`map_4k`/`read_leaf`/`set_leaf`/`unmap_4k` 四路均 `BadRoot`，且大页之下的数据未被改写。

### 2.5 `4b0b039`（perf）仅为当前根刷 TLB——**前置条件**

- **决策**：`needs_local_tlb_flush(target, current) = current == Some(target)`；`memseg_map`/`memseg_unmap` 只在目标根 == 当前根时 `invlpg`。
- **健全性前置（本片显式登记）**：单核、无 PCID ⇒ 唯一可能缓存用户页翻译的是**活跃 CR3 所在的根**；非当前根在下一次 `sched::resume` 写 CR3 时自然失效。
- **已核查的支撑事实**：全仓**唯一**的 CR3 写入点是 `sched::resume`（`crate::cpu::write_cr3`），且与 `set_current` **成对**发生——故凡 CR3 持有某任务根时 `current_root()` 即等于它。boot 期 `current_root()` 可为 `None`，此时活跃根不是任何任务根（`memseg_map` 也只针对任务根），跳过刷新成立。
- **登记为复核项**：该不变量此前**未在任何文档中显式声明**。若将来新增任何"写 CR3 后不更新 `current`"或"经 `memseg_map` 映射进 boot 根"的路径，本优化会**静默**产生陈旧翻译。
- **测试**：`tlb_flush_only_for_current_root`（`Some(同)`/`Some(异)`/`None` 三例）。

### 2.6 `dd1c08b`（perf）区间游标与批量清零

- **决策**：分配器增 `next_range` 游标，已耗尽区间不再重扫；`check_invariants` 新增两条（`index < next_range ⇒ range.next == range.end`、`index ≥ next_range ⇒ range.next < range.end`）使"无检查读当前区间"安全；`fill_zero` 改收 `usize` 以批量清零。
- **测试**：`allocator_skips_exhausted_ranges_and_reuses_recycled_frames`。

## 3. 验证（2026-10-10，当前 master）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 365、prima-ostd 21、xtask 192（本批的单测含在内） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1250 mechanism_lines=19671 ratio=5.97%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（39 条事件） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

## 4. 未决与登记

1. **未重建 pre-merge 事件流 diff**（见 §1 方法边界）。
2. **§2.5 的 TLB 前置不变量**此前未显式声明，本片补登记；后续任何 CR3 写入路径或 `memseg_map` 目标面的变化须重核。
3. **§2.3 的返回值语义变更**为静默兼容（现有调用方恰好只用存在性判定），登记。
4. 本批来自独立分支且合并时无理由记录，**其余提交（`4b0b039`/`dd1c08b` 之外）的原始意图由本报告据代码回溯**，非作者自述。
5. 不勾选任何交付项或退出条件（沿用组织级独立性门槛）。
