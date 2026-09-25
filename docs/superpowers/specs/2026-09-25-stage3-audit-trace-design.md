# 切片 AL 设计：阶段 3 第三片 —— `audit(cap)` 审计查询（§13.2）

> 文档类型：真实内核实现设计（阶段 3 第三片 / 收口片）
>
> 日期：2026-09-25
>
> 状态：已获用户确认（2026-09-25：阶段 3 拆 **3 片**；新原语要求 ring3 可见调用与证据）

## 0. 定位与前置

本切片落实 **S3-D5** 的查询面与退出条件 **S3-X3**（Audit 能重建 Capability 派生链与位置历史），
并补齐 **S3-X4** 的四类容器销毁反例（Task / Namespace / Message / Timer）。

规范锚点：§13.2 `audit(cap) -> AuditTrace`（前置、五类结果、纯查询语义）、§13.1 事件模型、
§4.2 `CapRecord`（`parent` 链是**不可变**的）、§4.6 位置变化（每次移动都记旧位置与新位置）。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| `audit(cap)` | §13.2：前置「`cap` 当前由调用者持有」，**目标对象可以已经销毁**；纯查询，不参与权限判定、不改状态 | 不存在 |
| 结果 1/2 | 从根 Capability 到 `cap` 的不可变 `parent` 链，及链上每个 Capability 的铸造或派生事件 | `CapabilityRecord.parent` 已存在（`bootstrap.rs`），但无回溯查询 |
| 结果 3 | `cap` 自创建以来的**全部位置变化** | 审计已有 `CapabilityMoved`（含 `from`/`to`），但无按 cap 的检索 |
| 结果 4 | 目标对象的创建与销毁事件（若存在） | 审计已有 `ObjectCreated`/`ObjectDestroyed` |
| 结果 5 | 若链上存在 `origin ≠ None` 的跨节点代理根，其源节点/源 CapId/传输接收事件 | **本实现无 `origin` 字段、无跨节点传输**（单机细化层）→ 不适用，登记 |

### 0.1 本片的口径（照实登记）

1. **结果 5 不适用**：`CapabilityRecord` 没有 `origin`、也没有 `InTransit` 位置，跨节点代理根在
   本细化层不可表示。查询结果以 `origin_none = true` 显式声明「链上无代理根」，而不是静默省略。
2. **位置历史的范围是「本 cap 自身的移动事件」**：§13.1 的 `CapabilityMoved` 记录
   `from`/`to` 两个位置；本片按 `cap` 句柄过滤它们。**删除位置**（`CapabilityDropped`）也算一次
   位置变化（§4.6「删除位置时新位置为 `None`」），计入 `moves` 并在链上保留。
3. **链上事件的判定用**「事件的 `cap` 落在链上」**而非「事件的 `parent` 匹配」**：因为
   `CapabilityDerived { cap, parent, holder }` 同时带两者，取 `cap ∈ chain` 即可覆盖
   「该 Capability 自身的铸造/派生事件」，且天然包含根（`minted` 与 `derived` 分开计数）。
4. **查询不设容量失败**：链长上限即 [`MAX_CAPABILITY_SLOTS`]（`parent` 链不会成环——
   `CapabilityRecord` 的 `parent` 在创建时固定、只指向更早分配的句柄，是结构性良基）。切片 AJ 的
   `NamespaceAudit`/切片 AK 的 `TimerAudit` 是**汇总**计数；本片的 `AuditTrace` 是**单条 cap 的
   可重建证据**，两者不重叠。
5. **`audit` 是纯查询**（§13.2 明示）：不追加任何审计事件、不改任何状态。

## 1. 设计

### 1.1 结果类型（`kernel/src/bootstrap.rs`）

```rust
pub const MAX_TRACE_CHAIN: usize = 64; // = MAX_CAPABILITY_SLOTS

pub struct AuditTrace {
    /// 从根 Capability 到 `cap` 的不可变 `parent` 链（根在前、`cap` 在末）。
    pub chain: [Handle; MAX_TRACE_CHAIN],
    pub chain_len: usize,
    /// 链上 Capability 的铸造事件数（`parent = None` 者）。
    pub minted: u32,
    /// 链上 Capability 的派生事件数（`parent = Some(_)` 者）。
    pub derived: u32,
    /// `cap` 自创建以来的位置变化次数（`CapabilityMoved` 或 `CapabilityDropped`）。
    pub moves: u32,
    /// 目标对象的创建 / 销毁事件数。
    pub created: u32,
    pub destroyed: u32,
    /// 目标对象是否仍存活（§13.2：已销毁**不**构成失败，只是这里为 `false`）。
    pub target_live: bool,
    /// 链上无跨节点代理根（本细化层恒 `true`，见 §0.1 第 1 条）。
    pub origin_none: bool,
}

pub enum AuditError { InvalidCaller, InvalidCap, NotHeld }
```

### 1.2 查询实现

`BootstrapState::audit_trace(caller, cap) -> Result<AuditTrace, AuditError>`：

1. **前置**：`cap` 是有效句柄（槽 `Free` → `InvalidCap`；`Stale` → `InvalidCap`），且
   `holder_of(cap) == Some(caller)`（否则 `NotHeld`）。**目标是否存活不参与前置**（§13.2）。
2. **链**：从 `cap` 沿 `CapabilityRecord.parent` 回溯到 `parent = None` 的铸造根；初始三条根
   （`InitialCapability`）没有 `CapRecord`，其 `parent` 视为 `None` 且自身即链首。链长受
   `MAX_TRACE_CHAIN` 约束（结构性良基，不会溢出）。
3. **事件扫描**：一次 `audit.for_each`，按三类累积：
   - `CapabilityDerived { cap, parent, .. }`：`cap ∈ chain` 时，`parent == None` 记 `minted`、
     否则记 `derived`；同时 `cap == 查询目标` 时无额外语义（位置由下一类覆盖）。
   - `CapabilityMoved { cap, .. }` / `CapabilityDropped { cap }`：`cap == 查询目标` 时 `moves += 1`。
   - `ObjectCreated { object_id, .. }` / `ObjectDestroyed { object_id }`：`object_id` 等于目标对象
     的 id 时分别累加 `created`/`destroyed`。目标对象的 id 由 `target_of(cap)` 给出；**目标已销毁
     时 `target_of` 仍能回答**（`CapRecord` 不可变，`target` 字段一直在）。
4. **`target_live`**：取 `self.target_live(target)`。

**纯查询**：全程只读，不 `append` 任何事件。

### 1.3 ring3 面

`SYS_AUDIT = 32`：`args[0] = cap`，返回打包的摘要（每字段 8 位，低位在前）：

```
chain_len | minted<<8 | derived<<16 | moves<<24 | created<<32 | destroyed<<40
```

失败为负 errno（`InvalidCap` → `ENOENT`、`NotHeld` → `EACCES`）。选打包返回而非用户缓冲，
是为了让 ring3 见证只需一次 `int 0x80` 与一个回执槽。

ring3 序列（在切片 AK 的 Timer 块之后）：

```
c = derive(image_cap, {Derive, SegMapR})       → 派生 1
bind(init_ns, "audit", c)                      → 位置变化 HeldBy→InNamespace
a1 = SYS_AUDIT(c)                              → chain_len=2, derived≥1, moves=1
c2 = derive(image_cap, {Derive})               → 再派生一个，然后用它做「目标已销毁」的反例
mount(...) / destroy(...)                      → 让某个被查询 cap 的目标变为已销毁
a2 = SYS_AUDIT(c_stale)                        → 目标已销毁仍成功（target_live=0 但返回非负）
SYS_AUDIT(task_cap)                            → NotHeld（EACCES）反例
```

具体序列在实现时定，判据写进 `audit_ready`。

新事件 `audit_ready`（seq 28，紧随 `timer_ready`）：字段 `chain_ok`、`minted_ok`、`derived_ok`、
`moves_ok`、`created_ok`、`destroyed_ok`、`stale_target_ok`、`not_held_rejected`、`purity_ok`
（查询前后审计条数不变——纯查询的判决位）、`evidence_ok`。

### 1.4 S3-X4 的四类容器销毁反例（host 单测）

| 容器 | 反例 |
|---|---|
| Task | `task_destroy` 后：其消息被丢弃、cap 位置被删、Timer 被清理（AK 已做），且**不影响**其他 Task 的 cap/Timer |
| Namespace | `ns_destroy` 后：其他 Namespace 中指向它的挂载项**保留**为 stale Mount（AJ 已做），且不误删其他 ns 的条目 |
| Message | 丢弃某消息后：消息内 Capability 的位置被删（成为无位置 token），但不影响同 cap 在别处的位置（本层 cap 至多一个位置） |
| Timer | `task_destroy` 联动只删该 recipient 的 Timer，**不影响**其他 recipient 的 Timer（AK 已做；本片补一条跨 recipient 反例） |

## 2. 回归面

预期：既有 37 条事件中 `audit_ready` 使 `timer_ready` 及之后 seq 顺移 27→28…37→38；`task_ready.caps`
再增（新派生的 cap）；镜像布局类字段随代码增长变化。**纯查询不追加审计事件**，故
`state_audit_ready.used` 只应因 ring3 序列里的 `derive`/`bind` 而增，不因 `SYS_AUDIT` 而增——
这一点由 `audit_ready.purity_ok` 亲自证明。

## 3. 验证

`cargo test --workspace`、`cargo xtask lint`、`cargo xtask tcb`（复测登记）、`cargo xtask test`
（38 条事件）、`cargo xtask compat`；同目录切换提交的事件流逐条 diff；同镜像连跑两次。

## 4. 边界（不夸大）

1. **不勾选任何阶段 3 交付项或退出条件**——沿用组织级独立性门槛，闭合判断交复核者。
2. 结果 5（跨节点代理根）在本细化层不适用（§0.1 第 1 条）。
3. `audit` 不返回 payload 明文或内容摘要——§13.1 明示 `MessageSent` 只记长度与 Capability 列表，
   本片不越界读取消息内容。
4. safe-by-construction ≠ sound：沿用「信任调用方」契约。
