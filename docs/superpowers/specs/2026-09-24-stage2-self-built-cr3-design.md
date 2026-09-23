# 切片 X 设计：自建内核 CR3 与镜像逐页映射（ISSUE-AUTH-001）

> 文档类型：真实内核实现设计（阶段 2 收尾，第二节）
>
> 日期：2026-09-24
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-23/24，范围项逐项确认）

## 0. 定位与前置

本切片是「闭合阶段 2 剩余登记项」的第二节，承接已交付的切片 W（SPEC-068）。跟踪表
`ISSUE-AUTH-001` 原文大意：

> 规范 §14 步骤 4 的「每 image 页建 vspace 映射」未物化到真实 CR3（内核仍运行 bootloader
> 页表）；本切片只建对象与归属。解除条件：**让 init 运行在自建页表上并逐页映射 image**，
> 或**显式修订 §20.3 细化对应**。状态：Open（细化边界，非缺陷）。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| §14 步骤 4（`COSIR 原语规范.md:1605`） | 对**每个 image 页 `i`** 建 `vspace[image_virtual_base + i] = {segment=image_segment, offset=i, access={R,X}}` | image 页在**模型与真实 CR3 两侧都未物化**；内核运行在 bootloader 页表上 |
| §20.5（`:2296`） | 「仅确认外部根页帧归属，**不证明子页表物理所有权**……边界完成前证据只保持 `REFINEMENT-DESIGN` + `MODEL-TEST`」 | 每任务根由 `clone_root` 复制 bootloader 根全部 512 项（**共享其上级表**），子页表无归属 |
| §20.1（`:2216`） | `MachineState` 含 `cr3: Cr3 // 根页表物理地址` | 运行中的 CR3 非本内核构造 |

§20.3 只是 §14→x86_64 的投影对应表，**无任何"逐页映射可省略"的豁免行**；§20.5 明文把
"未落真实 CR3"登记为细化边界。故跟踪表给的两条出路是仅有的两条。

### 0.1 已确认的关键决策（用户逐项确认，2026-09-23/24）

1. **范围**：只做**自建 CR3**（自有 PML4 + 全部中间页表 + 镜像逐页映射），
   **不**把 §14「image `{R,X}`」与内核可写数据（`.data`/`.bss`）的冲突强行解掉。
2. **冲突登记为未解**：§14 的前置 3 要求 `entry` 落在 `image_virtual_base` 区间内，而内核入口
   就在 `kernel_image_offset`——即 §14 的 `{R,X}` 只能落在那段含可写静态的 VA 上，**同一段 VA
   不能既 `{R,X}` 又可写**，别名映射亦被前置 3 排除。要解此冲突须**在段一级切分镜像**
   （只读 text/rodata 归 §14 的 image Segment，可写 data/bss 归新增的可写内核段），而本仓库
   **没有链接脚本**（`kernel/` 下无 `.ld`，`.cargo/config.toml` 无 rustflags，用系统 default script），
   `boot_info` 也不暴露段表——故该切分**不在本切片**，作为一条登记在案的未解冲突留给后续 SPEC。
3. **切片划分**：X 与 W/Y 各为独立 SPEC、各自报告。

### 0.2 布局事实（实测，`cargo xtask run` + 布局探针）

| 项 | 实测值 |
|---|---|
| 内核镜像 VA 基址 `kernel_image_offset` | `0x10000000000` → **PML4 槽 2** |
| 物理直映射 `physical_memory_offset` | `0x28000000000` → **PML4 槽 5**，`phys_to_virt(p) = offset + p` |
| 启动栈 | VA `0x18000001000..0x18000041000`（256 KiB）→ **PML4 槽 48**，**独立映射、不在直映射区** |
| 可用 RAM 上界 | `0x7fe0000`（≈128 MiB，32736 帧） |
| 内核 ELF | **PIE**；4 个 LOAD 段，`vaddr = file_offset + k*4096`，**各段 k 不同** |

**最后一项是本切片最关键的事实**：镜像不可能用"整段 VA→物理单一常量偏移"复刻——实测
统一偏移会在段边界之后读到错页（见 §2.3 反例）。镜像页的物理帧**必须逐页取得**。

## 1. 设计

### 1.1 自建根（`vspace::build_self_root`）

新增 `SelfRootSpec { direct, image_va, image_pages, stack_va, stack_pages }` 与
`build_self_root(map, alloc, spec, image_phys, stack_phys)`：

- 自取一个根帧并清零，然后逐页建立三段映射：
  - **镜像**：`image_phys(index)` 给出第 `index` 页的物理帧（由调用方逐页解析源根得到）；
    源根无该页映射时**跳过**——与源根保持同一"哪些 VA 有效"集合。权限 supervisor 可写
    （W^X 未做，见 §5）。
  - **物理直映射**：恒等区间 `[offset, offset + max_phys) → [0, max_phys)`。
  - **启动栈**：`stack_phys(index)` 逐页给出；缺失即 `BadRoot`（不留半张根）。
- 页表帧一律经 `alloc` 取得——内核路径传 `frames::allocate_table_frame`，故**每个中间表帧都有
  保留页表帧归属**，可被 W6 守恒与帧注册表审计（补上 §20.5 登记的那一环）。
- 任一步失败即归还**本次**已分配的全部表帧（`MAX_SELF_ROOT_TABLES = 256` 回滚表），不泄漏归属。

### 1.2 大页感知的页表解析（`vspace::leaf_phys_or_none`）

`cpu::translate` 只认 4 KiB 叶（遇 `PS` 直接 `PageNotMapped`），而 **bootloader 根对镜像/直映射/启动栈
用的是大页**。新增解析器支持 1 GiB（PDPT）与 2 MiB（PD）大页，页内偏移取自 VA 低位，
供两处使用：启动栈逐页物理帧、镜像逐页物理帧。

### 1.3 任务根改从自建根克隆

`user::build_kernel_root(...)` 在 boot 早期构造**一次**自建内核根并登记；`build_task_root`
改为 `clone_root(自建根 → 任务根)` 并清空用户槽。任务根仍是独立帧、用户槽互不可见，
但**内核高半区（镜像/直映射/启动栈）全部是本内核自有页表**，不再是 bootloader 页表的共享上级。

调用时机在 `main.rs`：**E/F 见证与 `cow::prepare` 之后、`user::prepare` 之前**——既保证前述
见证仍绑定 bootloader 根（否则会平移它们的帧序），又让随后的任务根全部源自自建根。
其后 `cow::rebase_root(自建根)` 使 `SYS_TASK_CREATE` 也从自建根克隆地址空间。

### 1.4 判决性见证（新增独立事件 `self_root_ready`）

```rust
pub struct SelfRootWitness {
    pub root: u64,         // 当前 CR3（运行中的地址空间根）
    pub kernel_root: u64,  // 登记的自建内核根帧
    pub image_table: u64,  // 当前根 PML4[2] 指向的中间表帧
    pub owned: bool,       // 该表帧在帧注册表中的归属是否为「保留页表帧」
    pub pages: u64,        // 当前根下已映射的镜像 VA 页数
    pub translated: u64,   // 当前根解析 kernel_image_offset 得到的物理地址
}
```

在 `resume::after_resume`（内核已运行于任务根=自建根的克隆之后）采集。xtask 侧断言：

- `owned == 1`——**运行中的地址空间通过本内核自有、且在 W6 下登记为保留页表帧的表映射内核镜像**；
- `translated == page_table_ready.translated`——跨事件不变式：运行中的根把 `kernel_image_offset`
  解析回内核镜像物理基址；
- `pages > 0`；三帧均非零且 4 KiB 对齐。

事件落在 `frames_ready` 之后，故只顺移 `boot_complete`（30→31），其余 seq 不动。

## 2. 实施中暴露并修掉的既有缺陷（反例保留）

### 2.1 `count_missing` 每个新建路径多分配一张页表帧（泄漏）

`vspace::count_missing` 原返回 `4 - k`，并注释为"3 个中间层 + 1 个叶 PT"。实际上 `k` 级缺失时
需新建的正是 PDPT/PD/PT 中自 `k` 起的那 `3 - k` 张（叶 PT 已含在内）。多出的那一帧被分配后
从不使用、也从不归还——**每个新建路径泄漏一帧**。既有映射规模小，泄漏 2–3 帧不可见；
本切片逐 4 KiB 覆盖 128 MiB 直映射（64 张 PT）时泄漏 60+ 帧并撑爆 `MAX_TABLE_FRAMES`。

修正为 `3 - k`，并加 host 单测 `map_4k_fresh_path_allocates_exactly_the_tables_it_uses`
（全新路径恰分配 3 张；同路径第二条不新增）。**该修正平移了此后全部帧分配顺序**，见 §4。

### 2.2 boot 栈溢出（首版实现）

首版把自建根的中间表**内联进一个按值搬移的结构**，boot 早期溢出 256 KiB 启动栈 → 三重故障。
处置为**就地构造 / 不引入大临时量**，并**不调大启动栈**——不靠放宽既有约束迁就实现。
（同类反例已在切片 W 记录；此处登记它在本切片再次出现。）

### 2.3 镜像"单一常量偏移"假设不成立（本切片最关键的定位）

首版把镜像按 `phys = kernel_phys_start + (va - kernel_image_offset)` 复刻。boot 停在
`mov cr3` 之后的指令取指：QEMU `-d int` 显示 `CR3` 已成功写入自建根，随后在
`cpu::write_cr3` 的 `pop rax`（ELF 偏移 `0x5e5a8`）处 #GP → #DF → 三重故障。

逐页对账（源根 vs 自建根，同一 VA）给出决定性证据：

```
vboot=0x105d000  byte_boot=0x58   ← bootloader 根：正确（pop rax）
vself=0x105e000  byte_self=0xcc   ← 自建根：错一页
```

根因：内核 ELF 是 PIE，bootloader 按程序头**逐段**装载，各段 `vaddr = file_offset + k*4096`
且 **k 逐段不同**（实测第 2 段起差 1 页），故统一偏移在第 2 段之后即失配。
修正为**逐页向源根解析物理帧**（与启动栈同一做法）。这是一条必须登记的实现事实：
镜像的 VA↔物理关系不是区间恒等，而是 ELF 段布局。

### 2.4 调试方法记录（供后续切片复用）

- QEMU `-d int,cpu_reset -D <log>` 给出异常向量/错误码/RIP/CR，是定位三重故障的首选；
  `-d in_asm` 可把故障 RIP 处的**实际内存字节**取出与 ELF 反汇编对账。
- 串口用 `out 0x3f8` **非阻塞**写入时，reset 会丢掉 FIFO 中的末尾字符，使"最后一条 trace"
  不可信；须先轮询 LSR(0x3fd) bit5 再写，且仍以异常日志为准。

## 3. host 单测（新增 5 条，kernel 283→288）

`vspace.rs`：镜像逐页物理帧不被统一偏移替代（含"第二页用它自己的帧"断言）、源根缺失的镜像页
被跳过、启动栈页缺失确定性失败（`BadRoot`）、`leaf_phys_or_none` 解析 2 MiB 大页（含页内偏移）、
`map_4k` 新建路径恰好分配 3 张表。

`xtask/src/protocol.rs`：4 条反例——`owned != 1`、`translated` 与
`page_table_ready.translated` 不符、`pages == 0`、根帧非 4 KiB 对齐（153 条全绿）。

## 4. 回归面（逐条登记，实测自事件流逐条 diff）

| 事件 | 改前（SPEC-068） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30693;image=782;free=30693;kend=0x130e000` | `usable=30671;image=804;free=30671;kend=0x1324000` | 自建根与见证代码使镜像 +22 帧；可用帧相应减少 |
| `bootstrap_ready` | `image_len=3202664;image_frames=782;free_frames=30693` | `image_len=3289112;image_frames=804;free_frames=30671` | 同上 |
| `cow_ready` | `shared=0x6000;private=0xb000` | `shared=0x5000;private=0x9000` | **`count_missing` 泄漏修正**使此前少耗 2 帧，帧序整体前移 |
| `preempt_ready` | `root1=0xc000;root2=0x15000;data1=0x1e000;data2=0x22000` | `root1=0x53000;root2=0x5b000;data1=0x63000;data2=0x66000` | 两任务根改为克隆**自建根**（且此前已多分配 74 张表帧）；`rsp*`/`kstack*_top` 随镜像增长后移 |
| `frames_ready` | `free=30648;segment=19;table=26;image=782` | `free=30561;segment=19;table=91;image=804` | 自建根 74 张自有表帧 + 镜像 +22 帧；**`segment=19` 不变** |
| **新增** `self_root_ready` | — | `root=0x5b000;kernel_root=0xa000;image_table=0xb000;owned=1;pages=165;translated=0x1000000` | 本切片判决性见证（新增独立事件） |
| `boot_complete` | seq=30 | seq=**31** | 上述新事件插在 `frames_ready` 之后，仅顺移本事件 |
| 其余 24 条 | — | **逐字段不变** | 含 `page_table_ready.translated`、`vspace_ready`、`platform_segments_ready`、`authority_ready` |

`table` 由 26 升至 **91**（`MAX_TABLE_FRAMES = 128`，余量 37）——逐 4 KiB 覆盖 128 MiB 直映射
约需 66 张，其余为镜像/启动栈链。**余量收紧是登记在案的事实**；后续若逼近上限，按项目纪律
须扩展模型（如大页映射）而非放宽阈值。

## 5. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」（组织级门槛，与阶段 0/1 一致）。
- **§14 `{R,X}` 与内核可写数据的冲突未解**（§0.1 第 2 条）：本切片把 image 页映射为
  supervisor **可写**（W^X 未做）。这**不满足** §14 步骤 4 的权限要求，作为未解冲突登记。
- **§14 的 `image` 与实现的 init 可执行镜像仍分叉**：抽象 §14 的 `image` 是 init 的可执行镜像，
  实现里 `image` = **内核镜像**，而 init 实跑在 ring3 用户段（`user.rs` 的 `USER_CODE_VA`）。
  本切片把 §14 步骤 4 物化到**内核镜像**的 VA 上（前置 3 用 `kernel_image_offset` 满足），
  该语义分叉登记在案，不自行宣告闭合。
- 镜像页的物理帧**逐页取自 bootloader 根**：本内核不解析 ELF 程序头，而是复刻 bootloader 已建立的
  VA↔物理关系。这是"自建页表"而非"自建装载"——登记为方法边界。
- 直映射按**可用 RAM 上界**（`0x7fe0000`）建立，未映射 MMIO/帧缓冲；本内核不访问它们。
- 用户槽仍不建立镜像映射；任务根克隆自建根后再加载各自用户页（同 VA 不同帧的隔离不变）。
- `ISSUE-AUTH-001` 的**闭合判断交复核者**：真实 CR3 侧已自建且镜像页逐页物化，但上述两条
  规范偏差未消，是否算满足解除条件由复核者裁定。
