# Prima OS 实现阶段

> 文档类型：实施进度与证据跟踪
>
> 更新日期：2026-08-18
>
> 架构与阶段定义：[[Prima OS]]
>
> 精确语义规范：[[COSIR 原语规范]]

## 维护规则

本文只记录实际进度、退出条件证据和阻塞项，不重新定义架构或原语语义。

状态只允许：

- **未开始**：尚无可验收交付物；
- **进行中**：已有工作产物，但尚未满足全部退出条件；
- **阻塞**：存在阻止继续推进的明确问题；
- **已满足退出条件**：全部退出条件均有可复查证据。

维护约束：

1. “已设计”或“已定义”不等于“已实现”；
2. 检查项只有在证据列提供可访问工件后才能勾选；
3. 测试证据记录命令、日期和结果摘要；
4. 证明证据记录工具、入口文件和定理或检查名称；
5. 实验失败和反例必须保留，不得通过修改措辞隐藏；
6. 阶段定义变化先修改 [[Prima OS]]，语义变化先修改 [[COSIR 原语规范]]，再同步本文。

## 当前快照

| 字段 | 当前值 |
|---|---|
| 当前阶段 | 阶段 0 — 语义基线 |
| 当前状态 | 进行中 |
| 已满足退出条件的最高阶段 | 无 |
| 当前实现代码 | 阶段 1 启动闭环探索实现：`kernel/`、`xtask/` |
| 当前主要工件 | [[Prima OS]]、[[COSIR 原语规范]]、Rust/QEMU 启动闭环 |
| 当前首要缺口 | S1-D5 Bootstrap 快照、固定 Task 生命周期与 Capability/消息生命周期审计基础已接入；后续缺口为完整 IPC 消息原子提交/阻塞恢复、S1-X2 初始 Authority 细化、S1-X3 活动 Segment/页帧全局归属，以及 S0-X1/S0-X2 |
| 最近规范修订 | 2026-07-31 — 规范审查修复：ObjectRef 统一、ExternalInterrupt 语义明确、CapMoved 审计事件补充、Node 对象完整定义、DMA 帧/页帧信息流说明、POSIR/COSIR 边界清单、ns_mount 跨节点约束、task_create 增加 kind 参数、task_yield 调度时机明确、应用域/LibOS 关系精确化、细化要求从 21 项扩展到 25 项 |
| exc_resume（本切片） | 2026-08-07 — 定义异常恢复原语 `exc_resume`、x86_64 SretFrame 帧布局（细化层定义）、Faulted 细化态与错误码；新增 §12.10，标注细化要求 #16 与边界清单 #1/#2 |
| 最近实现证据 | 2026-08-06 — 固定 30 秒失败保护后 `cargo xtask test` 在 QEMU 11.0.50 上连续完成两次 BIOS 启动；整条验收耗时 17.156 秒，23 个 host 单元测试全部通过 |
| 最近细化证据 | TEST-005：2026-08-13 在验收 HEAD `1f682db600947f49975eca91e045cae222bb8089` 上，Rust `MODEL-TEST` 100/100 与 workspace 123/123 通过，并获独立 verification PASS；仅为 host 模型、regression/lint 与既有 QEMU 启动闭环无回归，证据边界见 TEST-005。 |
| 最近审计验收 | TEST-006：2026-08-16，cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29)；`cargo test -p cosir` 180/180、workspace 203/203（cosir 180 + xtask 23）、clippy 零告警，`cargo xtask test` 六条 PASS（QEMU BIOS run=1/2、run=2/2 均通过）。仅支持 host `MODEL-TEST`、lint 与既有启动闭环无回归；不支持 `KERNEL-TRACE`、`REFINEMENT-PROOF`、`COMPAT-TEST`、真实硬件 TLB 或 BLK-001 关闭。 |
| 物理内存发现（本切片） | 2026-08-17 — 真实内核 `mem::build_inventory` 帧清单 + `memory_ready` 协议上报 + xtask 结构不变式断言；证据 SPEC-017 |
| 页表/GDT/IDT/trap（本切片） | 2026-08-17 — 真实内核物理直映射 + CR3 页表 page-walk translate + GDT/TSS 装载 + IDT 最小 trap 闭环（`extern "x86-interrupt"`）；证据 SPEC-018 |
| Lean 形式化（本切片） | 2026-08-18 — Lean 4 + lake 可执行状态模型（`Formal.State`/`Bootstrap`/`ExcResume`）与 cosir host 对拍；`MODEL-TEST`；证据 SPEC-019；关闭 BLK-001、支持 S0-X3；tlb/memseg/cow 的 Lean 建模登记为后续切片待办 |
| Bootstrap/帧所有权（本切片） | 2026-08-25 — 真实内核创建固定 Bootstrap 状态表、Capability 位置表，并以 `Usable` 区间分段帧分配器支持分配、归还和再分配；补充 Capability 派生的父子关系与权限单调性基础；串口 `bootstrap_ready` 事件由 xtask 校验；证据 SPEC-020；仍不宣称完整动态 Capability 派生、用户态执行或完整 S1-X2/S1-X3 |
| 动态 Capability 位置（本切片） | 2026-08-25 — `BootstrapState::derive_capability` 将派生记录与位置注册绑定；固定容量表支持 holder 校验的移动、撤销、唯一活动位置检查和容量拒绝；证据 SPEC-021；仍不宣称 IPC、任务生命周期联动、完整全局 Authority 不变量或用户态切换 |
| BLK-002 细化（本切片） | 2026-08-13 — MemorySegment host 四级索引树与双视图的 `REFINEMENT-DESIGN`；`MODEL-TEST` 检查 canonical 地址拒绝、失败原子性、α mutation 隔离和 allocator partition。`cargo test -p cosir` 69 tests 通过；未覆盖真实 CR3/PTE、非叶权限与页表页物理身份、TLB、多核、真实硬件原子更新窗口或机器证明。 |
| COW 组合（本切片） | 在明确限定的 host 组合场景中，MODEL-TEST 未发现为“共享只读映射 → 写保护 fault → 用户态显式分配相离新 Segment 并换映射 → exc_resume → 显式调度”新增 POSIR 一等原语的需求。该结论不覆盖帧字节复制、真实 trap/PTE/TLB、多核协议、KERNEL-TRACE 或 REFINEMENT-PROOF。 |
| TLB 细化（本切片） | host MODEL-TEST 仅观察到该 per-task、无替换策略缓存模型中的非自动刷新与显式失效行为；不外推为真实硬件 TLB、CR3/PCID、invlpg/INVPCID、多核 shootdown 或内存顺序结论。 |

当前已形成架构白皮书和抽象语义规范，并有阶段 1 启动闭环探索实现、真实内核物理内存发现（S1-D3，SPEC-017）、真实页表/GDT/IDT/trap 框架（S1-D4，SPEC-018）与 Bootstrap 初始 Authority 快照（S1-D5，SPEC-020）；尚无机器证明、完整 POSIR Bootstrap、用户态执行或兼容性测试证据。

## 证据索引

| ID | 类型 | 工件或命令 | 日期 | 结果 | 支持的检查项 |
|---|---|---|---|---|---|
| DOC-001 | 架构文档 | [[Prima OS]] | 2026-07-30 | 已形成决策型架构白皮书；不代表实现完成 | S0-D3 |
| SPEC-001 | 抽象规范 | [[COSIR 原语规范]] | 2026-08-16 | 已定义对象能力状态机（11 类 Bootstrap 后常规机制 + 1 个 Bootstrap 初始化迁移，共 12 项；16 个不变式）；尚无机器证明 | S0-D1、S0-D2 |
| IMPL-001 | 实现工件 | `Cargo.toml`、`kernel/`、`xtask/`、`Cargo.lock` | 2026-08-05 | 固定 nightly-2026-07-30 与 bootloader 0.11.17；生成 BIOS/UEFI 镜像 | S1-D1、S1-D2 |
| TEST-001 | QEMU 集成测试 | `cargo xtask test` | 2026-08-05 | QEMU 11.0.50；固定 BIOS profile 连续启动两次，结构化串口序列和退出码均通过 | S1-D1、S1-D2、S1-X1 |
| TEST-002 | 最终审查加固验收 | `cargo test -p xtask`、`cargo xtask test` | 2026-08-06 | 23 个 host 单元测试通过；严格 LF/CRLF、reader 启动失败 cleanup 与 UART TEMT drain 加固后，QEMU 11.0.50 固定 BIOS profile 连续启动两次并输出六条稳定 PASS 摘要 | S1-D1、S1-D2、S1-X1 |
| TEST-003 | 高负载失败保护验收 | `cargo test -p xtask`、`cargo xtask test` | 2026-08-06 | 固定 30 秒失败保护、不增加自动重试；23 个 host 单元测试通过，QEMU 11.0.50 两次 BIOS run 输出六条 PASS，整条验收耗时 17.156 秒 | S1-D1、S1-D2、S1-X1 |
| SPEC-012 | MODEL-TEST + 源码审查 | `COSIR 原语规范.md` §12.9/§12.10、`cosir/src/main.rs`、`cosir/src/state.rs`、`cosir/src/ops.rs`；证据：TEST-005 | 2026-08-13 | `MODEL-TEST` 检查 `exc_resume` Capability/订阅授权、`0xA0` little-endian 帧、RIP/RSP/CS/SS/RFLAGS、pending/source/kind、不可恢复异常、最终状态、精确已消费 region、失败完整 State 原子性、二次消费与 Audit；不直接观察内部 mutation 时序。模型范围边界：`raise_exception` 仅建模同步侧（Faulted + `pending_exc` + 现场帧）；规范 §12.9 的"订阅者经 `enqueue_or_deliver` 收到通知"异步投递未建模（`subscriptions` 存在但异常路径不消费），与 Send/Recv 消息机制未建模一致。源码审查确认实现顺序为 frame bytes `fill(0)` → pending remove → kernel frame → Audit → Ready；TEST-005 的独立 verification 为 PASS（无 Critical/Important）。不代表真实 trap/`iretq`、通知投递、`KERNEL-TRACE` 或 `REFINEMENT-PROOF` | S0-D1、S0-D2 |
| SPEC-013 | 细化规范与可执行模型 | `COSIR 原语规范.md` §20、`cosir/src/machine.rs`、`cosir/src/abstract_ops.rs` | 2026-08-07 | 定义 x86_64 MachineState、抽象函数 α 与 Bootstrap 细化对应；`cargo test -p cosir` 17 tests 通过；`cargo xtask test` 六条 PASS | S0-D4 |
| SPEC-014 | MODEL-TEST + REFINEMENT-DESIGN | COSIR 原语规范 §20.5、cosir/src/pagewalk.rs、cosir/src/memseg_refine.rs | 2026-08-12 | host 四级索引树与 vspace 双视图设计；`MODEL-TEST` 检查 W6/W7，W8 仅有 unmap entry-page guard；未覆盖非叶权限、页表页物理身份、TLB、多核、真实 CR3/PTE 与原子更新窗口；`cargo test -p cosir` 42 tests 通过 | S0-D4 |
| SPEC-015 | MODEL-TEST + REFINEMENT-DESIGN + 反例 | COSIR 原语规范 §12.8、cosir/src/cow_refine.rs、cosir/src/cow_refine_tests.rs、cosir/src/main.rs COW 冒烟 | 2026-08-14 | 在明确限定的 host 组合场景中，MODEL-TEST 未发现为“共享只读映射 → 写保护 fault → 用户态显式分配相离新 Segment 并换映射 → exc_resume → 显式调度”新增 POSIR 一等原语的需求。该结论不覆盖帧字节复制、真实 trap/PTE/TLB、多核协议、KERNEL-TRACE 或 REFINEMENT-PROOF。宿主模型不模拟内存内容字节，`memseg_create` 仅为副本分配同卡片数私写新物理帧（`parent_frames.len()==copy_frames.len()` 且物理帧相离），"内容复制"超出本 host 抽象层。反例 R1（并发写共享只读页各 fault，需用户态协调）与 R4（不换映射直接恢复则页仍只读、内核式自动复制不可表达）边界如实记录——R4 为对 §12.8“用户态 COW”的边界澄清、非 H1 反例。总验收 2026-08-14：`cargo test --workspace` cosir 117/117 + xtask 23/23（140/140）、clippy `-D warnings` 零告警、`cargo xtask test` qemu bios run=1/2 六条 PASS 退出 0；`cargo run -p cosir` 冒烟新增 5 条 PASS；不宣称 `KERNEL-TRACE`/`REFINEMENT-PROOF` | S0-D4 |
| SPEC-016 | MODEL-TEST + 源码审查 | COSIR 原语规范 §8、cosir/src/tlb_refine.rs、cosir/src/tlb_refine_tests.rs、cosir/src/main.rs TLB 冒烟 | 2026-08-15 | host 模型 per-task TLB，非一致刷新 + 显式 `invalidate_va`/`flush_all`，不模拟替换策略；地址必须 canonical 页对齐（`validate_page_vaddr`）。`MODEL-TEST` 检查写前显式失效的后读一致、逐地址 `has_tlb` 是否存在与失效作用域、`NoSuchTask`/无效地址错误面。源码审查两个 Minor（不放大证据，随终审复核）：`access_allowed` 读路径以 `access != 0`（而非字面 R 位）判定、`translate` 经 `pf.tasks` 而 `invalidate_va`/`flush_all` 经 `per_task` 检查存在性。总验收 2026-08-15(cargo 1.99.0-nightly 3efb1f477、rustc 1.99.0-nightly 1a833e165)：`cargo test -p cosir` 124/124（117 既有 + 7 新 TLB）、`cargo run -p cosir` 冒烟新增 `PASS model-test tlb non-coherent refresh + explicit invalidation` 退出 0、clippy `--workspace --all-targets --no-deps -- -D warnings` 零告警、`cargo test --workspace` cosir + xtask 全绿、`cargo xtask test` qemu bios run=1/2 六条 PASS 退出 0。证据仅为 `MODEL-TEST`；不宣称 `KERNEL-TRACE`/`REFINEMENT-PROOF`、真实硬件 TLB，不含替换策略、多核 shootdown 或 CR3 切换自动失效 | S0-D4 |
| SPEC-017 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/mem.rs`（`build_inventory`/`MemError`/`FrameInventory`）、`kernel/src/main.rs`、`kernel/src/protocol.rs`、`xtask/src/protocol.rs` | 2026-08-17 | 真实 bootloader 交接处解析 `BootInfo.memory_regions` 构建 4KB 帧清单：只统计 `Usable` 区（内核可自由使用帧）、报告内核镜像帧（`image_frames`）；bootloader_api 把内核镜像所在区标为 `Bootloader`（已从 `Usable` 排除），故 `free_frames = usable_frames`，强制不变量为镜像与所有 Usable 帧不相交（`image ∩ usable = ∅`，W6 帧级）且 `image_frames > 0`。host 单测覆盖对齐截断/非 Usable 忽略/镜像不扣减（`free == usable`）/镜像对齐取整/`EmptyImage`/`ImageOverlapsUsable`/重叠区报错/多区求和/镜像落 Bootloader 区不占 usable；QEMU boot-trace 断言 `memory_ready` 数据满足 `free>0 ∧ image>0 ∧ free==usable ∧ kstart/kend 4KB 对齐 ∧ kstart<kend`。总验收 2026-08-17(cargo 1.99.0-nightly 3efb1f477、rustc 1.99.0-nightly 1a833e165)：`cargo test -p kernel` 10/10、`cargo test -p xtask` 29/29、`cargo test -p cosir` 180/180、`cargo test --workspace` 219/219、clippy 零告警、`cargo xtask test` qemu bios run=1/2 六条 PASS 退出 0。不宣称帧分配器（§18.0 #12）、真实页表（S1-D4）、物理内存读写（默认配置无 `physical_memory_offset`）、COW 端到端 `KERNEL-TRACE` 或 `REFINEMENT-PROOF` | S1-D3 |
| SPEC-018 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/cpu.rs`（`cr3`/`translate`/`PageTableEntry`）、`kernel/src/gdt.rs`（`Gdt`/`Tss`/`init_kernel_gdt`）、`kernel/src/idt.rs`（`Idt`/`extern "x86-interrupt"` handler/`trigger_int3`）、`kernel/src/safety.rs`、`kernel/src/config.rs`、`kernel/src/protocol.rs`、`xtask/src/protocol.rs` | 2026-08-17 | 真实 bootloader 交接处启用物理内存直映射（`BootloaderConfig.mappings.physical_memory`，`BootInfo.physical_memory_offset` 回传），沿 CR3 页表 page-walk `translate(kernel_image_offset)` 恢复内核物理基址（QEMU 实测 `page_table_ready.translated == memory_ready.kstart`，两者相等由 xtask 跨事件断言强制）；装载 GDT（`[u8;64]` 按 `index*8` 布局，code selector 0x8 / data 0x10 / TSS 0x18，仅 reload ds/es、保留 bootloader ss）+ 最小 TSS（`#[repr(C,packed)]` 104 字节、rsp0@4、iopb=104）；装载 IDT（16 字节中断门 Entry 含 reserved 字段）并以 `extern "x86-interrupt"` breakpoint handler + `int3` 完成最小 trap 闭环（QEMU `trap_ready seen==3` 证明 trap→handler→iretq 真实发生、恰好一次）。host 单测 pin 各描述符字节编码（`build_gdt_bytes`/`tss_desc_upper_base_bytes` 按 Intel SDM、`PageTableEntry` 位布局、`translate` 多层 4KiB 页、`Idt`/`Entry` 尺寸）。unsafe 收敛到细化模块（`config.rs`/`cpu.rs`/`gdt.rs`/`idt.rs`/`safety.rs`），lib crate 加 `#![deny(unsafe_op_in_unsafe_fn)]`。事件序列从切片 A 的 4 扩为 8：boot_start(1) serial_ready(2) memory_ready(3) physical_map_ready(4) page_table_ready(5) gdt_tss_ready(6) trap_ready(7) boot_complete(8)，前 3 个切片 A 冻结事件未动。总验收 2026-08-17：`cargo test --workspace` 259/259（cosir 180 + kernel 41 + xtask 38）、clippy `-D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` qemu bios run=1/2 六条 PASS 退出 0。不宣称自建页表（沿用 bootloader CR3）、ring3/用户态栈切换、完整异常分派（16 向量+IRQ+PIC）、`KERNEL-TRACE`、`REFINEMENT-PROOF` 或机器证明；TSS upper-base 编码经 host decode-back 测试钉住（Intel SDM 长模式系统段格式） | S1-D4 |
| SPEC-019 | 可执行状态模型（Lean） | `lean/Formal.lean`（入口，import 全覆盖）、`lean/Formal/State.lean`、`lean/Formal/Bootstrap.lean`、`lean/Formal/ExcResume.lean`、`lean/README.md`（映照契约）、`cosir/tests/lean_backtrace.rs` | 2026-08-18 | 选用 Lean 4 + lake 作为形式化模型工件；`Formal.State` 定义 `Object`/`Capability`/`State`/`WellFormed = True`/`initTaskId = 1`，`Formal.Bootstrap` 定义 `alpha`/`emptyState`/`bootedState`/`bootstrap` + 5 条 `by rfl` 定理（幂等唯一性、init 铸造、α 投影保真、WellFormed），`Formal.ExcResume` 定义最小 `excResume` + 4 条 `by rfl` 定理（失败原子性、owner 不匹配、成功返回、成功不吞能力）。证据等级 `MODEL-TEST`：Lean `#eval`/`theorem` 为参考语义，cosir `tests/lean_backtrace.rs` 对拍 bootstrap 幂等性/init task 与 exc_resume 失败原子性，两端一致。`cd lean && lake build` 无错误，三个 olean（`Formal.State`/`Formal.Bootstrap`/`Formal.ExcResume`）全部编译且入口冒烟 `#eval` 输出 `booted` false/true。仅支撑 bootstrap+exc_resume；不宣称 `REFINEMENT-PROOF`、`KERNEL-TRACE`、`COMPAT-TEST`、机器证明或真实硬件；tlb/memseg/cow 的 Lean 建模登记为后续切片待办；S0-X1（独立规格审查）另行推进 | S0-X3、BLK-001 |
| SPEC-020 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/bootstrap.rs`、`kernel/src/mem.rs`、`kernel/src/main.rs`、`kernel/src/protocol.rs`、`xtask/src/protocol.rs` | 2026-08-25 | `BootstrapState` 固定 init Task=1、三条唯一 Capability 记录及其精确权限，并从 `FrameInventory` 建立 image/free 帧归属计数；`CapabilityPositionTable` 支持受 holder 校验的移动/撤销，`CapabilityDerivationTable` 覆盖初始 Capability 的父子关系与权限单调性基础，`FreeFrameAllocator` 按 Usable 区间分段并支持一次性归还/再分配且拒绝镜像重叠；host 单测覆盖权限、ID 唯一性、派生权限收窄、分配穷尽、归还复用和不一致清单拒绝；QEMU 启动路径校验 allocator 剩余帧数与 inventory 一致，`bootstrap_ready` 纳入 9 事件序列。`cargo test --workspace` 270/270、clippy 零告警、QEMU BIOS run=1/2 通过。该证据仍不等同于完整动态 Capability 派生、全局位置不变量或用户态切换，因此不关闭 S1-X2/S1-X3、`KERNEL-TRACE` 或 `REFINEMENT-PROOF` | S1-D5 |
| SPEC-021 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | `BootstrapState::derive_capability` 将 Capability 派生与位置注册绑定；位置表覆盖初始与派生对象，支持 holder 校验的转移、撤销后唯一活动位置检查、重复 ID 拒绝和固定容量拒绝；单测覆盖派生注册、权限收窄、错误不改变已有记录、移动/撤销和 8 条派生容量边界。证据仅为 `MODEL-TEST`，不等同于 IPC/Task 生命周期联动、完整全局 Authority 不变量、`KERNEL-TRACE`、`REFINEMENT-PROOF` 或用户态切换 | S1-X2、S1-X3 |
| SPEC-022 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | `BootstrapState::revoke_capability` 要求当前 holder 授权，并将撤销从初始或派生根 Capability 传播到完整派生子树；所有失效对象同步进入 dropped/inactive 位置，错误 holder 不改变状态。单测覆盖初始根撤销、派生根撤销、跨两级子树传播、stale 移动拒绝与唯一活动位置不变量。证据仅为 `MODEL-TEST`，不等同于 Task/IPC 生命周期联动、对象销毁回收、`KERNEL-TRACE`、`REFINEMENT-PROOF` 或用户态切换 | S1-X2、S1-X3 |
| SPEC-023 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | `BootstrapState::revoke_task_capabilities` 为非 init holder 提供退出/销毁前 Capability 清理入口：收集该 Task 直接持有的活动 Capability，逐根调用授权撤销并传播派生子树；返回直接撤销根数，重复清理为空操作，init Task 受保护。单测覆盖 holder 清理、两级子树回收、重复清理、init 保护和唯一活动位置不变量。证据仅为 `MODEL-TEST`，不等同于 Task 状态表、调度、IPC 队列、Segment/帧回收、`KERNEL-TRACE` 或用户态切换 | S2-D4 |
| SPEC-024 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | 新增固定容量 `TaskTable`：init Task 初始为 Running；新 Task 进入 Ready；单核约束拒绝第二个 Running；支持 schedule/yield；Running Task 不可销毁，销毁前调用 holder Capability 清理并标记 Dead，init Task 不可销毁。单测覆盖状态转换、单 Running、不允许销毁运行中 Task、销毁后的 Capability 清理和 init 保护。证据仅为 `MODEL-TEST`，不等同于真实上下文切换、抢占、优先级调度、IPC 阻塞恢复或用户态执行 | S2-D4 |
| SPEC-025 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | 新增固定容量 FIFO `IpcQueue` 与 `send_message`/`receive_message`：消息按接收者 FIFO 出队；空队列将接收 Task 置为 BlockedRecv；发送成功唤醒接收者；队列满、未知 Task 和空队列均有明确错误；Task 销毁清除相关消息。单测覆盖 FIFO 顺序、阻塞/唤醒、空队列、Task 销毁清理。证据仅为 `MODEL-TEST`，不等同于真实 syscall/中断唤醒、优先级继承、跨核 IPC、权限授权或用户态执行 | S2-D5 |
| SPEC-026 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | `send_message` 在入队前要求发送者持有活动初始或派生 `TaskSend` Capability；未授权发送返回 `NotAuthorized` 且队列保持不变；init 初始 TaskSend 与派生 TaskSend 均可发送，接收者只能读取自己的邮箱。单测覆盖未授权拒绝、队列原子性、派生权限收窄后的合法发送。证据仅为 `MODEL-TEST`，不等同于接收 Capability、端点对象、IPC syscall、跨核通信或用户态执行 | S2-D5 |
| SPEC-027 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | 消息支持最多两条 Capability，发送前验证 holder/活跃/重复 ID，并将位置从 `HeldBy(sender)` 原子迁移到 `InMessage(message_id)`；接收按 FIFO 将其迁移到 `HeldBy(receiver)`；队列移除或 Task 销毁时失效消息内 Capability。单测覆盖中间位置、接收归属、非法 holder 拒绝和队列原子性。证据仅为 `MODEL-TEST`，不等同于完整消息原子提交、直接交付优化、stale token 审计、跨核传输或用户态执行 | S2-D5 |
| SPEC-028 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | 当 receiver 处于 `BlockedRecv` 时，发送绕过 FIFO 活动消息表，直接将消息写入 receiver 的 pending 结果并把消息内 Capability 迁移到 `HeldBy(receiver)`；receiver 被唤醒后下一次 recv 取出该结果。单测覆盖 blocked + 带 Capability 直接交付、队列计数不增加和后续非法持有者发送拒绝。证据仅为 `MODEL-TEST`，不等同于真实 syscall 线性化、调度恢复、审计事件或跨核传输 | S2-D5 |
| SPEC-029 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | 新增固定容量 `AuditLog`；成功排队发送追加 `MessageSent`，成功排队接收追加 `MessageReceived`；直接交付在同一成功操作中追加发送/接收事件；审计容量预检在消息和 Capability 状态变更前拒绝。单测覆盖直接交付事件顺序。证据仅为 `MODEL-TEST`，不等同于完整对象生命周期 Audit、拒绝事件持久化、跨节点审计或机器证明 | S3-D5 |
| SPEC-030 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | Task 销毁前预检审计容量；销毁队列中的消息追加 `MessageDropped`，销毁 pending 直接交付结果同样追加丢弃事件，完成状态转移后追加 `TaskDestroyed`；审计不足时不清理 Capability、消息或 Task。单测覆盖直接交付消息丢弃与 Task 销毁事件。证据仅为 `MODEL-TEST`，不等同于完整 Capability 撤销审计、拒绝事件持久化、跨节点审计或机器证明 | S3-D5 |
| SPEC-031 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-25 | Capability 派生追加 `CapabilityDerived`；显式 Task 间移动追加 `CapabilityMoved`；父/派生根撤销先在副本预演完整子树规模，再原子提交位置失效并追加每个 `CapabilityRevoked`；审计容量不足不会改变派生/撤销状态。单测覆盖移动、两级撤销链和审计事件存在性。证据仅为 `MODEL-TEST`，不等同于 IPC 内部自动移动的完整审计、拒绝事件持久化、跨节点 Capability 历史或机器证明 | S3-D5 |
| SPEC-032 | 真实内核 MODEL-TEST | `kernel/src/mem.rs`、`kernel/src/main.rs` | 2026-08-25 | `FreeFrameAllocator::check_invariants` 校验分配区间对齐、排序、互不重叠、镜像不相交；校验归还帧唯一且不落入当前空闲尾部，并校验 `remaining` 守恒。构造分配器时强制通过该检查，host 单测覆盖镜像重叠、归还复用、空闲尾部污染和重复归还状态；QEMU 启动路径继续校验 allocator 剩余帧数等于 inventory。证据仅为 MODEL-TEST + QEMU 集成断言，未建模活动 Segment/页表页的全局对象归属，因此不关闭 S1-X3 | S1-X3 |
| TEST-004 | 阶段一命令验收 | `cargo fmt --all --check`、`cargo test -p cosir`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo test --workspace`、`cargo xtask test` | 2026-08-13 | 在 `60a61bd0e01d568541c43edf557ac12b341e0a1f`，cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29) 上全部退出码 0：fmt 无差异；cosir 69/69；workspace cosir 69/69 + xtask 23/23（92/92）；clippy 无 warnings；QEMU BIOS run=1/2 输出六条 PASS。直接支持 host `MODEL-TEST`、lint 与既有 QEMU 启动闭环无回归；不支持真实 CR3/PTE 细化、非叶权限/页表页物理身份、TLB、多核、真实硬件原子更新窗口、机器证明或阶段一退出条件已满足。 | S0-D4、S1-D1、S1-D2、S1-X1 |
| TEST-005 | 阶段二 exc_resume 独立验收 | `cargo fmt --all --check`、`cargo test -p cosir`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo test --workspace`、`cargo xtask test`、独立 verification | 2026-08-13 | 验收 HEAD `1f682db600947f49975eca91e045cae222bb8089`；cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29)、rustup 1.29.0 (28d1352db 2026-03-05)。fmt 无差异；cosir 100/100；workspace cosir 100/100 + xtask 23/23（123/123）；clippy 无 warnings；xtask 六条 PASS；独立 verification verdict PASS（无 Critical/Important）。仅支持 host `MODEL-TEST`、workspace regression/lint 与既有 QEMU 启动闭环无回归；xtask 不证明真实 trap/`iretq`。不证明 `KERNEL-TRACE`、`REFINEMENT-PROOF` 或机器证明，也不表示阶段退出条件整体满足。 | S0-D1、S0-D2、S1-D1、S1-D2、S1-X1 |
| TEST-006 | 最终整体审查修复验收 | `cargo fmt --all --check`、`cargo test -p cosir`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo xtask test`、`git diff --check` | 2026-08-16 | cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29)；fmt 无差异；cosir 180/180；workspace cosir 180/180 + xtask 23/23（203/203）；clippy 零 warnings；QEMU BIOS run=1/2、run=2/2 均通过，均输出六条 PASS；`git diff --check` 无错误。仅支持 host `MODEL-TEST`、workspace regression/lint 与既有 QEMU 启动闭环无回归；不支持 `KERNEL-TRACE`、`REFINEMENT-PROOF`、`COMPAT-TEST`、真实硬件 TLB、帧字节复制或 BLK-001 关闭，且不表示阶段退出条件满足。 | S0-D1、S0-D2、S0-D4、S1-D1、S1-D2、S1-X1 |

新增证据时使用稳定 ID；阶段检查项只引用该表中的 ID，不复制大段输出。

## 已知问题与纠偏计划

| ISSUE ID      | 问题与影响                                            | 当前证据                                                        | 修复阶段 | 可执行验收条件                                                                                                                                            | 状态             |
| ------------- | ------------------------------------------------ | ----------------------------------------------------------- | ---- | -------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| ISSUE-MEM-001 | pagewalk 接受未对齐/非 canonical 地址，high-half 投影缺少符号扩展 | MODEL-TEST 暴露缺口                                             | 阶段一  | 地址别名、对齐、高低半测试通过；`cargo test -p cosir pagewalk_tests -- --nocapture`、`cargo test -p cosir memseg_refine_tests -- --nocapture`、`cargo test -p cosir` | Fixed          |
| ISSUE-MEM-002 | 地址、页数、offset、usize 转换和 SegId 递增可能溢出              | MODEL-TEST 暴露缺口                                             | 阶段一  | 每个溢出返回稳定错误且状态不变；`cargo test -p cosir pagewalk_tests -- --nocapture`、`cargo test -p cosir memseg_refine_tests -- --nocapture`、`cargo test -p cosir` | Fixed          |
| ISSUE-MEM-003 | α 未验证物理帧、offset 和 PTE 权限位                        | REFINEMENT-DESIGN 不完整                                       | 阶段一  | 七项独立 mutation 均被拒绝                                                                                                                                 | Fixed          |
| ISSUE-MEM-004 | allocator 未实施严格有限集合语义                            | MODEL-TEST 暴露缺口                                             | 阶段一  | 重复、外部帧、重复归还均显式失败                                                                                                                                   | Fixed          |
| ISSUE-MEM-005 | memseg_create 错误要求预存 Segment 映射权                 | 与 POSIR §7.1 不一致                                            | 阶段一  | 删除 seg_cap_rights 参数                                                                                                                               | Fixed          |
| ISSUE-MEM-006 | SPEC-014/PASS 可能被外推为真实页表细化                       | MODEL-TEST + REFINEMENT-DESIGN                              | 阶段一  | 三份权威文档和 harness 文案一致                                                                                                                               | Fixed          |
| ISSUE-EXC-001 | exc_resume 未检查 Capability、持有者、活跃状态和订阅            | TEST-005 host `MODEL-TEST` 与独立 verification PASS            | 阶段二  | 每项授权 mutation 原子失败                                                                                                                                 | Fixed          |
| ISSUE-EXC-002 | 恢复帧没有实际字节编码，RIP/RSP/CS/SS/RFLAGS 未完整验证           | TEST-005 host `MODEL-TEST` 与独立 verification PASS            | 阶段二  | wire-format 与字段 mutation tests 通过                                                                                                                  | Fixed          |
| ISSUE-EXC-003 | 成功恢复未清零用户帧、保存 kernel frame 或追加 Audit             | TEST-005 host `MODEL-TEST`、源码审查与独立 verification PASS        | 阶段二  | `MODEL-TEST` 的最终状态、精确 consumed region、二次消费与 Audit 检查通过；源码审查确认提交顺序                                                                                  | Fixed          |
| ISSUE-EXC-004 | §12.9/§12.10、旧设计与实现接口矛盾                          | TEST-005、规范与 tracker 已与三参数接口、`0xA0` 帧、实现提交顺序及 host 模型证据边界一致 | 阶段二  | 三份文档与最终接口、源码审查的提交顺序及 MODEL-TEST 范围一致                                                                                                               | Fixed          |
| ISSUE-COW-001 | 内核式 COW 自动复制在既存原语下不可表达                           | SPEC-015 反例 R4（host `MODEL-TEST`）                           | 阶段二  | R4 为对 §12.8“用户态 COW”的边界澄清，非 H1 反例；COW 复制必须由用户态/LibOS 显式换映射触发                                                                                       | Open（边界澄清，非缺陷） |
| ISSUE-COW-002 | 并发写共享只读页各 fault、无合并原语                            | SPEC-015 反例 R1（host `MODEL-TEST`）                           | 阶段二  | 并发 COW 协调为用户态显式责任；切片未新增内核原语                                                                                                                        | Open（边界澄清，非缺陷） |
| ISSUE-DOC-001 | 原语类别口径、W1–W16 义务与 SegWrite 术语不一致 | TEST-006 文本审计 + 最终验收 | 本轮 | 三份权威文档口径一致，且无未解释的 W1–W14/SegWrite 残留；TEST-006 180/180、203/203、clippy 零告警、QEMU 六条 PASS | Fixed |
| ISSUE-COW-003 | 组合 Authority 边界、Task 生命周期与父侧 COW 闭环缺口 | 最终整体审查 I-1/I-2/I-3 + TEST-006 | 本轮 | 私有子状态、参数化 metadata、父/子公开闭环、授权 Task/Segment 销毁及完整状态原子性测试通过；TEST-006 180/180、203/203、clippy 零告警、QEMU 六条 PASS | Fixed |
| ISSUE-TLB-001 | 非写访问未区分 R/X，失效 API 的 Task/地址语义不一致 | TEST-006 源码审查 + host MODEL-TEST | 本轮 | `AccessKind` R/W/X 矩阵、存活无 cache Task 幂等失效和非法地址原子拒绝测试通过；TEST-006 通过 | Fixed |
| ISSUE-DOC-002 | 已完成计划未归档且含过期帧复用断言 | TEST-006 历史计划与文本审计 | 本轮 | 两计划 Archived；历史帧复用断言更正为等页数、物理帧相离且不代表字节复制；TEST-006 通过 | Fixed |
| ISSUE-CAP-001 | host 模型 Capability 无派生链/位置表/溯源事件，§2.1 目标 2/3/4 无测试抓手 | 静态搜证 + 研究备忘（见 `docs/superpowers/reports/2026-08-17-capability-derivation-model-gap.md`） | 后续独立切片候选 | 与消息机制同批设计；以 §17.2 不可伪造/权限单调/位置唯一为验收不变量 | Open（模型缺口，非缺陷） |

## 研究停止条件与下一垂直切片

阶段一采用以下停止条件：若真实实现要求修改已冻结的 POSIR observable semantics、需要新增 OS/架构专属原语才能维持映射、`COMPAT-TEST` 出现不可接受且无法由既有机制表达的语义偏差，或真实页表/trap trace 无法建立到抽象迁移的可审查对应，则停止扩大实现并记录为 H1/H3 或细化分层的反例。`MODEL-TEST` 或 `REFINEMENT-DESIGN` 不能解除这些停止条件，也不能单独升级为 refinement 结论。

下一垂直切片固定为：**COW/page fault → handler → `exc_resume` → TLB → Linux trace**。该切片必须先冻结 page-fault/COW 的 observable semantics，再以真实 trap、页表更新和 TLB 行为建立端到端 trace，最后与选定 Linux 行为形成 `COMPAT-TEST`；在该闭环完成前不横向扩大 MemorySegment、跨架构或异构实现范围。

## 当前阻塞项

| ID | 阶段 | 问题 | 解除条件 | 状态 |
|---|---|---|---|---|
| BLK-001 | 0 | 尚未选择并建立形式化模型工件 | 明确工具、入口文件和可执行状态迁移 | Closed |
| BLK-002 | 0 | 草案已放宽——已覆盖 MemorySegment 页表↔vspace 细化 | 定义 MachineState、抽象函数和至少 Bootstrap（及 MemorySegment）细化对应 | 草案已成 |

## 决策日志

| 日期 | 决策 | 理由 | 影响 |
|---|---|---|---|
| 2026-07-30 | 首个平台采用 QEMU x86_64 | 先验证启动链、语义和细化关系 | 阶段 1–3 以单核 QEMU profile 为准 |
| 2026-07-30 | 兼容路线采用原生服务 → POSIX 源码 → Linux 静态 ELF | 分离原生模型验证与传统 ABI 复杂度 | 对应阶段 4–6 |
| 2026-07-30 | 不把原语数量作为约束 | 以语义最小、边界清晰和可证明为准 | 新机制必须经过必要性与中立性审查 |
| 2026-07-30 | 新增 Exception 原语而非依赖 Send/Recv 模拟 | 异常/中断的不可伪造来源、广播递送和硬件同步语义与通用消息 IPC 根本不同 | 原语集从 9 类扩展到 11 类；该旧数字是当时的 Bootstrap 后常规机制计数；异常事件源作为一等对象 |
| 2026-07-30 | 新增 Device 原语封装 I/O 资源 | MMIO/DMA 的物理资源隔离、地址连续性和缓存策略与通用 Segment 根本不同 | I/O 对象独立于 MemorySegment；用户态驱动通过 Capability 访问硬件 |
| 2026-07-31 | 规范审查与修复 | 修正 COSIR 原语规范中 18 项语义/一致性/完整性/实现细节问题 | 见审查报告（本次会话）；ObjectRef 统一、CapMoved 审计事件补充、Node 完整定义、POSIR/COSIR 边界清单等 |
| 2026-08-06 | BIOS 测试失败保护固定为 30 秒 | 原 10 秒阈值在主机 CPU 高负载下出现间歇误报；同一镜像与参数在负载回落后实测仍需 7.079 秒启动 | 不增加自动重试；成功仍必须同时满足 QEMU 退出码 33 与严格 PRIMA 事件序列 |
| 2026-08-16 | 内核实现代码按“唯一允许 unsafe 的细化 crate + safe 机制 crate”组织 | 借鉴 framekernel 内核内特权分离：unsafe 收敛使可信代码可度量、可审计（见差距分析报告 G2，docs/superpowers/reports/2026-08-16-framekernel-gap-analysis.md） | 约束未来 kernel/COSIR 实现代码；待代码量增长后执行审计与重组（G8） |
| 2026-08-16 | COW 切片真实复制路径采用 POD read/write 接口形态 | host 模型不模拟内存字节；真实 trap 层表达“复制帧内容”参照 framekernel untyped memory 的读写接口而非引用（报告 G9） | 作为下一垂直切片的设计输入 |

## 阶段 0 — 语义基线

**状态：进行中**

### 交付检查

| ID | 交付物 | 状态 | 证据 |
|---|---|---|---|
| S0-D1 | 一致的对象能力状态模型 | [x] 已定义，未证明 | SPEC-001 |
| S0-D2 | 操作、错误、全局不变量和非目标 | [x] 已定义，未证明 | SPEC-001 |
| S0-D3 | 关键研究假设与反例标准 | [x] 已定义 | DOC-001 |
| S0-D4 | x86_64 机器状态到 POSIR 状态的细化草案 | [x] 已定义，未证明 | SPEC-013 |

### 退出条件

| ID | 条件 | 满足 | 证据 |
|---|---|---|---|
| S0-X1 | 规范没有已知且未记录的对象生命周期空洞 | [ ] | 需要独立规格审查或机器检查证据 |
| S0-X2 | 每项操作具有明确成功与失败语义 | [ ] | 需要逐操作覆盖报告 |
| S0-X3 | 形式化工具能够表达并执行状态与迁移 | [x] | SPEC-019 |

阶段 0 不能因文档篇幅完整而直接关闭；必须补齐形式化工件、覆盖报告和细化草案。
本阶段仍**进行中**：S0-X1（独立规格审查）需另行推进，不在 BLK-001 切片内完成；S0-X2 逐操作覆盖报告亦未完成。S0-X3 由 BLK-001（Lean 可执行模型 + cosir 对拍，证据 SPEC-019）满足。

## 阶段 1 — 启动与最小平台层

**状态：进行中**

### 交付检查

- [x] S1-D1：QEMU 可重复启动；证据：TEST-001、TEST-002、TEST-003（固定 BIOS profile 连续两次）。
- [x] S1-D2：串口结构化输出与确定性退出；证据：IMPL-001、TEST-001、TEST-002、TEST-003。
- [x] S1-D3：物理内存发现；证据：页帧清单与保留区域测试（SPEC-017）。
- [x] S1-D4：初始页表、GDT/IDT 与 trap 框架；证据：异常和权限测试（SPEC-018）。
- [x] S1-D5：Bootstrap 创建 init Task；证据：SPEC-020 初始对象和 Capability 快照。

### 退出条件

- [x] S1-X1：脚本可无人值守启动并退出 QEMU；证据：TEST-001、TEST-002、TEST-003。
- [ ] S1-X2：init 只获得规范定义的初始 Authority。
- [ ] S1-X3：活动 Segment 与空闲页帧满足唯一归属不变量。

## 阶段 2 — Capability、Segment、Task 与 IPC

**状态：未开始**

### 交付检查

- [ ] S2-D1：不可伪造且不复用的句柄表示。
- [ ] S2-D2：Capability 派生、移动、drop 与 stale 检测。
- [ ] S2-D3：Segment create/map/unmap/destroy。
- [ ] S2-D4：Task create/yield/destroy 与单核调度。
- [ ] S2-D5：Send/Recv、FIFO 邮箱与阻塞恢复。

### 退出条件

- [ ] S2-X1：每项成功和失败路径具有状态转换测试。
- [ ] S2-X2：两个不可信 Task 只能经授权消息或共享 Segment 交互。
- [ ] S2-X3：页帧在对新 Segment 可见前清零。
- [ ] S2-X4：stale handle 不会因 ID 复用重新获得 Authority。

## 阶段 3 — Namespace、Timer 与 Audit

**状态：未开始**

### 交付检查

- [ ] S3-D1：Namespace bind/unbind/mount/unmount/resolve。
- [ ] S3-D2：Mount 环检测与 stale Mount 清理。
- [ ] S3-D3：绝对单调 Timer。
- [ ] S3-D4：`deadline ≤ now` 时原子立即触发。
- [ ] S3-D5：Capability 派生、位置变化与对象生命周期 Audit。

### 退出条件

- [ ] S3-X1：所有 Namespace 解析均终止，活动 Mount 图无环。
- [ ] S3-X2：每个 Fired Timer 恰有一次对应内核通知。
- [ ] S3-X3：Audit 能重建 Capability 派生链和位置历史。
- [ ] S3-X4：Task、Namespace、Message 或 Timer 容器销毁不破坏其他对象生命周期。

## 阶段 4 — Prima 原生运行时与服务

**状态：未开始**

### 交付检查

- [ ] S4-D1：原生运行时 API 与版本规则。
- [ ] S4-D2：init / 服务管理器。
- [ ] S4-D3：日志、Namespace 和基本存储服务。
- [ ] S4-D4：第一条用户态驱动或受控 I/O 路径。
- [ ] S4-D5：使用消息与共享 Segment 的原生示例应用。

### 退出条件

- [ ] S4-X1：不依赖 POSIX LibOS 即可自举并运行应用。
- [ ] S4-X2：服务按最小权限拆分，证据列出各服务初始 Capability。
- [ ] S4-X3：大数据路径可使用共享 Segment，且有取消和错误恢复测试。
- [ ] S4-X4：故障服务不能直接破坏无关应用域。

## 阶段 5 — POSIX 源码兼容

**状态：未开始**

### 交付检查

- [ ] S5-D1：冻结且版本化的 POSIX profile。
- [ ] S5-D2：libc 适配。
- [ ] S5-D3：文件描述符、路径、线程和基础网络子集。
- [ ] S5-D4：逐 API 语义映射矩阵。
- [ ] S5-D5：与参考 Linux 的差分测试矩阵。

### 退出条件

- [ ] S5-X1：profile 内每项 API 都有语义映射和可重复测试。
- [ ] S5-X2：每项 API 明确标记 supported / partial / unsupported。
- [ ] S5-X3：选定开源程序可重新编译并通过冻结的功能测试集。

## 阶段 6 — Linux x86_64 静态 ELF

**状态：未开始**

### 交付检查

- [ ] S6-D1：静态 ELF 加载器与 Linux 初始栈。
- [ ] S6-D2：OS 中立的 syscall / exception 递送机制。
- [ ] S6-D3：版本化的 Linux LibOS syscall profile。
- [ ] S6-D4：所选 profile 必需的 futex、TLS 和进程环境。
- [ ] S6-D5：未修改静态 ELF 白名单与测试镜像。

### 退出条件

- [ ] S6-X1：白名单中的静态 ELF 无需修改即可运行。
- [ ] S6-X2：支持的 syscall 与参考 Linux 通过差分测试。
- [ ] S6-X3：HexArc / COSIR 不包含 Linux 专属对象或 syscall 语义。
- [ ] S6-X4：未支持 syscall 得到确定且可审计的失败。

## 阶段 7 — 扩展与硬化

**状态：未开始；范围未冻结**

候选工作不预先视为承诺：

- [ ] SMP；
- [ ] IOMMU 与更完整的用户态驱动；
- [ ] 动态链接；
- [ ] 资源计量与配额；
- [ ] 选择性撤销；
- [ ] 更强的故障恢复；
- [ ] 第二硬件架构。

只有在阶段 0–6 的证据揭示实际优先级后，才把候选项拆成带退出条件的独立里程碑。

## 阶段晋级规则

1. 阶段状态由该阶段的退出条件决定，不由提交数量、代码行数或主观完成度决定；
2. 前一阶段未满足退出条件时，后一阶段可以做探索原型，但不能成为“当前已完成阶段”；
3. 任何失败测试、未解决反例或实现偏差都必须进入阻塞项或决策日志；
4. 关闭阶段时，在证据索引中记录完整命令、工件版本和结果；
5. 架构或语义变化必须先更新权威文档，再重新评估受影响的已关闭检查项。
