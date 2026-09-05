# 切片 C(续)设计：#PF error-code 捕获与写保护语义分类

> 文档类型：真实内核实现设计（切片 C 后段 — 故障语义分类）
>
> 日期：2026-09-05
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-05）

## 0. 定位与前置

垂直切片固定为 **COW/page fault → handler → `exc_resume` → TLB → Linux trace**。上一提交
`ff66443`（切片 C 第一步，证据 SPEC-040）完成了真实向量 14 #PF 捕获：读 CR2、`translate`
自证探测地址未映射、把返回地址前移 `FAULT_SKIP_LEN` 以`iretq` 返回、上报
`page_fault_ready(addr; not_mapped)`，由 QEMU 断言 `addr==0x40000 ∧ not_mapped==1`。

本设计把该#PF 捕获从"只读 CR2 + 无脑 skip 指令"的**最小捕获**，升级为**对故障做结构化语义
分类**：捕获并译码硬件压栈的 #PF error code，区分"写保护/保护违规(可 COW 处置)"与"真缺页
(P=0)"等，落实切片 C 后半（trap → exc_resume）首先要建立的故障分类前置。

### 0.1 已确认的关键决策

脑暴中与用户逐项收敛的决策（全部已确认）：

1. **不建叶 PTE 全枚举**。bootloader 直映射几乎都用 2MiB huge 页，而现有 `translate`（
   `kernel/src/cpu.rs`）只翻 4KiB、遇 huge 叶返回 `PageNotMapped`；做一张全叶 PTE 逐位索引的
   成本（VA 重建、递归、宽度）高于其对本切片的收益，且并不直接回答"这次故障可否处置"。
   因此不把它作为本 slice 交付。
2. **supervisor 写保护现场的门是 CR0.WP，不是"是否存在 RO 叶"**。CPU 判定
   "supervisor(supervisor CPL)写一个只读页是否 #PF"由 CR0 的 WP(bits 16)总闸决定：WP 清位时
   supervisor 写只读页**不**产生写保护 fault。故本次故障是否可现场触发，取决于**读 CR0.WP**，
   一条 asm，是廉价的先验。
3. **写保护触发为 best-effort**，受 WP 门约束：仅当 WP=1 且选取到一个 `translate` 预确认
   present 的只读 VA 时，才在 QEMU 抓真实现场 P1∧W1 并断言；WP=0 或实际不 fault 时**不强行
   现场触发**，如实收进边界记录（转自建页表/后续 B 承接）。
4. 唯真正交付的新审核能力 = **error-code 捕获 + 纯位译码分类 + CR0.WP 读**——身形小、可控、
   可被 host 单测强钉。

### 0.2 bootloader 交接相关事实

- 故障由 `trigger_page_fault(addr)`（`mov rax,[rax]` REX.W 8B，3 字节）在 ring0 内核发起，故
  现现场 CR2 与 error code 均反映 supervisor 源（U/S=0）。
- 上一提交已打开向量 14 的 IDT 门，handler 为裸 `page_fault_entry`（携带自管现场与 `iretq`）。
- 错误码在入口时位于被 push 6 个 callee-saved 后的 `[rsp+48]`，上一实现直接 `add rsp,8` 弹出
  丢弃——本设计的捕获目标。

## 1. x86-64 #PF error code 语义基线（Intel SDM）

`#PF` 向栈压 32 位(零扩展到 u64) error code，CPU 同时把故障线性地址写入 CR2。低字节位：

| bit | 名称 | 1 值语义 | 0 值语义 |
|---|---|---|---|
| 0 | P | 页存在，fault 由**保护/权限**引起 | 由页不存在（缺页）引起 |
| 1 | W/R | fault 由**写**访问引起 | 由读或取指引起 |
| 2 | U/S | fault 来自用户/低特权(CPL3) | 来自 supervisor(CPL<3) |
| 3 | RSVD | 检测到 PTE 保留字段被置(格式错误) | 无 |
| 4 | I/D | 由取指令引起 | 不是取指 |
| 5 | PK | 保护键违规 | — |

本切片关切的分界：**P=1 且 W/R=1 → "写保护/保护违规"，可 COW 处置路径候选**；**P=0 → 真缺页，
demand-paging / 不可简单由恢复同一页而解**。其余位随后续事件续用。当前只 4KiB 页、无 SMAP；
页大小位不在本 error code（fault 页大小差异表现于超页面不一致）。

### 1.1 分类目标

把一次"某线性地址写操作被拒"归类为一组**互斥 class**，每种配给不同的后续处理倾向：

- `NotPresent`（P=0）：真缺页。
- `ProtectionViolation.Present=Yes`：页存在却被拒——写入只读页(P1W1) 是 COW 切片关心的核心，
  是"写保护处置后恢复同一任务/指令"的心智模型。
- 其余已检测之置位（RSVD/PK/取指等）归 `Other`，本 slice 不展开处置语义。

该分类由纯函数完成，输入是译码的 error code，输出 enum class——**host 全位表单测是本 slice 证据
之主，无关 QEMU 现场是否触发成功**。

## 2. 组件与文件级改动

### 2.1 `kernel/src/fault.rs`（新增：纯译码分类器）

不含任何 asm 的纯逻辑模块，是分类语义的唯一宿主，为此完整可 host 单测：

```rust
/// 一次 #PF 的语义分类（互斥）。由 error code 译码，判定可否简单处置。
pub enum FaultClass {
    /// P=0：页不存在 → 真缺页（demand-paging 路径候选，非 COW 写保护）。
    NotPresent,
    /// P=1 且为写意图：写保护/保护违规 → COW 处置候选。
    WriteProtection,
    /// P=1 且非写、或含 RSVD/PK/取指等其它置位：分类给后续，本 slice 不处置。
    Other,
}

/// 把 #PF error code 收缩为 FaultClass。
/// host 单测对 (P,W,U,S,I,RSVD) 全组合钉分类落点。
pub fn classify(err: u32) -> FaultClass;
```

对应 relationship：
- `P=0`（无论 RW 位）→ `NotPresent`。注意：写一个 absent 页产生的 fault **error code 中 P=0、
  W/R=1**——它仍是"缺页"，不是写保护。这条是宿主单测最该钉住的（防止把"写缺页"误归写保护）。
- `P=1 ∧ write` → `WriteProtection`。
- `P=1 ∧ 非 write`，或 RSVD/PK/I/D 其它置位 → `Other`。

### 2.2 `kernel/src/idt.rs`（error-code 捕获）

上一实现弹掉 error code。改为在 `page_fault_entry` 内、前移返回地址前、`add rsp,8` 弹出前，
先把这个 error code（位于 `[rsp+48]`）存到新的 `static FAULT_ERROR: AtomicU32`，并提供
`pub fn last_fault_err() -> u32`（镜像既有 `fault_addr()`）。正确性：Fault 类异常把 error code
压栈位于被 push 的 6×8=48 字节之后、CPU 压的 rip 之前。

### 2.3 `kernel/src/cpu.rs`（CR0.WP 读门）

```rust
/// 读 CR0 的 WP 位（bits 16）：1 = supervisor 写只读页产生 #PF；0 = 允许。
/// asm `mov d, cr0`，此读取无法 host 纯测，语义在 QEMU 侧观察。
pub fn read_cr0_wp() -> bool;
```

该读门才是"supervisor 写保护现场在本次是否可造"的可证伪开关(§0.1.3)。

### 2.4 `kernel/src/protocol.rs` / `xtask/src/protocol.rs`（事件字段语义化）

把 `page_fault_ready(addr; not_mapped=<0|1>)` 泛化为携带 error code 与分类落点：
`page_fault_ready(addr; err=<hex>; class=<NotPresent|WriteProtection|Other>)`。

QEMU 侧现 trigger 场景：ring0 读 `0x40000`（absent）→ 期望 `class=NotPresent`。xtask 断言译码落点
而非一序 bool，从而"译码正确"成为可跨 boot 验证的不变契约。

### 2.5 `kernel/src/main.rs`（串联 + best-effort 写保护触发）

在既有读 absent 探测后追加一个 **best-effort 写保护触发分支**：

```
let wp = kernel::cpu::read_cr0_wp();
if wp {
    // 取一个 translate 预确认 present 的只读 VA（链接期 rodata 或恒定只读符号），
    // 对该地址作写触发抓现场；若触发成功(error code 呈现) → 收为
    //     class=WriteProtection 的现实现场上报；
    // 否则（实际不 fault / 页竟可写）→ 该支不落 QEMU 断言，如实进边界。
} else {
    // WP=0：supervisor 写只读页不 fault。不能造写保护现场——诚实记录边界（见 §3）。
}
```

注意：写触发**不能**沿用将返回地址前移跳过的机制去"撞穿"只读页却假装写成功——现场不存在时宁可不测。

## 3. 证据边界（不夸大）

正承诺（会成为 SPEC 行的 true 能力）：

- **error-code 捕获**：`page_fault_entry` 实际把每次 #PF 的错误码保存，`last_fault_err()` 可读。
- **纯位译码分类正确**：`classify` 对全 (P,W,U,S,I/D,RSVD) 位组合给出唯一 class，host 单测表钉，
  含"写一个 absent 页 P0W1 → NotPresent 非写保护"这条反混淆。
- **CR0.WP 真实可读**：`read_cr0_wp()` 在 QEMU 上如实返回。

对 `present∧write` 真实 supervisor 写保护现场：**仅当 WP=1 且取得一个 translate 预确认 present
的只读 VA 时才断言抓到**；WP=0 或不存在该 VA／实际不 fault 时**不强行触发、不虚假断言**，把
P1W1 真实现场记为"需真实页操控(自建页表/未来 B)才能呈现"的明确待办，而不是让本 slice 假装达成。

本设计**仍不宣称**：自建页表、改 PTE、把 #PF 喂给 `exc_resume`、ring3/用户故障递送、IRQ/PIC
完整分派、`KERNEL-TRACE`、`REFINEMENT-PROOF`。它们留在切片 C 后半段及更后。

## 4. host 单测面（证据之主）

- `fault.rs`：全位组合→class 枚举表；特别钉 `P0W1→NotPresent`、`P1W1→WriteProtection`、
  `P1W0`(读保护)→`not {WriteProtection}` 等分界；U/S、I/D、RSVD 置位不影响本关切分界或归 Other
  的规则。
- 译码原函数为纯输入/输出，无需真实 CR3。
- `idt.rs` error 捕获 static 读写由其语义 + (如可) 单元级注入测试钉；`read_cr0_wp` 为 asm 直读，
  无 host 纯测试，语义由 QEMU 观察。

## 5. 测试组织与工件

- 新 `kernel/src/fault.rs` 纯分类器 + `#[cfg(test)] mod`。
- idt.rs `FAULT_ERROR` + `last_fault_err()`、cpu.rs `read_cr0_wp`、main.rs 串 best-effort 触发。
- 事件/字段 (protocol 两侧) + xtask 断言更新。
- 跑 `cargo test --workspace`、`clippy -- -D warnings`、`cargo fmt`、`cargo xtask test`(QEMU
  BIOS run)保持既有样板。结果将记入 `Prima OS 实现阶段.md` tracker 新 SPEC 行 + 同步白皮书边界。

## 6. 范围未含（YAGNI / 后续）

剔除的叶 PTE 全枚、改 PTE/自建页、写保护真实现场无法在现有 CR3 呈现时的降级路径、向
`exc_resume` 喂故障的桥接——均不是切片 A 的目标，清单留给后续 slice（自建页表仍无在册字母，
登记为后续切片待办）。
