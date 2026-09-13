# Prima OS 实现阶段

> 文档类型：实施进度与证据跟踪
>
> 更新日期：2026-08-29
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
| 当前首要缺口 | 已冻结的下一垂直切片 **COW/page fault → handler → `exc_resume` → TLB → Linux trace** 中 **ring3 用户态递送、用户态 COW 与 syscall 调用/返回协议已落地（SPEC-045~047）**：#PF 捕获（SPEC-040/041）、验证式 `exc_resume`（SPEC-042）、自建页表/TLB（SPEC-043）、内核侧 COW（SPEC-044）、CPL3 用户态故障→内核 COW→恢复用户态（SPEC-045）、CPL3 `int 0x80` 最小递送（SPEC-046）、syscall ABI v1（6 参数寄存器 + 负 errno 编码 + `SYS_SUM4`/`SYS_COPY_IN` + `EFAULT`/`EINVAL`/`ENOSYS`，SPEC-047）、x86-64 `syscall`/`sysret` 快路径（EFER.SCE/STAR/LSTAR/FMASK + 真实 `syscall` 往返，SPEC-048）、capability 授权查询接入 syscall（真实初始/派生 Capability 表 + 授予/`EACCES`/`ENOENT`，SPEC-049）、异常订阅生命周期 + pending 直投 + Audit 接入 syscall（SPEC-050：预定义 `page_fault_source`/`syscall_trap_source` + capability 门控 `subscribe`/`unsubscribe` + BlockedRecv→pending→Ready + 四类 Audit 事件）、双 ring3 任务协作调度 + 真实 Task 身份绑定（SPEC-051：`SYS_TASK_ID`/`SYS_YIELD` + 全用户上下文保存/恢复 + 授权/订阅改用当前 Task id，实测 task1 授予、task2 `EACCES`）；尚缺 **用户可调 `raise`/`wait` 的完整 `syscall` 原语、抢占/per-task CR3 的完整多任务与 Linux trace / `COMPAT-TEST`**；当前 S1-X2（初始 Authority）经 SPEC-039 仅收紧、现由 SPEC-051 进一步收紧真实 Task 身份，S1-X3（页帧全局归属）经 SPEC-032~038 仅收紧，均未关闭；S0-X1（独立规格审查）与 S0-X2（逐操作覆盖报告）仍待补齐 |
| 最近规范修订 | 2026-08-26 — 白皮书 §6.2 补全选择性撤销的撤销域定义（撤销域/派生树一致性/与 cap_drop 正交/token 中间态/在途位置不纳入），§8.5 新增"下一阶段机制能力映射"（归属层+假说+规范锚点表），白皮书与 COSIR 规范 §4.7 回链同步（DOC-006）；此前 2026-07-31 — 规范审查修复：ObjectRef 统一、ExternalInterrupt 语义明确、CapMoved 审计事件补充、Node 对象完整定义、DMA 帧/页帧信息流说明、POSIR/COSIR 边界清单、ns_mount 跨节点约束、task_create 增加 kind 参数、task_yield 调度时机明确、应用域/LibOS 关系精确化、细化要求从 21 项扩展到 25 项 |
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
| Capability 生命周期与撤销（本切片） | 2026-08-25 — `derive_capability`/`revoke_capability`/`revoke_task_capabilities` 实现初始与派生根的子树撤销传播、跨两级回收与审计（SPEC-022/023）；`validate_authority`（SPEC-039）校验 ID 唯一、位置/活动一致、holder 存活与派生权限单调。证据 `MODEL-TEST`；仅收紧 S1-X2，不关闭。 |
| Task/IPC/Audit（本切片） | 2026-08-25 — `TaskTable`（单 Running、schedule/yield、销毁前清理）、`IpcQueue`/`send_message`/`receive_message`（FIFO + blocked 唤醒 + 直接交付）、`AuditLog`（MessageSent/Received/Dropped/Derived/Moved/Revoked/TaskDestroyed）全链路引入，覆盖 S2-D4/D5 与 S3-D5（SPEC-024~031）。证据 `MODEL-TEST`；不关闭 S1-X3、IPC 生命周期联动或完整 Authority 不变量。 |
| 页帧所有权与页表审计（本切片） | 2026-08-26 — cosir 细化模型为页表节点引入 backing frame 分配/回收与 W6 守恒扩展（Segment∪页表帧∪空闲帧，SPEC-033~036）；真实内核读取 CR3、审计可达页表树并确认外部根页帧归属（SPEC-037/038）。均未接入内核自建页表，仅收紧 S1-X3，不关闭。 |
| 切片 C：#PF 捕获闭环（本切片） | 2026-08-29 — 真实内核注册 vector 14，用裸 `page_fault_entry`（手动现场/CR2 记录/弹 error code/iretq）捕获一次真实 #PF：读 CR2、`translate` 自证探测地址 `0x40000` 未映射，新增 `page_fault_ready(addr;not_mapped)` 事件并被 QEMU 验收断言 `addr==0x40000 ∧ not_mapped==1`。证据 SPEC-040；2026-09-05 深化为硬件 error-code 语义（切片 Task 1-5，证据 SPEC-041）：`page_fault_entry` 把硬件压栈的 error code 存为 `last_fault_err`（原被弹出丢弃），新增 `kernel/src/fault.rs` 纯解码器 `classify`（err 由真实 CPU 驱动：`err=0x0`→`not_present`、P1W1→`WriteProtection`、其余→`Other`，含 absent-write 反混淆并扣除读路 W 位误判）；`page_fault_ready` 泛化为 `addr=<hex>;err=<hex>;class=<name>`，移除软件 `translate` 自证，`err`/`class` 由真实 CPU 驱动并经 xtask 跨事件断言（缺页实读 `addr=0x40000;err=0x0;class=not_present`）。新增 CR0.WP 总闸门 `kernel::cpu::read_cr0_wp()`（纯算术 `wp_of` 可 host 单测钉死位判定；特权 `mov cr0` 仅内核 CPL0/QEMU 内执行）、`main.rs` PF 阶段后打印非-PRIMA 信息行，QEMU 实测 `[pf] cr0.wp=1`（本切片首个真实 WP 活值；WP=1 ⇒ 当前环境 supervisor 写只读页故障，仅 bootloader CR3 无 present-只读页可触发，故只记录现场能力不伪报捕获）。仍不宣称 ring3 递送、`ops::raise_exception`/`exc_resume` 桥接、完整异常分派（16 向量+IRQ/PIC）或 `KERNEL-TRACE`。 |


当前已形成架构白皮书和抽象语义规范，并有阶段 1 启动闭环探索实现：真实内核物理内存发现（S1-D3，SPEC-017）、真实页表/GDT/IDT/trap 框架（S1-D4，SPEC-018）、Bootstrap 初始 Authority 快照（S1-D5，SPEC-020）、Capability 动态位置/撤销/任务表/IPC/Audit（S1-X2/S2、S3，SPEC-021~031）、页帧所有权/页表审计（S1-X3，SPEC-032~039）、切片 C #PF 捕获闭环（SPEC-040 捕获、SPEC-041 error-code/classify/CR0.WP 深化）、切片 D #PF→验证式 `exc_resume` 桥接（SPEC-042：完整现场捕获 + 纯校验器 + 64 位裸 iretq 跳板 + 四车道恢复自证）、切片 E 真实页表更新/TLB 见证（SPEC-043：自建 4 级映射 + 陈旧 TLB 现场 + invlpg 刷新 + 真实写保护 #PF）、切片 F 真实 COW（SPEC-044：分派式 #PF + 帧字节复制 + 换映射 + 重试原指令）与切片 G ring3 用户态 COW（SPEC-045：DPL3 描述子 + U/S 页表 + CPL3 执行用户代码 + 用户态写故障→内核 COW→回 CPL3 重试→int3 回执）、切片 H 最小 syscall 递送（SPEC-046：CPL3 `int 0x80` DPL3 门 + 号/参数/返回寄存器往返 + 合法回显 + 非法号确定性 `-ENOSYS`）、切片 I syscall ABI v1（SPEC-047：6 参数寄存器 + 负 errno 编码 + 用户缓冲 `COPY_IN` 的 `EFAULT`/`EINVAL`）、切片 J `syscall`/`sysret` 快路径（SPEC-048：EFER.SCE + STAR/LSTAR/FMASK 配置 + 真实 `syscall`/`sysretq` 往返）、切片 K syscall 授权查询（SPEC-049：真实 Bootstrap 初始/派生 Capability 表 + 授予/`EACCES`/`ENOENT`）、切片 L 异常订阅与 pending 递送（SPEC-050：平台初始化显式创建 `page_fault_source`/`syscall_trap_source` 并铸造 `ExcSubscribe` 根 Capability + 内核侧 BlockedRecv 任务经真实 pending 直投唤醒 + CPL3 `SYS_EXC_SUBSCRIBE`/`SYS_EXC_UNSUBSCRIBE` 的授予/重复/kind 不符/取消 + 四类真实 Audit 事件）、切片 M 双 ring3 任务协作调度（SPEC-051：`SYS_TASK_ID`/`SYS_YIELD` + 复用 `exc_resume` 保存/恢复完整用户上下文 + TaskTable Running/Ready 协作迁移 + 授权/订阅改用真实当前 Task id，实测 task1→task2→task1 两次往返且 task2 因非持有者得 `EACCES`），并建立 Lean 4 可执行状态模型（S0-X3，SPEC-019，BLK-001 关闭）。尚无机器证明、完整 POSIR Bootstrap、用户可调 `raise`/`wait` 的完整 `syscall` 原语、抢占/per-task CR3 的完整多任务、替换 CR3 的完整自建页表体系、Linux trace 或兼容性测试证据；S0-X1/S0-X2 仍待补齐，S1-X2/S1-X3 均仅收紧未关闭。

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
| SPEC-033 | 细化模型 MODEL-TEST | `cosir/src/pagewalk.rs`、`cosir/src/memseg_refine.rs` | 2026-08-26 | `FrameAllocator::check_invariants` 统一验证 frame universe 唯一、空闲帧唯一且属于 universe；`memseg_alpha` 复用该检查，并继续验证活动 Segment 帧与空闲集合互斥、并集守恒、Segment 内无重复。单测覆盖分配/归还成功、重复空闲状态和 universe 外帧状态。页表树仍为逻辑模型节点，未宣称物理页表页的分配与回收，因此仅收紧 S1-X3 模型边界，不关闭完整 S1-X3 | S1-X3 |
| SPEC-034 | 细化模型 MODEL-TEST | `cosir/src/pagewalk.rs`、`cosir/src/memseg_refine.rs` | 2026-08-26 | `PageTable::check_invariants` 校验根节点级别、Pml4→Pdpt→Pd→Leaf 层级、非根非叶节点不得为空、叶节点不得为空；`memseg_alpha` 对每个 Task 页表执行该检查。单测覆盖映射/解除映射后的结构、错误根级别和孤立节点。该检查仍针对逻辑页表树，未引入物理页表页帧所有权，故不关闭完整 S1-X3 | S1-X3 |
| SPEC-035 | 细化模型 MODEL-TEST | `cosir/src/pagewalk.rs` | 2026-08-26 | 新增带 `FrameAllocator` 的页表生命周期 API：`new_owned_task_table` 为根页表分配 backing frame，`map_vaddr_owned` 为新建页表节点分配帧，`unmap_vaddr_owned` 回收被剪枝节点；`check_frame_ownership` 校验页表帧数量、唯一性、属于 universe 且不在 free 集合。单测覆盖创建、映射、解除映射回收和帧归属污染。该 API 尚未接入 `RefineState` 的 Segment 数据帧分配，也未接入真实内核页表，因此不关闭完整 S1-X3 | S1-X3 |
| SPEC-036 | 细化模型 MODEL-TEST | `cosir/src/memseg_refine.rs`、`cosir/src/cow_refine.rs`、`cosir/src/tlb_refine_tests.rs` | 2026-08-26 | `RefineState` 的 Task 创建、映射、解除映射和 Task 销毁已接入页表 backing frame 分配/回收；W6 守恒扩展为 Segment 帧∪页表帧∪空闲帧，批量 map/unmap 在副本上原子提交；COW Task 销毁回收页表帧。相关 fixture 扩容并覆盖完整生命周期，QEMU 启动闭环无回归。仍是 COSIR 细化模型，未接入真实内核 CR3 页表分配器，因此不关闭完整 S1-X3 | S1-X3 |
| SPEC-037 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/mem.rs`、`kernel/src/main.rs`、`kernel/src/cpu.rs` | 2026-08-26 | 真实启动读取并掩码 CR3 根地址；`validate_external_page_table_root` 要求根页表帧 4KiB 对齐、位于 bootloader memory map 且不得被分类为 `Usable`，从而明确 bootloader 外部拥有、内核不回收该根帧。host 单测覆盖 Bootloader/Usable/越界/未对齐分类，QEMU BIOS run=1/2 继续通过。仅审计根页表帧，不证明整棵页表页集合、CR3 切换或内核自建页表，因此不关闭完整 S1-X3 | S1-X3 |
| SPEC-038 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/cpu.rs`、`kernel/src/mem.rs`、`kernel/src/main.rs` | 2026-08-26 | 启动路径尝试递归遍历当前 CR3 可达的 PML4/PDPT/PD/PT 页表页；根页通过 4KiB 对齐、memory map 且非 `Usable` 审计，整树审计器提供层级、共享节点去重、大页终止和最多 1024 页表页保护，但对 bootloader 兼容性错误不阻断启动；host 单测覆盖四级遍历，QEMU BIOS run=1/2 通过。该证据不证明子页表物理所有权、内容完整性、CR3 切换或内核自建页表，不关闭完整 S1-X3 | S1-X3 |
| SPEC-039 | 真实内核 MODEL-TEST | `kernel/src/bootstrap.rs` | 2026-08-26 | `BootstrapState::validate_authority` 校验初始/派生 Capability ID 唯一、位置覆盖与活动状态一致、holder 存活、派生父权限单调收窄，以及消息队列中的 Capability 位置一致；`create_state` 要求该校验通过后才返回。单测覆盖位置状态损坏、死亡 holder 与权限升级拒绝。证据仅为 `MODEL-TEST`，未覆盖完整动态跨节点 Authority、Task pending 直送消息联动或机器证明，因此仅收紧 S1-X2，不关闭 S1-X2 | S1-X2 |
| SPEC-040 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/idt.rs`（`Entry`/`set_handler_addr`/`page_fault_entry` naked 入口/`trigger_page_fault`/`fault_addr`）、`kernel/src/main.rs`（`PROBE_FAULT_VA`/translate 自证/触发/上报）、`kernel/src/protocol.rs`（`page_fault_ready`）、`xtask/src/protocol.rs`（`PageFaultReady`/`validate_page_fault_data`/10 事件序列） | 2026-08-29 | 切片 C 第一步：真实内核为 vector 14 注册裸 `page_fault_entry`（naked 手动处理现场：读 CR2 记录、把返回地址前移 3 字节跳过故障指令、弹出硬件压栈 error code、iretq），运行期触发一次真实 `#PF`（读未映射探测地址 `0x40000`），触发前用现有 `translate` 自证该地址 `PageNotMapped`。新增 `page_fault_ready` 事件上报 `addr`（CR2）与 `not_mapped`，xtask 断言 `addr==0x40000 ∧ not_mapped==1`。证据 `MODEL-TEST` + QEMU：`cargo test --workspace` 全绿（cosir 145、kernel 63、xtask 41 等）、clippy `-D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2 六条 PASS 退出 0。实现在既有 `trap->handler->iretq`（vector 3）基础上验证了"异常现场（CR2+error code）→ handler → `translate` 缺页判定 → 事件上报"的真实 trap 闭环。不宣称 ring3/用户态递送、`ops::raise_exception`/`exc_resume` 桥接、完整异常分派（16 向量+IRQ+PIC）、真实内核自建页表或 `KERNEL-TRACE`；error code 值未通过协议外传 | S1-D4 |
| SPEC-041 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/idt.rs`（`page_fault_entry` 存 error code/`last_fault_err()`）、`kernel/src/fault.rs`（`classify` 纯解码器）、`kernel/src/cpu.rs`（`read_cr0_wp`/`wp_of`）、`kernel/src/main.rs`（无 translate 自证 /PF 触发上报/WP 信息注记）、`kernel/src/protocol.rs`、`xtask/src/protocol.rs` | 2026-09-05 | 切片 C 第二/终端步：#PF 捕获从"CR2 + 软件 translate 判 not_mapped"升级为真实硬件 error-code 语义。`page_fault_entry` 现在把硬件压栈的 error code 存为 `last_fault_err()`（不再丢弃），`kernel/src/fault.rs` 纯解码 `classify`：`err=0x0`→`not_present`、`P=1∧W=1`→`WriteProtection`、其余→`Other`，含 absent-write 反混淆（区分硬件到底给没给 W/R 位，避免把读路误判为写保护）；`page_fault_ready` 泛化为 `addr=<hex>;err=<hex>;class=<name>`，移除内核侧 `translate` 自证，`err`/`class` 由真实 CPU 驱动并按 SPEC-040 探测值被 xtask 断言 `addr=0x40000;err=0x0;class=not_present`（首次在真实启动中 round-trip 两侧合并实现，六条 PASS 退出 0）。新增 CR0.WP 总闸门 `read_cr0_wp()`：位判定抽为纯算术 `wp_of` 可 host 单测钉死，特权 `mov cr0` 仅 QEMU/CPL0 内执行，C 阶段后在 `main.rs` 打印非-PRIMA 信息行，QEMU 实测输出 `[pf] cr0.wp=1` —— 本切片首个真实 WP 活值（也实证了 CPL3 host 无法读取该信息的边界，观测为 QEMU 侧等价物）。总验收 2026-09-05：`cargo test --workspace` 303/303（cosir 145、cosir_state 40、lean_backtrace 2、kernel 74、xtask 42）、clippy `-D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2 六条 PASS 退出 0。真实 `present∧write` 写保护现场为 best-effort：取决于 CR0.WP=1 且 bootloader CR3 存在对 supervisor 写只读的可触发页，二者并不都保证，故仅记录、不硬断言，登记为自建页表后的后续工作。不宣称 ring3/用户态递送、`ops::raise_exception`/`exc_resume` 桥接、完整异常分派（16 向量+IRQ+PIC）、真实内核自建页表或 `KERNEL-TRACE` | S1-D4 |
| SPEC-042 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/exc.rs`（`SretFrame` 编解码/`validate_for_kernel_resume`/`valid_kernel_rflags`/`exc_resume_entry`/`resume`）、`kernel/src/idt.rs`（`page_fault_entry` 完整现场捕获/`fault_frame`）、`kernel/src/resume.rs`（`resume_landing`/`after_resume`/`run_and_exit`）、`kernel/src/protocol.rs`（`exc_resume_ready`）、`xtask/src/protocol.rs`（`ExcResumeReady`/`validate_exc_resume_data`/11 事件序列） | 2026-09-12 | 切片 D：真实 #PF 现场→验证式 `exc_resume` 桥接。`kernel/src/exc.rs` 定义与抽象模型（`cosir::state::SretFrame`）逐字节一致的 0xA0 帧（20×u64，`encode`/`decode`，host 单测钉死全部字段偏移）与纯校验器 `validate_for_kernel_resume`（canonical RIP/RSP、RIP 命中 landing、RSP 16 对齐且落在恢复栈 `[bottom, top]`、CS/SS 为内核选择子、`valid_kernel_rflags` 禁 TF/IF/IOPL/NT/RF/VM/VIF/VIP 与保留位）；`page_fault_entry` 从"只存 CR2+error code"升级为保存全部 15 个 GPR + 硬件压栈 rip/cs/rflags + 合成的原始 rsp + 实时 ss（`fault_frame()`），并修复原有 handler 泄漏 rax/rdi/rsi 的问题；`kernel/src/resume.rs` 用专用 16 KiB `.bss` 恢复栈与裸 `exc_resume_entry`（压入完整 64 位 iret 帧 `SS:RSP:RFLAGS:CS:RIP`、加载除 rsp 外全部 GPR、`iretq`）真实恢复进 `resume_landing`，落点以四条独立车道（r12/r13 哨兵、rsp==恢复栈顶、RFLAGS.ZF）自证"确实恢复而非盲跳"。**关键修正：64 位 `iretq` 无条件弹出 5 元帧（含 SS:RSP），初版 3 元件导致 RSP 取自恢复栈之外（QEMU 实测 RSP=0 → 三故障）；经 `-d int` 逐异常定位后改为完整帧并通过。** 新增 `exc_resume_ready` PRIMA 事件（seq 10，`boot_complete` 顺延 seq 11），发出前在 `main.rs` 的 `bootstrap_ready` 之后调用：QEMU 实测 `data=sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=1`，由 xtask 逐项断言。总验收 2026-09-12：`cargo test --workspace` 327/327（cosir 145、cosir_state 40、lean_backtrace 2、kernel 92、xtask 48）、clippy `-D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2 六条 PASS 退出 0。**边界**：ring0→ring0 only（GDT 无 DPL3 选择子，不做 ring3/用户态递送）；恢复的是"指向 landing/恢复栈的改写帧"，不是被捕 #PF 现场本身（RIP/RSP 被改写，其余寄存器为被捕值）；不是抽象模型的 `exc_resume`（无 capability/subscription/pending/`TaskResumed` audit/`Faulted→Ready`）；未接 TLB、未做 `KERNEL-TRACE`/`REFINEMENT-PROOF`/`COMPAT-TEST` | S1-D4 |
| SPEC-043 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/vspace.rs`（`map_4k`/`set_leaf`/`read_leaf`/`invlpg`/`run_witness`/`Witness`）、`kernel/src/idt.rs`（`trigger_page_fault_write`）、`kernel/src/main.rs`（保留 allocator + vspace 见证接线）、`kernel/src/protocol.rs`（`vspace_ready`）、`xtask/src/protocol.rs`（`VspaceReady`/`validate_vspace_data`/12 事件序列） | 2026-09-12 | 切片 E：真实页表更新 + TLB 行为见证 + 真实写保护 #PF。利用已启用的物理直映射，在**空闲 PML4 槽 1**（`VSPACE_VA=0x80_0000_0000`，与 i4=0/2/5 隔离，入口先 `slot_free` 校验）下**自建一条 4 级映射**：`vspace.rs` 纯逻辑 `map_4k`（两阶段：先统计缺失页表页并一次性分配，任一失败返回 `NoFrame` 且零写入）经 host 单测钉死（8 条：标志位/索引/建全层/复用路径/分配失败原子性/清只读/BadRoot×2，用 `std::alloc` 4096 对齐假帧池 + identity map）。boot 路径从 `FreeFrameAllocator` 取 2 数据帧 A/B 写入魔数，建 `VA→A` 读回 `MAGIC_A`（PTE 驱动翻译）→ 改叶 `VA→B` **不刷新**读回 `MAGIC_A`（**真实陈旧 TLB 现场**）→ `invlpg` 后读回 `MAGIC_B`（真实页表更新 + TLB 刷新）→ `cpu::translate` 对账 `translated==frame_b` → 把叶改为只读并 `invlpg`，执行 3 字节 store（`mov [rax], rax` = `48 89 00`，与 `FAULT_SKIP_LEN=3` 成对）触发**真实 `present∧write` #PF**：`wp_addr==VSPACE_VA`、`wp_err==0x3`、`fault::classify`→`write_protection`。**这关闭了切片 C/SPEC-041 记录为 best-effort 的写保护现场边界**（此前只能在 bootloader CR3 上观测 CR0.WP=1，无可触发只读页）。新增 `vspace_ready` PRIMA 事件（seq 10；`exc_resume_ready`/`boot_complete` 顺延 11/12），xtask 逐项断言并含 7 条反例。真实 QEMU 输出：`read_before=0xaaaa000011112222;stale_read=0xaaaa000011112222;read_after=0xbbbb000033334444;translated=0x1000;wp_addr=0x8000000000;wp_err=0x3;class=write_protection`。总验收 2026-09-12：`cargo test --workspace` 342/342（cosir 145、cosir_state 40、lean_backtrace 2、kernel 100、xtask 55）、clippy `-D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2 六条 PASS 退出 0。**边界**：不替换 bootloader CR3、不重建整棵页表（仅新增一条隔离映射）；`stale_read` 的陈旧观测依赖 QEMU TCG 的 TLB 模型（校验器接受陈旧或已一致两值，确定性由 `invlpg` 后的 `read_after` 承担）；未实现完整 COW 复制/换映射用户态协议、未接 Linux trace/`COMPAT-TEST`、未做 `KERNEL-TRACE`/`REFINEMENT-PROOF` | S1-D4、S1-X3 |
| SPEC-044 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/cow.rs`（`COW_VA`/`pattern`/`CowWitness`/`prepare`/`run`/`dispatch`）、`kernel/src/idt.rs`（`page_fault_entry` 改为分派式：`call cow::dispatch` 决定 iretq RIP）、`kernel/src/main.rs`、`kernel/src/protocol.rs`（`cow_ready`）、`xtask/src/protocol.rs`（`CowReady`/`validate_cow_data`/13 事件序列） | 2026-09-12 | 切片 F：**真实 COW（写时复制）**。在空闲 PML4 槽 16（实测该 bootloader 占用槽 0..8 与切片 E 的槽 1，槽 16 隔离）自建一条**只读**映射，预填可识别模式 `pattern(i)=0xC0FFEE00_0000_0000|i`；对页首执行真实写 → `present∧write` #PF（err=0x3）。**#PF handler 从"盲目跳过 3 字节"升级为分派式**：naked 入口捕获完整现场后 `call cow::dispatch(&FAULT_FRAME) -> u64`，返回 iretq 应恢复到的 RIP（默认 `rip+FAULT_SKIP_LEN` 保持切片 C/E 行为；命中 COW 时返回原 `rip` 重试）；调用用 `mov rbp,rsp; and rsp,-16; call; mov rsp,rbp` 保证 SysV 对齐，15 个 GPR 已全部压栈故 clobber 安全。分派器执行：分配私有帧 → **逐 qword 复制** 512 项（POD read/write 形态，落实决策日志 2026-08-16）→ `set_leaf` 改可写指向私有帧 → `invlpg` → 重试。真实 QEMU 实测 `cow_ready`：`err=0x3;class=write_protection;shared0=0xc0ffee0000000000`（**原共享帧未被修改**）`;private0=0x80000000000`（重试的 store 已执行）`;private1=0xc0ffee0000000001;private_last=0xc0ffee00000001ff`（**整页字节已复制**）`;read_back=0x80000000000`（重试成功）。**这关闭了 SPEC-015/SPEC-041 列为未覆盖的"帧字节复制"边界**，并把切片 C（#PF）+ D（验证式帧恢复）+ E（自建映射/TLB）串成端到端写时复制。`CowWitness::evidence_ok` 纯判定 + 9 条 host 单测钉死正/反例；xtask 7 条反例。总验收 2026-09-12：`cargo test --workspace` 357/357（cosir 145、cosir_state 40、lean_backtrace 2、kernel 109、xtask 61）、clippy `-D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2 六条 PASS 退出 0。**边界**：ring0-only、单核、非抢占、单页单任务；COW 动作自身若故障会产生嵌套 #PF（未处理，已知边界）；不实现 ring3 用户态 COW 协议、多页/多任务 COW、Linux trace/`COMPAT-TEST`、`KERNEL-TRACE`/`REFINEMENT-PROOF` | S1-D4 |
| SPEC-045 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/gdt.rs`（DPL3 用户 code/data 描述子、`USER_CS/USER_DS`）、`kernel/src/vspace.rs`（`PT_USER`、`pte_flags(writable,user)`、中间层 U/S）、`kernel/src/user.rs`（`user_code`/`prepare`/`enter`/`UserWitness`）、`kernel/src/idt.rs`（双路径 `page_fault_entry`、`set_handler_dpl3`、`set_user_trap_handler`）、`kernel/src/cow.rs`（唯一分配器 + 用户 COW 区域）、`kernel/src/protocol.rs`、`xtask/src/protocol.rs`（`UserCowReady`/`validate_user_cow_data`/14 事件序列） | 2026-09-12 | 切片 G：**ring3 用户态 #PF 递送 + 用户态 COW**。GDT 新增 DPL3 用户 code/data 描述子（槽 5/6，选择子 `0x2b`/`0x33`，`lgdt` limit 扩至覆盖）；PTE 增 U/S 位且**中间层同样置 U/S**（缺则 CPL3 穿越中间层保护性 #PF，取指时表现为 `P=1,US=1,ID=1`——本轮实测定位的第一个缺陷）。`user.rs` 手写 37 字节用户代码（`mov rdx,X; mov rax,VAL; mov [rdx],rax`（3 字节 store）`; mov rcx,Y; mov [rcx],rax; int3`），`enter()` 复用切片 D 的 5 元 `iretq` 跳板**真正进入 CPL3**。`page_fault_entry` 增双路径：经 `[rsp+0x88] & 3` 判定 CPL3（硬件多压 2 个 qword：`rsp/ss`），保存用户返回现场后调分派器、恢复 15 GPR、重建 5 元 iretq 帧回 CPL3 重试。**第二个实测缺陷：CPL3 的 `int3` 需 DPL3 门**（软件中断检查门 DPL），否则 `#GP(0x1a)`→双重→三故障；改为 `set_handler_dpl3`。真实 QEMU 实测：用户态写只读用户页 → `err=0x7`（P|W|US，**US 位为 CPL3 的硬证据**）→ 内核 COW 逐 qword 复制 + 换映射（保留 U/S）+ `invlpg` → 重试 store 成功 → 用户代码写回执页 → `int3` 陷入内核。`user_cow_ready`：`va=0xa0000002000;err=0x7;class=write_protection;shared0=0xc0ffee0000000000`（原共享帧未改）`;private0=0x5a5a1234deadbeef`（重试成功）`;private1=0xc0ffee0000000001`（整页已复制）`;receipt=0x5a5a1234deadbeef`（CPL3 继续执行）。`UserWitness::evidence_ok` 纯判定 + 7 条 host 单测；xtask 5 条反例。总验收 2026-09-12：`cargo test --workspace` 375/375（cosir 145、cosir_state 40、lean_backtrace 2、kernel 122、xtask 66）、clippy `-D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2 六条 PASS 退出 0。**边界**：单核、非抢占、最小单任务；不实现完整 syscall ABI、不做多页/多任务用户态 COW 协议、不做 Linux trace/`COMPAT-TEST`、不做 `KERNEL-TRACE`/`REFINEMENT-PROOF`；不替换 bootloader CR3 | S1-D4 |
| SPEC-046 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/syscall.rs`（`SYS_ECHO`/`SYS_BAD`/`ECHO_MASK`/`ENOSYS`/`echo_expected`/`dispatch`/`handle`/`SyscallWitness`）、`kernel/src/idt.rs`（`SYSCALL_VECTOR=0x80`、`set_handler_addr_dpl3`、naked `syscall_entry`）、`kernel/src/user.rs`（用户代码扩至 79 字节、两次 `int 0x80`）、`kernel/src/protocol.rs`（`syscall_ready`）、`kernel/src/resume.rs`、`xtask/src/protocol.rs`（`SyscallReady`/`validate_syscall_data`/15 事件序列） | 2026-09-13 | 切片 H：**最小 syscall ABI（OS 中立递送，对齐阶段 S6-D2）**。选择 `int 0x80` + DPL3 中断门而非 `syscall`/`sysret` MSR 快路径：复用既有 IDT/`iretq` 与 TSS.rsp0 换栈机制。新增 `SYSCALL_VECTOR=0x80`，`Idt::set_handler_addr_dpl3` 装入裸入口 `syscall_entry`（无 error code 的软件中断：压 15 GPR → 从 `[rsp+0x00/0x28/0x20/0x80]` 取号/arg0/arg1/CS → SysV 对齐 `call syscall::handle` → 返回值写回保存的 `rax` 槽 → 弹全部 GPR → `iretq` 依目标 CS 弹 3/5 元帧回 CPL3）。`syscall.rs` 提供纯分派器 `dispatch(num,arg0,arg1)`：合法号 `SYS_ECHO=1` 返回 `arg0 ^ ECHO_MASK`（内核侧常量），其余号确定性返回 `ENOSYS=-38`；`handle` 记录每次进入的号/参数/CS/返回值（前 2 次）。用户代码扩为 79 字节：COW store 成功后写回执 0，再 `int 0x80` 合法回显（`rdi=arg`）把返回值写 `Y+8`，再 `int 0x80` 非法号 `0xdead` 把 `-ENOSYS` 写 `Y+16`，最后 `int3`。**真实 QEMU 实测 `syscall_ready`：`calls=2;cs=0x33;num=0x1;arg=0x123456789abcdef;ret=0xfedc45677654cdef;bad_cs=0x33;bad_num=0xdead;bad_ret=0xffffffffffffffda;receipt0=0x5a5a1234deadbeef;receipt1=0xfedc45677654cdef;receipt2=0xffffffffffffffda`**（`cs=0x33` 两次均为 CPL3 硬证据；`ret` 与 `receipt1` 一致证明返回值经真实 CPL3→CPL0→CPL3 往返；非法号得到确定性 `-ENOSYS` 且用户继续执行）。`SyscallWitness::evidence_ok` 纯判定 + 13 条 host 单测；xtask 7 条反例。总验收 2026-09-13：`cargo test --workspace` 395/395（cosir 145、cosir_state 40、lean_backtrace 2、kernel 135、xtask 73）、clippy `--workspace -- -D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2、run=2/2 六条 PASS 退出 0。**边界**：这是 syscall **递送**（号/参数/返回值/特权级往返 + 确定性拒绝），不是抽象模型里带 capability/订阅授权语义的 `syscall` 原语；不做完整 syscall 表、`syscall`/`sysret` 快路径、多任务用户态协议、Linux trace/`COMPAT-TEST` 或 `KERNEL-TRACE`/`REFINEMENT-PROOF`；单核、非抢占、最小单任务 | S1-D4 |
| SPEC-047 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/syscall.rs`（`MAX_ARGS=6`/`SYS_SUM4`/`SYS_COPY_IN`/`Errno`/`encode_errno`/`errno_name`/`dispatch_plain`/`copy_in_bounds`/`copy_in`/`handle(frame)`/`SyscallWitness`）、`kernel/src/idt.rs`（`syscall_entry` 组装 8-qword ABI 现场帧）、`kernel/src/cow.rs`（`root_and_offset`）、`kernel/src/user.rs`（用户代码扩至 223 字节、6 次 `int 0x80`）、`kernel/src/protocol.rs`、`xtask/src/protocol.rs`（`validate_syscall_data` 16 字段） | 2026-09-13 | 切片 I：**syscall ABI v1 调用/返回协议**（POSIR/COSIR 边界清单 §18.0 #3 的 COSIR 细化）。冻结约定：调用号在 `rax`（`0` 保留 → `EINVAL`，未登记 → `ENOSYS`）、参数寄存器 `rdi,rsi,rdx,r10,r8,r9`（6 个）、返回在 `rax`、错误码为负 errno 二补码（`EINVAL=-22`/`EFAULT=-14`/`ENOSYS=-38`）。裸入口改为在栈上组装 8-qword 现场帧 `[num,arg0..arg5,cs]` 并把帧指针传给 `handle`（`r10` 作暂存，原值已在保存块中）。新增 `SYS_SUM4`（4 参数求和，证明多寄存器传递）与 `SYS_COPY_IN`（`(user_ptr,len)`：`len∈[1,8]` 且不跨页，经 `cow::root_and_offset` + `vspace::read_leaf` 要求页 present+user 后读入组装；坏指针 → `EFAULT`，`len` 越界 → `EINVAL`）。用户程序 223 字节，6 次 `int 0x80`：ECHO、SUM4、COPY_IN(X,8)、COPY_IN(坏指针)、保留号 0、未登记号，回执写 `Y+8..Y+48`。**真实 QEMU 实测 `syscall_ready`：`calls=6;cs=0x33;cs_all=1;echo_ret=0xfedc45677654cdef;sum_ret=0x3f3f3f3f3f3f3f3f;copy_ret=0x5a5a1234deadbeef;fault_ret=0xfffffffffffffff2;invalid_ret=0xffffffffffffffea;bad_ret=0xffffffffffffffda;receipt0..6` 与对应返回值逐一相等**。`cs_all=1` 证明 6 次进入全部来自 CPL3；`copy_ret=0x5a5a1234deadbeef` 是 **COW 重试写过的私有页**内容（而非原共享 `pattern(0)`），证明 `COPY_IN` 读的是 COW 后的用户私有映射；三类 errno 编码均按约定返回且被用户写回。`SyscallWitness::evidence_ok` 纯判定 + 14 条 host 单测；xtask 10 条反例。总验收 2026-09-13：`cargo test --workspace` 401/401（cosir 145、cosir_state 40、lean_backtrace 2、kernel 139、xtask 75）、clippy `--workspace -- -D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2、run=2/2 六条 PASS 退出 0。**边界**：这是 syscall **递送协议**，不是抽象模型里带 capability/订阅授权语义的 `syscall` 原语；`COPY_IN` 仅单页最多 8 字节、不写用户内存、不模拟完整 `read` 语义；不做 `syscall`/`sysret` MSR 快路径、多任务用户态协议、Linux trace/`COMPAT-TEST` 或 `KERNEL-TRACE`/`REFINEMENT-PROOF`；单核、非抢占、最小单任务 | S1-D4 |
| SPEC-048 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/msr.rs`（`rdmsr`/`wrmsr`/`IA32_EFER`/`IA32_STAR`/`IA32_LSTAR`/`IA32_FMASK`/`efer_with_sce`/`star_value`/`STAR_VALUE`/`SYSCALL_FMASK`）、`kernel/src/idt.rs`（`SYSCALL_STACK`/`USER_RSP`/naked `syscall_fast_entry`）、`kernel/src/syscall.rs`（`serve`/`handle_fast`/`configure_fast_path`/`FAST_ARG`/`FastWitness`）、`kernel/src/main.rs`、`kernel/src/resume.rs`、`kernel/src/protocol.rs`、`xtask/src/protocol.rs`（`SyscallFastReady`/`validate_syscall_fast_data`/16 事件序列） | 2026-09-13 | 切片 J：**x86-64 `syscall`/`sysret` 快路径**。新增 MSR 细化模块：置 `EFER.SCE`、装 `IA32_STAR=0x0020_0008_0000_0000`（`[47:32]=0x8` 内核 `CS`/`SS=0x10`，`[63:48]=0x20` 用户基址 ⇒ `sysret` 得 `CS=0x33`/`SS=0x2b`，与既有 GDT 用户段吻合；host 单测钉住选择子解码）、`IA32_LSTAR=syscall_fast_entry`、`IA32_FMASK=0x700`（屏蔽 IF/TF/DF）。`syscall` 不换栈不压帧，故裸入口自行把用户 `rsp` 存入 `USER_RSP` 静态、切到专用 16 KiB `SYSCALL_STACK`（单核，无需 `swapgs`/per-CPU）；15 个 GPR 的保存块同时保存了 `syscall` 写入的 `rcx`（用户返回 RIP）与 `r11`（用户 RFLAGS），据此 `sysretq` 回 CPL3。快路径与 `int 0x80` **共用 [`serve`]** 与同一 ABI v1 约定（号 `rax`、参数 `rdi,rsi,rdx,r10,r8,r9`、返回 `rax`、负 errno），见证记录独立于 `int 0x80` 计数器，故 SPEC-047 的 `syscall_ready` 证据不受影响。用户程序在 `int 0x80` 电池之后执行一次 `syscall`（`SYS_ECHO(FAST_ARG)`），因 `syscall` 破坏 `rcx` 而重载回执基址后写 `Y+56`。**真实 QEMU 实测 `syscall_fast_ready`：`cs=0x33;num=0x1;arg=0xfedcba987654321;ret=0xf012cba9789a4321;receipt=0xf012cba9789a4321;efer_sce=1;star=0x20000800000000;fmask=0x700;lstar_ok=1`**（`ret==receipt` 证明真实 `syscall`→内核→`sysretq` 往返；MSR 四项实测值全部符合约定）。`FastWitness::evidence_ok` 纯判定 + 9 条 host 单测；`msr` 4 条 host 单测；xtask 6 条反例。总验收 2026-09-13：`cargo test --workspace` 417/417（cosir 145、cosir_state 40、lean_backtrace 2、kernel 149、xtask 81）、clippy `--workspace -- -D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2、run=2/2 六条 PASS 退出 0。**边界**：单核、非抢占，未做 `swapgs`/per-CPU 区、PCID 或用户态中断；FMASK 仅 IF/TF/DF；快路径仍无 capability/订阅授权语义；不做多任务用户态协议、Linux trace/`COMPAT-TEST` 或 `KERNEL-TRACE`/`REFINEMENT-PROOF` | S1-D4 |
| SPEC-049 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/bootstrap.rs`（`AuthError`/`BootstrapState::authorize`/全局 `install`/`authorize`）、`kernel/src/syscall.rs`（`SYS_AUTHORIZE`/`RIGHT_SEGMAPR`/`RIGHT_SEGMAPW`/`AUTHORIZE_*`/`UNSET_CAP`/`Errno::{NoEnt,Access}`/`right_name`/`auth_errno`/`authorize_call`/`serve` 分派/`MAX_CALLS=9`/`AuthWitness`）、`kernel/src/user.rs`（用户代码扩至 357 字节、3 次授权查询）、`kernel/src/main.rs`（`install`）、`kernel/src/resume.rs`、`kernel/src/protocol.rs`、`xtask/src/protocol.rs`（`SyscallAuthReady`/`validate_syscall_auth_data`/17 事件序列） | 2026-09-13 | 切片 K：**capability 授权语义接入 syscall**。`BootstrapState::authorize(caller, cap_id, right)` 在真实初始/派生 Capability 与位置表上查询：要求位置 active、`HeldByTask(caller)`、且持有者权限串含所请求权限；失败区分 `NoSuchCapability`/`Inactive`/`NotHolder`/`MissingRight`。`main.rs` 在建表后 `install(state)` 到全局槽，syscall 层 `bootstrap::authorize` 据此判定。新增 `SYS_AUTHORIZE(cap_id,right_code)`：权限经 `right_name` 纯映射（`SegMapR`/`SegMapW`），结果 errno 经 `auth_errno` 纯映射（`NoSuchCapability`/`NotInstalled`→`ENOENT=-2`，其余→`EACCES=-13`）。**调用者取 Bootstrap init Task（id=1）——当前唯一 ring3 任务，属单任务近似**。用户程序三次查询：`(image_cap=1, SegMapR)` 授予（返回 cap id `0x1`）、`(1, SegMapW)` 缺权限拒绝（`0xfffffffffffffff3`）、`(999, SegMapR)` 无此 Capability（`0xfffffffffffffffe`）。**真实 QEMU 实测 `syscall_auth_ready`：`cs_all=1;granted_ret=0x1;denied_ret=0xfffffffffffffff3;unknown_ret=0xfffffffffffffffe;receipt_granted=0x1;receipt_denied=0xfffffffffffffff3;receipt_unknown=0xfffffffffffffffe`**（全部来自 CPL3，三类结果均符合授权表事实且被用户写回）。`syscall_ready` 的 `calls` 随之内 ABI 电池 6 次 + 授权 3 次更新为 `9`（ABI 字段与快路径见证不变）。`BootstrapState::authorize` host 单测 5 条（授予/缺权限/未知/非持有者/失效）；`AuthWitness::evidence_ok` 纯判定 + 5 条 host 单测；xtask 5 条反例。总验收 2026-09-13：`cargo test --workspace` 434/434（cosir 145、cosir_state 40、lean_backtrace 2、kernel 161、xtask 86）、clippy `--workspace -- -D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2、run=2/2 六条 PASS 退出 0。**边界**：授权查询只读、不产生 Audit 事件、不改授权表；caller 为单任务近似而非按实际 Task 对象；不是抽象模型带订阅/pending 的完整 `syscall` 原语；不校验 IPC 端点或对象级逐操作权限；不做多任务用户态协议、Linux trace/`COMPAT-TEST` 或 `KERNEL-TRACE`/`REFINEMENT-PROOF` | S1-D4、S1-D5、S1-X2 |
| SPEC-050 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/bootstrap.rs`（`ExcKind`/`ExcError`/`ExcSource`/`ExcSourceTable`/`ExcBootRecord`/`ExcAuditCounts`/`exc_source_create`/`exc_subscribe`/`exc_unsubscribe`/`exc_source_add_subscriber`/`exc_raise`/`platform_init`/`exc_audit_counts`/全局 `platform_init_exception_sources`/`exc_subscribe`/`exc_unsubscribe`/`exc_audit_counts`/`exc_boot_record` + `AuditEvent::{ExcSourceCreated,ExcSubscribed,ExcUnsubscribed,ExceptionRaised}`）、`kernel/src/syscall.rs`（`SYS_EXC_SUBSCRIBE=5`/`SYS_EXC_UNSUBSCRIBE=6`/`EXC_*` 镜像常量/`kind_of`/`Errno::NoSpace`/`exc_errno`/`exc_subscribe_call`/`exc_unsubscribe_call`/`serve` 分派/`MAX_CALLS=15`/`ExcWitness`/`PendingWitness`）、`kernel/src/user.rs`（用户代码扩至 526 字节、6 次订阅 syscall、回执 `Y+88..Y+128`，其中 `[rcx+128]` 用 disp32）、`kernel/src/main.rs`（`platform_init_exception_sources`）、`kernel/src/resume.rs`、`kernel/src/protocol.rs`（`exc_ready`/`exc_pending_ready`）、`xtask/src/protocol.rs`（`ExcReady`/`ExcPendingReady`/`validate_exc_data`/`validate_exc_pending_data`/19 事件序列） | 2026-09-13 | 切片 L：**异常订阅生命周期 + pending 递送 + Audit 接入 syscall**（COSIR §12.4/§12.5/§12.6/§13 的最小细化）。`platform_init` 在进入 CPL3 前显式创建两个预定义源（`page_fault_source` kind=`PageFault`、`syscall_trap_source` kind=`SyscallTrap`），各铸造一个 holder=init、rights=`Derive,ExcSubscribe,ExcSourceDestroy` 的根 Capability（id 64/65），并追加 `ExcSourceCreated` Audit。**异常源 Capability 的元数据内联在源记录里、不进入 `positions`**，`validate_authority` 保持 `positions.count == 3 + derivations.count` 不变量不变，另加独立循环做跨表 id 唯一/holder 存活/subscriber 存活校验；`authorize` 增 exc-source 分支。pending 见证不靠用户在 syscall 内自触发：平台初始化建 Task 2、内核侧登记其订阅、`block_recv(2)` 使其 BlockedRecv，`exc_raise(page_fault_source, PageFault)` 按 §12.5 对 BlockedRecv 订阅者直投——`set_pending` 写入 pending 并置 Ready、追加 `MessageSent`+`MessageReceived`，随后取回消息留证 `ExcBootRecord`；非 BlockedRecv 订阅者则入 IPC 队列（host 单测覆盖两条路径）。`exc_raise` 在任何状态修改前预检 Audit 与队列容量。CPL3 侧新增 `SYS_EXC_SUBSCRIBE(cap,kind)`/`SYS_EXC_UNSUBSCRIBE(cap)`：前者返回 source_id，重复订阅→`EACCES`、kind 不符→`EINVAL`；后者返回 source_id，未订阅→`EACCES`；ExcError→errno 经 `exc_errno` 纯映射（新增 `ENOSPC=-28`）。用户程序 6 次 `int 0x80` 回执写 `Y+88..Y+128`。**真实 QEMU 实测 `exc_ready`：`cs_all=1;sub_ret=0x1;dup_ret=0xfffffffffffffff3;kind_ret=0xffffffffffffffea;sub2_ret=0x2;unsub_ret=0x1;unsub2_ret=0xfffffffffffffff3;receipt_sub=0x1;receipt_dup=0xfffffffffffffff3;receipt_kind=0xffffffffffffffea;receipt_sub2=0x2;receipt_unsub=0x1;receipt_unsub2=0xfffffffffffffff3;audit_created=2;audit_subscribed=3;audit_unsubscribed=1`**（全部来自 CPL3；三类失败均为约定 errno 且被用户写回；Audit 计数恰为成功状态修改次数）；**`exc_pending_ready`：`delivered=1;pending=1;message_id=1;payload=0x10001;task_ready_after=1;audit_raised=1`**（真实 pending 路径被走到：BlockedRecv→Ready，payload 为 source_id=1|kind(1)<<16 的编码，`ExceptionRaised` Audit 已追加）。首次 QEMU 实测还暴露一个真实缺陷：`[rcx+128]` 的 disp8 编码 `0x80` 被符号扩展为 `-128`，导致第 6 个回执写错地址——改为 disp32 后修复。`syscall_ready.calls` 随之更新为 `15`（6 ABI + 3 授权 + 6 订阅）。`BootstrapState` 订阅/取消/递送 host 单测 8 条（含重复/kind/非持有者/缺权限/失效/直投 pending/入队/源 cap 冲突）；`ExcWitness`/`PendingWitness::evidence_ok` 纯判定 + 12 条 host 单测；xtask 新增 8 条反例。总验收 2026-09-13：`cargo test --workspace` 462/462（cosir 145、cosir_state 40、lean_backtrace 2、kernel 181、xtask 94）、clippy `--workspace -- -D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2、run=2/2 六条 PASS 退出 0。**边界**：仅两个单 kind 预定义源；无 `exc_source_destroy`、无订阅撤销/源销毁、无 fan-out 多订阅者规模测试；`raise_exception` 仅由平台初始化触发一次，未做"所有 syscall 都路由 SyscallTrap"的完整细化，也未暴露用户可调 raise/wait（`SYS_EXC_WAIT` 登记为后续切片）；Task 2 的订阅走内核侧登记（不经理 Capability 门控），ring3 的 subscribe/unsubscribe 才走真实 Capability 门控；Audit 无 `EventId`/`Instant`/`Actor` 结构化字段，不是完整 §13 事件模型；单核、非抢占、最小单任务 | S1-D4、S1-X2 |
| SPEC-051 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/sched.rs`（新：`current`/`init`/`set_frame`/`frame`/`pick_next`/`switch_to`/`resume`）、`kernel/src/lib.rs`、`kernel/src/idt.rs`（`syscall_entry` 捕获保存块基址到 `CONTEXT_BASE`）、`kernel/src/syscall.rs`（`SYS_TASK_ID=7`/`SYS_YIELD=8`/`CONTEXT_BASE`/`frame_from_save_block`/`yield_call`/`mt` 记录/`MtWitness`/`handle` 按身份门控/授权与订阅改用 `sched::current()`）、`kernel/src/user.rs`（`USER2_*`/`task2_code`/`prepare_task2`/`task1_frame`/`task2_frame`/`enter` 经 sched；task1 代码扩至 583 字节）、`kernel/src/bootstrap.rs`（`yield_task_global`/`schedule_task_global`/`task_status`）、`kernel/src/main.rs`、`kernel/src/protocol.rs`（`multitask_ready`）、`kernel/src/resume.rs`、`xtask/src/protocol.rs`（`MultitaskReady`/`validate_multitask_data`/20 事件序列） | 2026-09-13 | 切片 M：**双 ring3 任务协作调度 + 真实 Task 身份绑定**（关闭 SPEC-049/050 的"调用者恒为 init Task"单任务近似）。新增 `sched` 仅持当前 id 与每任务完整 `SretFrame`；任务状态仍由真实 `BootstrapState::tasks`（`TaskTable`）管理，`SYS_YIELD` 先 `yield_task(cur)`（Running→Ready）再 `schedule_task(next)`（Ready→Running），两者成功后才更新 id 并恢复——失败即 `panic`，保证 `CURRENT` 与 TaskTable 的 Running 一致。切换机制**复用既有全上下文跳板 `exc::resume`**：`syscall_entry` 在 `mov rbp,rsp` 后把保存块基址写入 `CONTEXT_BASE`；`SYS_YIELD` 经 **逐槽按名映射**（保存块 `r8..r15` 在 `0x38..0x70`、硬件帧 `rip/cs/rflags/rsp/ss` 在 `0x78..0x98`，与 `SretFrame` 字段偏移不同）构造完整上下文存 TCB，再把下一任务的 `SretFrame` 交给 `exc_resume_entry` iretq 进去；被让出的 `syscall_entry` 栈帧有意放弃（下一次 CPL3→CPL0 硬件按 `TSS.rsp0` 重置 `rsp`，不泄漏）。task 2 置于**独立 PML4 槽 21**（首个实测缺陷：放在槽 20 会与 task 1 共享 `slot_free` 只按 PML4 present 判定的粒度而误报 `SlotOccupied`）。`handle` 只把 task 1 阶段一前 15 次进入记入 `CALLS`（`COUNT` 不越 15），其余记入 `mt`，故 SPEC-047/049/050 证据逐字节不变。用户程序：task 1 在原 15 次调用后追加 `SYS_TASK_ID`→`SYS_YIELD`→`SYS_TASK_ID`→`int3`（回执 `Y+136/144/152`）；task 2 为 `SYS_TASK_ID`→`SYS_AUTHORIZE(image cap)`→`SYS_EXC_SUBSCRIBE(page_fault cap)`→`SYS_YIELD`（回执 `Y2+0/8/16`；其 authorize/subscribe 因 cap 属 task 1 而成非持有者）。**真实 QEMU 实测 `multitask_ready`：`cs1_before=0x33;cs1_after=0x33;cs2=0x33;task1_before=0x1;task1_after=0x1;task2=0x2;switches=0x2;task2_auth=0xfffffffffffffff3;task2_sub=0xfffffffffffffff3;receipt1_before=0x1;receipt1_yield=0x0;receipt1_after=0x1;receipt2_id=0x2;receipt2_auth=0xfffffffffffffff3;receipt2_sub=0xfffffffffffffff3`**（三次进入全为 CPL3；task 1 让出前后 id 均为 1、task 2 为 2；两次切换且最后一次切回 task 1；task 2 的授权/订阅均因非持有者得 `EACCES`；全部回执与返回值一致）。`sched` host 单测 1 条、`frame_from_save_block` 逐槽映射 host 单测 1 条、`MtWitness` 纯判定 + 5 条 host 单测、`task2_code` 逐字节 1 条；xtask 新增 6 条反例。总验收 2026-09-13：`cargo test --workspace` 477/477（cosir 145、cosir_state 40、lean_backtrace 2、kernel 190、xtask 100）、clippy `--workspace -- -D warnings` 零告警、`cargo fmt --all --check` 无差异、`cargo xtask test` QEMU BIOS run=1/2、run=2/2 六条 PASS 退出 0。**边界**：单核、非抢占、无中断（IF=0）、无时钟；仅 `SYS_YIELD` 协作切换；共享地址空间与单一 `TSS.rsp0` 内核栈，非 per-task CR3/内核栈，未做 `swapgs`/per-CPU；无 `SYS_EXIT`/任务销毁回收（task 2 以 yield 结束不再运行）；`syscall`/`sysret` 快路径不参与切换；授权/订阅已绑定真实 Task id，但仍不覆盖对象级逐操作权限或 Audit 的 Actor 字段 | S1-D4、S1-X2 |
| TEST-004 | 阶段一命令验收 | `cargo fmt --all --check`、`cargo test -p cosir`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo test --workspace`、`cargo xtask test` | 2026-08-13 | 在 `60a61bd0e01d568541c43edf557ac12b341e0a1f`，cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29) 上全部退出码 0：fmt 无差异；cosir 69/69；workspace cosir 69/69 + xtask 23/23（92/92）；clippy 无 warnings；QEMU BIOS run=1/2 输出六条 PASS。直接支持 host `MODEL-TEST`、lint 与既有 QEMU 启动闭环无回归；不支持真实 CR3/PTE 细化、非叶权限/页表页物理身份、TLB、多核、真实硬件原子更新窗口、机器证明或阶段一退出条件已满足。 | S0-D4、S1-D1、S1-D2、S1-X1 |
| TEST-005 | 阶段二 exc_resume 独立验收 | `cargo fmt --all --check`、`cargo test -p cosir`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo test --workspace`、`cargo xtask test`、独立 verification | 2026-08-13 | 验收 HEAD `1f682db600947f49975eca91e045cae222bb8089`；cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29)、rustup 1.29.0 (28d1352db 2026-03-05)。fmt 无差异；cosir 100/100；workspace cosir 100/100 + xtask 23/23（123/123）；clippy 无 warnings；xtask 六条 PASS；独立 verification verdict PASS（无 Critical/Important）。仅支持 host `MODEL-TEST`、workspace regression/lint 与既有 QEMU 启动闭环无回归；xtask 不证明真实 trap/`iretq`。不证明 `KERNEL-TRACE`、`REFINEMENT-PROOF` 或机器证明，也不表示阶段退出条件整体满足。 | S0-D1、S0-D2、S1-D1、S1-D2、S1-X1 |
| TEST-006 | 最终整体审查修复验收 | `cargo fmt --all --check`、`cargo test -p cosir`、`cargo test --workspace`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo xtask test`、`git diff --check` | 2026-08-16 | cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29)；fmt 无差异；cosir 180/180；workspace cosir 180/180 + xtask 23/23（203/203）；clippy 零 warnings；QEMU BIOS run=1/2、run=2/2 均通过，均输出六条 PASS；`git diff --check` 无错误。仅支持 host `MODEL-TEST`、workspace regression/lint 与既有 QEMU 启动闭环无回归；不支持 `KERNEL-TRACE`、`REFINEMENT-PROOF`、`COMPAT-TEST`、真实硬件 TLB、帧字节复制或 BLK-001 关闭，且不表示阶段退出条件满足。 | S0-D1、S0-D2、S0-D4、S1-D1、S1-D2、S1-X1 |
| DOC-006 | 架构文档修订 | `Prima OS.md` §6.2/§8.5、`COSIR 原语规范.md` §4.7 | 2026-08-26 | 纯文档变更，无代码/测试：白皮书 §6.2 实现注记补全选择性撤销的完整撤销域定义（撤销对象与域、派生树一致性、与 `cap_drop` 正交、token 中间态原子性、在途 `InMessage`/`InNamespace`/`InTransit` 位置不纳入撤销域），并如实标注当前宿主实现边界（仅 `HeldBy` 根可撤销、固定容量表近似、并发与跨节点传播未覆盖）；§8.5 新增"下一阶段机制能力映射"（归属层约定 + 锚点约定 + 八项能力×驱动×机制层原语×规范锚点×假说×优先级表 + AI 不入机制层警示 + 跨架构反例提醒）；白皮书与 COSIR §4.7 回链同步。该证据只记录设计意图与语义定义，不改变已冻结的 POSIR observable semantics，也不新增机制实现证据 | S0-D1、S0-D3 |

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

截至 2026-09-13，该切片中 **page fault → handler → exc_resume → TLB → COW 已全部落地于 ring0（SPEC-042~044），并以 SPEC-045~047 推进到 ring3**：真实 CPL3 用户任务写只读用户页触发用户态 #PF（`err=0x7`，US 位为 CPL3 硬证据），内核 COW 逐 qword 复制 + 换映射 + `invlpg` 后**回到 CPL3 重试成功**；随后用户代码 6 次 `int 0x80` 走完 syscall ABI v1（多寄存器参数、用户缓冲读入、保留/未登记号与三类 errno）、3 次 `int 0x80` 能力授权查询（SPEC-049：`SegMapR` 授予 / 缺 `SegMapW` 得 `EACCES` / 未知 cap 得 `ENOENT`）、6 次 `int 0x80` 异常订阅/取消订阅（SPEC-050：订阅 `page_fault`/`syscall_trap` 授予、重复订阅与再次取消 `EACCES`、kind 不符 `EINVAL`，并产生四类真实 Audit 事件），再一次 x86-64 `syscall` 指令（SPEC-048 快路径，MSR 实测符合约定）完成往返，全部以 `cs=0x33` 证明来自 CPL3，最终 `int3` 陷入内核。SPEC-050 另以内核侧 BlockedRecv 任务见证 `raise_exception`→`pending_return`→Ready 的真实直投（`payload=0x10001`）；SPEC-051 再把系统推进到**双 ring3 任务协作调度**：task 1 以 `SYS_YIELD` 让出、task 2 运行后让回，两次切换均以完整用户上下文保存/恢复实现，且 task 2 的授权/订阅因真实 Task 身份非持有者而确定拒绝（`EACCES`）。尚余 **用户可调 raise/wait 的完整 `syscall` 原语 / 抢占与 per-task CR3 的完整多任务 / Linux trace / `COMPAT-TEST`**（后者需与选定 Linux 行为做差分，本机尚不具备）。当前证据为最小协作式 ring3 观测，不覆盖抽象模型的 `exc_resume`/`syscall` 授权语义与完整用户态运行环境。

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
| 2026-09-13 | 首个 syscall 递送切片采用 `int 0x80` + DPL3 门，而非 `syscall`/`sysret` MSR 快路径 | 最小切片先冻结“号/参数/返回值/特权级往返 + 确定性拒绝”的 OS 中立递送语义；复用既有 IDT/`iretq`/TSS.rsp0 机制，避免过早引入 EFER/STAR/LSTAR/SFMASK 与 `sysret` canonical 地址约束 | `syscall`/`sysret` 快路径登记为后续切片（Linux 兼容路线可能需要）；本切片不宣称抽象模型的 capability/订阅授权语义 |
| 2026-09-13 | syscall ABI v1 采用 SysV 参数序 `rdi,rsi,rdx,r10,r8,r9` + `rax` 返回 + 负 errno 二补码，保留号 0 → `EINVAL` | 与 Linux/x86-64 约定一致，便于后续 `syscall`/`sysret` 与 Linux trace 复用同一协议；错误码负值编码让“成功值 ≥ 0”成为单一判据 | 后续 `syscall`/`sysret` 切片复用该协议；`COPY_IN` 仅单页 ≤8 字节、不写用户内存，完整 `read`/`write` 语义登记为后续 |
| 2026-09-13 | `syscall` 快路径首版不做 `swapgs`/per-CPU，改用静态 `USER_RSP` + 专用内核栈 | 当前为单核、非抢占、最小单任务；`syscall` 不换栈，单核下静态保存用户 `rsp` 已足够，避免过早引入 GS base/每 CPU 数据结构 | 多核/多任务切片必须改为 per-CPU（`swapgs` + GS 基址）；STAR 段布局 `[47:32]=0x8`/用户基址 `0x20` 与现有 GDT 用户段相邻布局绑定，改 GDT 槽位时需同步 |
| 2026-09-13 | 授权查询首版把唯一 ring3 任务等同 Bootstrap init Task（id=1），且查询只读、不产 Audit | 当前只有最小单任务，尚无 Task 对象与用户态的稳定绑定；先用已有真实授权表见证“授权/拒绝/未知”三类判定，避免为造证据而提前引入多任务 | 多任务用户态协议切片必须改为按实际 Task 身份查询，并补 Authorization/Audit 事件与对象级逐操作权限；`syscall_ready.calls` 语义调整为“ABI 电池 + 授权探测”总次数 |
| 2026-09-13 | 异常源 Capability 元数据内联在源记录、不进入 `positions` 位置表 | `positions.count == INITIAL + derivations` 不变量与 `MAX_CAPABILITY_POSITIONS=11` 是按初始/派生 Capability 计的；把源 cap 塞入会破坏不变量、ids 定长数组越界，且 drop 后 active 推导误判 | `validate_authority` 保持原不变量并另加跨表 id 唯一/holder 存活校验；`authorize` 增 exc-source 分支；后续若统一 Capability 命名空间需重新评估 |
| 2026-09-13 | pending 递送以内核侧 BlockedRecv 任务（Task 2）见证，不暴露用户可调 `SYS_EXC_WAIT` | `raise_exception` 按 §12.5 本就是 KernelActor 行为；单任务在 syscall 内自触发 SyscallTrap 是重言式，无法独立证明 pending 路径 | Task 2 的订阅走内核侧登记（不经理 Capability 门控），ring3 的 subscribe/unsubscribe 才走真实门控；`SYS_EXC_WAIT`/用户可调 raise/wait 登记为后续切片；多任务切片须把订阅绑定到真实 Task 身份 |
| 2026-09-13 | 多任务首版不做内核线程切换，`SYS_YIELD` 复用 `exc::resume` 保存/恢复完整用户上下文，被让出的 `syscall_entry` 栈帧有意放弃 | `exc_resume_entry` 已能加载全部 GPR + 5 元 iret 帧；单核下每次 CPL3→CPL0 硬件都按 `TSS.rsp0` 重置 `rsp`，放弃的栈帧不泄漏；避免为造证据引入 per-task 内核栈/`swapgs` 与内核线程 ABI | 抢占、per-task 内核栈/CR3、多核必须改为真正的线程切换（`swapgs` + 每 CPU/每任务栈 + 时钟中断）；`syscall`/`sysret` 快路径暂不参与切换 |
| 2026-09-13 | Task 2 用户区域置于独立 PML4 槽 21，而非与 task 1 同在槽 20 | `vspace::slot_free` 只检查 PML4 项的 present 位（按 bootloader 占用判定），同槽会让 `prepare_task2` 误报 `SlotOccupied`；首个 QEMU 实测即暴露 | 后续多任务若需密集用户地址空间，应把占用检查细化到叶级 PTE 或引入每任务 CR3；当前每任务独占一个 PML4 槽 |

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
