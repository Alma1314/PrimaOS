# Capability 槽位区间分离（`ISSUE-CAP-002`）

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-12
>
> 对应登记：`Prima OS 实现阶段.md` → `ISSUE-CAP-002`

## 1. 根因

`ns_create`（SPEC-081）把根 Capability 铸在**固定预留区间** `[NS_ROOT_CAP_SLOT_BASE=52, +MAX_NAMESPACE_OBJECTS=8) = 52..59`；而 SPEC-087..093 的 usercap 固定块为 `[USER_IPC_CAP_SLOT_BASE=47, +10) = 47..56`。两者**重叠**于 52..56，令 `ns_create` 实际只剩 57/58/59 三槽——恰够 boot 的 3 次。任何新增 usercap 落入该区间都会使第 3 次 `ns_create` `ENOSPC` 并级联（namespace 见证退化、`audit.used` 掉 12）。SPEC-094 曾以「新槽置于区间外（61）+ 扩 `MAX_CAPABILITY_SLOTS` 64→72」绕过，未重构布局。

## 2. 方案

`NS_ROOT_CAP_SLOT_BASE` 改为**槽位空间顶端对齐**：

```rust
pub const NS_ROOT_CAP_SLOT_BASE: u32 = (MAX_CAPABILITY_SLOTS - MAX_NAMESPACE_OBJECTS) as u32; // = 64
```

区间 = 64..71，与 usercap(47..56)、`UNSET_CAP`(60)、`SERVICE_STACK_CAP_SLOT`(61) 结构上**不相交**。以 `MAX_CAPABILITY_SLOTS - MAX_NAMESPACE_OBJECTS` 表达，使 `MAX_CAPABILITY_SLOTS` 扩容时自动保持顶端对齐且不重叠。

**偏离原登记建议（如实登记）**：登记项建议「一次性平移 SPEC-087..093 的 usercap 槽位」。本实现**不平移任何 usercap**——usercap 句柄内嵌在 ring3 见证机码里（`kernel/src/user.rs` 多处 `*_CAP_HANDLE.encode()` 的 64 位立即数），平移会改动机码字节；移 NS 区间是纯常量改动，回归面最小。原建议中「重登记相关证据」的步骤在本机制下**不必要，且未发生**。

## 3. 证据

- **零事件扰动**：`cargo xtask test` ⇒ `PASS qemu bios run=1`、`PASS qemu bios run=2`、`PASS stage1 boot slice`。事件流按 `xtask` 内的 `VALID` 基线逐条校验，全部匹配 ⇒ **seq 1..42 逐字节不变**。理由：`ns_create` 的槽号**不进任何见证输出**（`namespace_ready` 只输出各计数值与 `create_ok` 布尔），init Namespace 根 cap 仍在槽 2（`NAMESPACE_CAP_HANDLE`，与 `NS_ROOT_CAP_SLOT_BASE` 无关）。
- 门禁：`cargo test -p kernel` ⇒ **367 passed / 0 failed**（含新增守卫单测）。
- 回归守卫：新增 `ns_reserved_range_is_disjoint_from_fixed_cap_slots`——断言 `ns_end ≤ MAX_CAPABILITY_SLOTS`、`usercap_end ≤ ns_start`、`UNSET_CAP < SERVICE_STACK_CAP_SLOT < ns_start`。原缺陷（28 槽区间重叠）在本断言下会失败。

## 4. 残余

- `SERVICE_STACK_CAP_SLOT`(61) 现夹在 `UNSET_CAP`(60) 与 NS 顶端区间(64..71) 之间的空档；其原注释「= `NS_ROOT_CAP_SLOT_BASE` + `MAX_NAMESPACE_OBJECTS` + 1 = 61」已随本修复失效并改写。
- `UNSET_CAP`(60) 为 ring3 内嵌 ABI 哨兵，**未改动**（改它会动 ring3 机码）。
- 未新增/删除任何 syscall 号或事件；阶段 4 交付项与退出条件不因本片勾选。
