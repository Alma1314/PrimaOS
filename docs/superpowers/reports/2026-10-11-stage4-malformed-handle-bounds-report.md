# SPEC-089 报告：畸形句柄越界（阶段 4 缺陷修复，`ISSUE-CAP-001`）

> 状态：修复实现 + 反例单测；不勾选任何阶段交付项或退出条件
>
> 来源：SPEC-088 实现过程中实测暴露的、对**不可信 ring3 输入可达**的内核越界 panic

## 1. 本片要回答的问题

`Handle` 只是 `(generation << 32) | index` 的编码，`Handle::decode` 只校验格式（`index`/`generation`
非零）不校验槽索引上界；而 `CapabilityRecordTable` 与 `CapabilityPositionTable` 按槽**直索引**
（`entries[handle.index() as usize]`）。ring3 可构造 `0xffff_ffff_ffff_ffea`（index `0xffffffea`、
generation `0xffffffff`，格式合法）经 syscall 传入；一旦抵到直索引即 `index ≥ MAX_CAPABILITY_SLOTS`
越界 panic。本片判定该缺陷并在**共享边界**一次性消除。

## 2. 根因与修复位置（最小改动、覆盖全部调用方）

同文件的 `CapabilitySlotTable::check`/`free`（`bootstrap.rs:1050`/`1066`）早已做上界校验，**两张
记录/位置表却没有**——这是缺陷本体。故修复落在三处共享边界，而非逐调用方打补丁：

| 位置 | 修复 |
|---|---|
| `CapabilityRecordTable::find/insert/remove` | 越界 → `None` / `Err(MalformedHandle)` / `false`（`find` 改用 `entries.get(..)`） |
| `CapabilityPositionTable::find/find_mut/register_location/remove` | 同上（`MAX_CAPABILITY_POSITIONS` 为上界；`find_mut` 覆盖 `move_to_*`/`drop_from_task` 全部经它路由的路径） |
| `BootstrapState::decode_handle`（syscall/协议入口） | 除格式外**校验槽索引上界**，`index ≥ MAX_CAPABILITY_SLOTS` 即拒为 `AuthError::MalformedHandle` |

信任边界（`decode_handle`）先拦、模型内两张表再兜底，二者共享同一上界常量，不会因未来新增调用方而失守。

## 3. 判定性反例（host 单测，`kernel/src/bootstrap.rs`）

`malformed_out_of_range_handles_fail_safety_not_panic`：构造 `0xffff_ffff_ffff_ffea`，断言
（a）`decode_handle` 返回 `MalformedHandle`；（b）`records.find`/`positions.find`/`holder_of`/
`rights_of`/`target_of` 全部返回 `None`；（c）`authorize` 返回 `NoSuchCapability`；（d）
`positions.remove` 返回 `false`；（e）全程 `state` 逐字段不变（纯查询/拒绝）。

**该测试在修复前以越界 panic 失败**（即 `find` 的直索引）；修复后通过，构成缺陷→修复的判决性见证。

## 4. 回归面（逐条登记）

**QEMU 事件流与 SPEC-088 终点逐字节相同（41 条，零变化）**：所有既有句柄（初始根、异常源、
平台段 cap 等）槽索引均 < 64，走原分支，判据与 errno 不变；被改变的只有"此前会 panic、现安全拒绝"
的畸形输入，而既有事件中无此类输入（否则早已崩溃）。`handle_ready.malformed_rejected` 由
`Handle::decode(999)`（generation=0，格式非法）独立得出，不经本片改动的 `decode_handle`，故不变。
本片不触碰镜像布局与任务/cap 计数。

TCB 机制层行数 20219 → **20267**（+48：守卫与反例；`unsafe` 仍 0）。

## 5. 验证（2026-10-11）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **366**（+1 反例）、prima-ostd 21、xtask 195 |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20267 total_lines=21529 ratio=5.86%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**41 条事件**，与 SPEC-088 终点逐字节相同） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

## 6. 未决与登记

1. 本片仅闭合"越界句柄致 panic"这一条；`ISSUE-CAP-001` 的**闭合判断仍交复核者**（组织级独立性门槛）。
2. 未引入"句柄→派生链/位置表可独立判定不变量"的更广义模型缺口（tracker 另有一条同名 ISSUE-CAP-001
   指 host 模型的派生链/位置表/溯源缺口）——两者是**不同条目**，本片只处置畸形句柄越界这一条。
3. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
