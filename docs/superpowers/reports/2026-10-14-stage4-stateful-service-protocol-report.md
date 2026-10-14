# SPEC-110：有状态服务请求协议（三服务语义下沉）

**日期**：2026-10-14
**阶段**：阶段 4（S4-D3 续，任务 #7 之①）
**基线**：`fc4fa29`（SPEC-109）

## 目标

SPEC-106 只判决「请求经服务表**路由到了正确的**服务二进制」——三个基础服务对探针都是**纯回声**
（回显入参）。本切片把请求协议升级为**有状态**：服务按 `op` 语义把入参并入**自身内部状态**，
查询返回的是**跨请求保留的状态**而非入参回显。定序「推进 / 推进 / 查询」后**三个服务终值互异**
（各自状态转移语义不同），任一**无状态回声端点无法通过**。

## 协议

请求 `SERVICE_SERVE_TAG | op(40..47) | arg(0..31)`：
- `op=0`（`ADVANCE`）把 `arg` 并入服务内部状态；
- `op=1`（`QUERY`）只读回当前状态（不改）。

应答沿用 `pack_service_reply(kind, state)`（`kind` 归槽 + 低 32 位当前状态）。

### 三服务的状态转移（语义各异）

| 服务 | 转移 | 5 → 3 → 查询 |
|---|---|---|
| 日志 | `state += arg`（累计） | `5 / 8 / 8` |
| Namespace | `state = (state << 4) ^ arg`（滚动指纹） | `5 / 0x53 / 0x53` |
| 存储 | `state = (state << 8) \| (arg & 0xff)`（移位追加） | `5 / 0x503 / 0x503` |

三个服务的第二次推进结果 `8 / 0x53 / 0x503` **两两互异** ⇒ 判决三者是各自**有状态**的服务二进制
（而非同一个回声端点）。纯回声会回 `5 / 3 / 0`。

## 改动

| 文件 | 内容 |
|---|---|
| `prima-runtime/src/protocol.rs` | 新增 `SERVICE_SERVE_OP_SHIFT/_ADVANCE/_QUERY`、`SERVICE_SERVE_ARG_A/B`、`SERVICE_SERVE_PROBES`、`pack_service_request`；删 `SERVICE_SERVE_PROBE` |
| `prima-user/src/service.rs` | `run` 加内部 `state`；按 `op` 分派 `advance(kind, state, arg)`（三服务语义各异） |
| `prima-user/src/servicemgr.rs` | 每服务探针 1 条 → **定序 3 条**（推进 A / 推进 B / 查询）；应答按到达顺序写各服务 3 槽；`services` 计数改为「答满 3 条的服务数」（语义不变） |
| `kernel/src/user.rs` | 定序状态见证 3×3；`SERVICE_REPLY_{LOG,NS,STORE}_{0,1,2}` 九常量 + `read_reply_triple`；判决逐字段相等 |
| `kernel/src/protocol.rs` | `servicemgr_ready` 输出 9 个 `*_stateN` 字段 |
| `xtask/src/protocol.rs` | 九常量、解析/断言九字段、`ipc_ready` queued 金标、否定测试修正 |

## 事件登记（逐条）

| 事件 | 字段 | 旧 | 新 | 依据 |
|---|---|---|---|---|
| `ipc_ready`(23) | queued | 18 | 30 | 每服务定序请求 1→3（+2 请求 +2 应答）× 3 服务 = +12 |
| `servicemgr_ready`(29) | data 键 | `log_reply/ns_reply/store_reply` | `log_state0..2/ns_state0..2/store_state0..2` | 单条回显 → 三条定序状态 |

**顺带修复（SPEC-109 遗留）**：否定测试 `rejects_ipc_ready_without_the_direct_wake_path` 的断言停在
`expected 28/23/30`，与 SPEC-109 提交的校验值 `18` **本就不一致**（`cargo test --workspace` 本应
失败）。本次随 queued 变更一并修正为 `30`。

**自动通过（关系不变量，无需改）**：`task_ready` 的 caps（97）、`service_negotiated_ready` 的
token/service、`service_selected_ready` 的三段 id、`frames_ready`（关系不变量）、`state_audit_ready`、
`namespace_ready` 等。

`frames_ready`(37)：`image` 1126→1127、`segment` 70→71、`free` 30029→30027（内核代码增 ~54 行 → 映像 +
段各多 1 页）——**关系不变量自动通过**，登记以免误解。

## 验证

- `cargo xtask test`：**6/6 PASS**（build kernel / image bios / image uefi / qemu bios run=1 / run=2 / stage1 boot slice）
- `cargo xtask lint`：**4/4 PASS**（含 `-D warnings`、`deny(unsafe_code)`、零 `unsafe`）
- `cargo test --workspace`：**788 passed, 0 failed**
- `cargo xtask tcb`：`mechanism_lines` 21069→**21123**，`ratio=5.64%`，机制层 `unsafe=0`
- QEMU `seq=29 servicemgr_ready|status=ok`：
  `log_state=5/8/8`、`ns_state=5/0x53/0x53`、`store_state=5/0x503/0x503`、`services=0x3`

## 判决性小结

- **有状态**：查询返回的是推进后的**累计/滚动/移位**值，回声端点只会回入参（`5/3/0`）。
- **路由正确**：应答 `kind` 归槽（沿用 SPEC-106）。
- **三服务各异**：同一请求序列得到 `8 / 0x53 / 0x503` 三个互异终值 ⇒ 各自独立的**状态语义**，
  而非同一实现换 `kind`。
