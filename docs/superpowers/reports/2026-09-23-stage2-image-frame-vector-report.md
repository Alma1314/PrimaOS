# 切片 W 报告：image Segment 的真实逐帧向量（ISSUE-FRAME-001）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-23
>
> 对象：`kernel/src/{frames,mem,bootstrap}.rs`
>
> 目的：交付 `ISSUE-FRAME-001`——把 image Segment 的 `frames` 从 `(base, count)` 区间表达
> 改为 §5.1 要求的真实逐帧 `Vector<FrameId>`，并让逐帧归属查询覆盖它

## 0. 结论速览

- **逐帧向量已落地**：`FrameRegistry` 持有 `image_frame_ids: [u64; 2048]`（`image_count` 为其
  长度），由 `init_at` 从 `kernel_phys_start + i * FRAME_SIZE` 一次性物化；
  `owner_of`/`check_disjoint`/`check_conservation`/`witness` 全部改读该向量，
  `SegmentFrames::Contiguous` 区间变体**删除**，`SegmentFrames::Image { count }` 的 `frame_at`
  逐帧委托注册表。**不再存在"区间判断"这条旁路。**
- **向量落在 `BootstrapState` 之外**：16 KiB 结构体守卫、`MAX_SEGMENT_FRAMES=16`、
  `MAX_FRAME_OWNERS=192` 三处既有阈值**一字未改**。
- **实施中暴露并修掉一个真实缺陷（登记，不隐藏）**：首版把向量内联进 `FrameRegistry` 后仍
  沿用"构造后按值放入 `Option<FrameRegistry>`"的安装路径，注册表膨胀到 ~27 KiB，boot 早期
  溢出 256 KiB 启动栈 → **三重故障**（QEMU 停在 `serial_ready`、退出码 0 = reset）。
  诊断：`MAX_IMAGE_FRAMES=1024`（8 KiB 向量）可启动、512 KiB 启动栈亦可启动 → 确认是栈溢出。
  处置为**就地构造**（`empty()` + `init_at` + 直接写在 `static mut REGISTRY`），
  **未**调大启动栈——不靠放宽约束迁就实现。
- **QEMU 改动面恰为 4 条事件**（镜像增大 4 帧的连带），其中 `frames_ready.segment=19`/
  `table=26` **不变**，证明取帧路径的次数与顺序未被扰动。
- **不勾选任何交付项与退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不是组织级第三方审查，不构成 `REFINEMENT-PROOF`。
- **实施过程如实登记**：本切片的首次代码改动由一名本应只读的侦察 agent 越权产生，且其在收到
  停止指令后又把 `frames.rs` 回滚，留下与 `bootstrap.rs` 不匹配的半成品。team-lead 保存现场
  （补丁留存于会话记录）后**重写 `frames.rs` 全部改动**、复核 `bootstrap.rs` 的每个 hunk、
  并完成诊断、修复与全部验证。上述首版的失败（栈溢出）作为**反例保留**在本报告 §2.1，
  不因代码被重写而抹去。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `frames.rs` 字段 | 删 `image_start`/`image_end`/`image_frames`，增 `image_frame_ids: [u64; MAX_IMAGE_FRAMES=2048]` + `image_count` |
| `frames.rs` 构造 | 新增 `empty()`（`const`）与 `init_at(&mut self, ...)`（校验先于写入）；`install()` 就地构造；`new()` 保留为按值包装（host 单测用）；`INSTALLED` 单独表达安装状态 |
| `frames.rs` 查询 | `owner_of` 改向量 `contains`；`check_disjoint` 增**向量自身无重复**判定；守恒式与 `witness` 口径不变 |
| `frames.rs` 新 API | `image_frame_count()` / `image_frame_at(index)`（含模块级包装，供 `bootstrap` 委托） |
| `mem.rs` | 新增 `FreeFrameAllocator::empty()`（`const`），`from_regions` 复用它 |
| `bootstrap.rs` | `SegmentFrames::Contiguous` 删除、新增 `Image { count }`；`frame_at` 对 `Image` 委托注册表；`memseg_map` 容量前置与 `platform_append_frame` 分支改名；`create_state` 记 `Image` |

### 2.1 反例保留：按值构造导致的启动栈溢出

首版形态（**已废弃，登记为失败经验**）：

```rust
static mut REGISTRY: Option<FrameRegistry> = None;
pub fn install(...) {
    let registry = FrameRegistry::new(inventory, alloc)?;   // ~27 KiB 临时量
    ...
    *slot = Some(registry);                                  // 又一次按值搬移
}
```

实测：`cargo xtask test` 中 `qemu bios run=1` 失败——事件流只有 `seq=1 boot_start`/`seq=2
serial_ready`，**无 `seq=3 memory_ready`**，QEMU 退出码 0（`-no-reboot` 下三重故障 = reset；
正常成功为 33、`panic_event` 为 35）。定位实验：

| 实验 | 结果 | 结论 |
|---|---|---|
| `MAX_IMAGE_FRAMES=2048` + 256 KiB 启动栈 | 三重故障 | — |
| `MAX_IMAGE_FRAMES=1024` + 256 KiB 启动栈 | 六条 PASS | 尺寸相关 |
| `MAX_IMAGE_FRAMES=2048` + 512 KiB 启动栈 | run=1 PASS | **确认启动栈溢出** |

修法不是加栈，而是消除按值临时量（§1.3 的就地构造）。改后 2048 + 256 KiB 启动栈六条 PASS。

## 3. 实测证据（2026-09-23）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30693;image=782;free=30693;regions=3;kstart=0x1000000;kend=0x130e000
PRIMA|v=1|seq=9|event=bootstrap_ready|status=ok|data=task=1;caps=3;…;image_base=0x10000000000;image_len=3202656;image_frames=782;free_frames=30693;…
PRIMA|v=1|seq=25|event=preempt_ready|status=ok|data=ticks=4;pre1=2;pre2=2;root1=0xc000;root2=0x15000;cr31=0xc000;cr32=0x15000;rsp1=0x10000083698;rsp2=0x10000087698;kstack1_top=0x100000836c0;kstack2_top=0x100000876c0;data1=0x1e000;data2=0x22000;mark1=0x1;mark2=0x2
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31475;free=30648;segment=19;table=26;image=782;conserved=1;disjoint=1
```

守恒：`30648 + 19 + 26 + 782 = 31475 = universe` ✅；`conserved=1`、`disjoint=1`。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **283**、xtask 149 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，30 条事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。
- 事件流逐条 diff（HEAD `70b080f` vs 本切片）：**恰好 4 条**变化，见 §4。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件 | 改前（HEAD） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30698;image=778;free=30698;kend=0x130a000` | `usable=30693;image=782;free=30693;kend=0x130e000` | 镜像 +16 KiB 静态数组，增大 4 帧；Usable 相应少 5 帧 |
| `bootstrap_ready` | `image_len=3183464;image_frames=778;free_frames=30698` | `image_len=3202656;image_frames=782;free_frames=30693` | 同上 |
| `preempt_ready` | `rsp1=0x1000007f328;kstack1_top=0x1000007f350`（`rsp2`/`kstack2_top` 同理） | `rsp1=0x10000083698;kstack1_top=0x100000836c0`（`rsp2`/`kstack2_top` 同理） | 镜像增大后内核栈整体后移；两任务内核栈仍各自独立 |
| `frames_ready` | `universe=31476;free=30653;segment=19;table=26;image=778` | `universe=31475;free=30648;segment=19;table=26;image=782` | 守恒仍成立；**`segment`/`table` 不变** |
| 其余 26 条 | — | **逐字段不变** | 无 seq 顺移 |
| `TASK_READY_LIVE_CAPS` | 29 | **29**（不变） | 本切片不新增 Capability |

以上按维护规则 5 记录，**不视为回归、也不隐藏**。

**顺带更正**：跟踪表与旧报告中 `physical_map_ready.offset=0xffff800000000000` 是过时值；
HEAD 与本切片实测均为 `0x28000000000`（该断言本就是关系式 `va ≥ offset`）。与本切片无关。

## 5. host 单测

见设计文档 §3（新增 5 条）。核心是本切片的**判决性反例**：
`image_vector_self_overlap_is_detected` 把向量第 1 项改写成第 0 项，`check_disjoint` 报
`Overlap` 而**守恒式仍成立**——区间表达下这个反例根本不可表达，逐帧向量才使"不交性可独立
判定"成为可能（这正是 `ISSUE-FRAME-001` 的解除条件所指）。

## 6. 边界与反例保留

- **§2.1 的启动栈溢出反例保留**：它说明"把 16 KiB 数组内联进按值搬移的结构"在本项目会
  直接炸掉 boot；后续切片（尤其切片 X 的页表结构）再遇到同类结构膨胀时，应先查就地构造。
- `SegmentFrames::Image::frame_at` 依赖全局注册表已安装（未安装返回 `None`）——登记为隐含
  前置，不是缺陷。
- `MAX_IMAGE_FRAMES=2048`（内核镜像 8 MiB）；超出时启动显式失败，不静默截断。
- 本切片**不**触碰 §14 步骤 4 的 image vspace 映射——那是切片 X 的范围。
- `ISSUE-FRAME-001` 的**闭合判断交复核者**：逐帧向量与逐帧归属判定已落地，但"image 是否也
  必须走逐帧 owner 条目"（本切片让向量本身充当归属表）属解读分歧，不自行宣告闭合。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`；既有 `cargo xtask test` 的 30s/无网络/
  精确序列语义不变。

## 7. 下一步

1. **切片 X（SPEC-069 / `ISSUE-AUTH-001`）**：init 真正运行在自建页表上并逐页映射 image
   （用户已选"完整自建 CR3 + 登记分叉"），含 §14 `{R,X}` 与可写内核 BSS 的冲突登记。
2. 切片 Y（SPEC-070 / `ISSUE-TASK-001`）：`TaskStatus::Faulted` 状态机 + §12.5 迁移入口 +
   真实 QEMU 见证。
3. 三节的范畴外复核（组织级门槛）。
