# 切片 AL 报告：阶段 3 第三片 —— `audit(cap)` 审计查询（§13.2）

> 文档类型：真实内核实现报告（阶段 3 第三片 / 收口片）
>
> 日期：2026-09-25
>
> 设计：`docs/superpowers/specs/2026-09-25-stage3-audit-trace-design.md`

## 1. 交付物

| 交付 | 内容 |
|---|---|
| `AuditTrace` / `AuditError` | `chain`（根在前的不可变 parent 链）+ `chain_len` + `minted`/`derived`/`moves`/`created`/`destroyed` + `target_live` + `origin_none`；失败域 `InvalidCaller`/`InvalidCap`/`NotHeld` |
| `BootstrapState::audit_trace` | 沿 `CapabilityRecord.parent` 回溯（结构性良基，不会成环）+ 一次审计日志扫描；**前置只有「cap 由 caller 持有」**——目标已销毁不构成失败，`CapTargetDead` 不是失败分支；**纯查询**，全程只读 |
| `ObjectRef::id()` | 供结果 4 按目标对象过滤创建/销毁事件 |
| `AuditEvent::ObjectDestroyed` 补 `segment` 标志 | 与 `ObjectCreated` 对称：`ObjectRef` 的 id 空间**按类型各自独立**，不按「id + 类型」过滤会把跨类型同 id 的销毁混计（见 §3 反例 1） |
| ring3 面 | `SYS_AUDIT = 32`（返回打包摘要：`chain_len \| minted<<8 \| derived<<16 \| moves<<24 \| created<<32 \| destroyed<<40`）；`AuditWitness` + `cache_audit_receipts`（回执必须在 task 1 地址空间下读） |
| 协议 | 新事件 `audit_ready`（seq 28）+ `validate_audit_data` + 5 条反例单测 |
| S3-X4 反例补全 | host 单测 `destroy_timers_of_recipient_only_touches_that_recipient`（Timer 侧按 recipient 过滤，不误伤其他 recipient） |

## 2. 判决性见证（`audit_ready`，seq 28）

```
chain_len=2;derived=1;moves=2;created=1;destroyed=0;stale_ok=1;evidence_ok=1
```

- `chain_len=2`：不可变 parent 链 = image 根 Capability → `ns` 块派生的 `child1`（**S3-X3** 的
  派生链重建）；
- `derived=1`：链上恰有一条 `CapabilityDerived`（目标自身）；
- `moves=2`：`ns_bind` + `ns_unbind` 两次 `CapabilityMoved`——**位置历史完整可重建**；
- `created=1 / destroyed=0`：目标 image Segment 的创建事件，未销毁；
- `stale_ok=1`：对**目标已销毁**的 Capability（`ns_destroy` 后的 `c_copy`）查询仍返回非负
  （§13.2 明示不构成失败）。

**纯查询的独立证据**：`state_audit_ready.used` 在切片 AK→AL 之间**保持 175 不变**——
ring3 的两次 `SYS_AUDIT` 没有追加任何审计事件（若有，"重建证据"就会被自己的查询污染）。

## 3. 实施中的失败与反例（全部如实保留）

1. **`created` 跨类型 id 混计（由 host 单测暴露）**：初版按 `object_id == target.id()` 过滤，
   于是 image `Segment(1)` 把 `Namespace(1)`（init Namespace 的 `ObjectCreated`）也算了进去——
   `created` 实测 2 而非 1。根因是 `ObjectCreated`/`ObjectDestroyed` 只带 `object_id` 而不带类型，
   而 `ObjectRef` 的 id 空间按类型各自独立。修正：给 `ObjectDestroyed` 补 `segment` 标志（与
   `ObjectCreated` 已有的 `segment` 对称），过滤改为「id + 类型」。**未改变 QEMU 协议事件格式**
   （`AuditEvent` 是内核内部事件），故既有 37 条事件零影响。
2. **路径文本改放 Y 页失败，回退代码页（QEMU 见证暴露）**：切片 AL 的 ring3 块用尽了 4 KiB
   代码页，故把 `SYS_NS_*` 的路径文本从代码页移到 Y 页（由 `prepare` 预置）。结果是
   `namespace_ready` 的 `evidence_ok` 掉到 0、`binds`/`mounts`/`unmounts`/`invalid_path_rejected`
   全部错位——直接 dump Y 页前 32 字节得 `0,0,0,<非零>`，即**路径文本没有落在那段内存上**。
   修正：**回退**到已由前两片验证过的「代码页高偏移静态数据」方案，并把 audit 块从约 200 字节
   精简到 8 条指令（复用 `ns` 块已派生的 `child1`，它已具备完整位置历史），腾出足够空间。
   **教训**：`prepare` 阶段的 Y 页写入与后续映射/用户写入的交互不可靠，登记为不可用方案。
3. **代码页用尽，`NotHeld` 反例改由 host 单测覆盖**：代码页定死 4096 B（USER_CODE_LEN=4096，
   路径文本落在 4064..4091）后，ring3 已无处安放 `SYS_AUDIT` 的 `NotHeld` 反例。该前置
   （§13.2 唯一前置）由 host 单测 `audit_trace_rejects_capabilities_the_caller_does_not_hold`
   覆盖，并在设计文档 §1.3 显式登记——**不是静默省略**。

## 4. 回归面（同一工作目录、切换提交取基线，逐条 diff）

基线 = 切片 AK 终点的提交，避免独立 worktree 的**构建路径差异**污染镜像布局字段。

**新增 1 条事件**（`audit_ready` 插在 `timer_ready` 之后），故 seq 28 起顺移一位
（`exc_resume_ready` 28→29 … `boot_complete` 37→38）。

**6 条既有事件的数据变化**（根因均为镜像体积/布局）：

| 事件 | 字段 | 旧 → 新 |
|---|---|---|
| `memory_ready` | `usable` / `image` / `kend` | 30454 → 30448 / 1018 → 1024 / 0x13fa000 → 0x1400000 |
| `bootstrap_ready` | `image_len` | 4166088 → 4193696（+27608 B） |
| `bootstrap_ready` | `image_frames` / `free_frames` | 1018 → 1024 / 30454 → 30448 |
| `preempt_ready` | `rsp*`/`kstack*_top` | 后移（`ticks`/`pre*`/`root*`/`cr3*`/`data*`/`mark*` 全不变，`mark1=0x1`） |
| `frames_ready` | `free` / `image` | 30344 → 30338 / 1018 → 1024（`universe`/`segment`/`table` 不变） |
| `self_root_ready` | `pages` | 211 → 213 |

**其余 31 条逐字段不变**——其中两条是**判决性的「不变」**：

- `state_audit_ready.used` 保持 **175**（纯查询不追加事件）；
- `task_ready.caps`/`caps_after` 保持 **43**（audit 块复用 `child1`，不再新铸 Capability）。

**同镜像连跑两次输出逐字节相同**。

## 5. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **356**（351→356）、prima-ostd 21、xtask **193**（188→193） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件·0 命中） |
| `cargo xtask tcb` | `trusted_lines=1217 mechanism_lines=19010 ratio=6.02%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**38 条事件**） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

**G3 基线复测**：占比 6.11% → **6.02%**（机制层 +322 行、可信面未动）。

## 6. 口径偏差登记

1. **结果 5（跨节点代理根）不适用**：本细化层无 `origin` 字段、无 `InTransit` 位置，故查询结果以
   `origin_none = true` 显式声明「链上无代理根」，而不是静默省略。
2. **位置历史是本 cap 的移动事件**：`CapabilityMoved` 与 `CapabilityDropped` 都计入 `moves`
   （§4.6「删除位置时新位置为 `None`」）。
3. **链上事件的归属判据是「事件的 `cap` ∈ chain」**，而不是「事件的 `parent` 匹配」——
   `CapabilityDerived { cap, parent, holder }` 同时带两者，取前者可覆盖「该 Capability 自身的
   铸造/派生事件」并天然包含根。
4. **初始三条根没有铸造事件**：它们由 `InitialCapability` 表示（非运行时铸造），故 `minted`
   在纯初始链上为 0；`bootstrap` 期的对象创建由 `ObjectCreated` 承载。
5. **`ObjectCreated`/`ObjectDestroyed` 的 id 空间按类型独立**，事件靠 `segment` 标志区分
   Segment 与非 Segment；更细的类型区分不在本片范围（登记）。
6. **代码页定死 4096 B**：ring3 见证已用满（路径文本落在 4064..4091），后续切片若需更多用户态
   见证必须扩页或改用新的用户页（本片尝试的 Y 页方案不可靠，见 §3 反例 2）。
7. safe-by-construction ≠ sound：沿用「信任调用方」契约。

## 7. 阶段 3 收口

切片 AJ（Namespace）、AK（Timer）、AL（`audit`）**三片全部落地**：

| 交付项 | 落点 | 退出条件 | 本片/前片证据 |
|---|---|---|---|
| S3-D1 Namespace bind/unbind/mount/unmount/resolve | AJ | S3-X1（解析终止、Mount 图无环） | `namespace_ready`：`cycles_rejected=1`、`acyclic_ok=2` |
| S3-D2 Mount 环检测与 stale Mount 清理 | AJ | — | `ns_resolve` 的 `MountTargetDead` + 单测 |
| S3-D3 绝对单调 Timer | AK | S3-X2（每个 Fired 恰一次通知） | `timer_ready`：`exactly_once_ok=1` |
| S3-D4 `deadline ≤ now` 原子立即触发 | AK | — | `immediate_fired=1` |
| S3-D5 Capability 派生、位置变化与对象生命周期 Audit | AJ/AK/AL | S3-X3（能重建派生链与位置历史） | `audit_ready`：`chain_len=2;derived=1;moves=2` |
| 容器销毁不破坏其他对象生命周期 | AJ/AK/AL | S3-X4 | 四类容器的 host 反例（Task/Namespace/Message/Timer） |

**阶段 3 的交付项与退出条件一律不勾选**——沿用阶段 0/1/2 的组织级独立性门槛，闭合判断交复核者。
