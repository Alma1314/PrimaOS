# SPEC-103 报告：ring3 服务管理器任务的引入（迁移第一片）

> 状态：探索实现；不构成 S4-D2/D3 验收或稳定 ABI
>
> 设计依据：[把服务管理器迁到 ring3 Rust](../specs/2026-10-13-stage4-ring3-service-manager-design.md)
>
> 前置：[SPEC-102 `TaskSpec.name` 前置](2026-10-13-stage4-string-service-names-report.md)
>
> 基线：SPEC-102 HEAD `d5ec48d`

## 1. 本片要回答的问题

设计与分片（见设计 §6）的第一片：把「服务管理器」从 `init` 的手写机器码里**立起来成一个独立的
ring3 Rust 任务**（`prima-servicemgr`）。本片只证明「这是一个被 `init` 建起、收到移交、能自报存活
的独立 ring3 任务」；**运行期注册表、按名解析、请求路由/转发**的迁移留 SPEC-104。

## 2. 判决性见证（`servicemgr_ready`，seq 29）

```
task_id=0xb;hello=0x5e5600000000000b
```

- `task_id=0xb`（= 11）：从第六次 `SYS_TASK_CREATE` 的返回（新任务的**管理 cap**）经
  `bootstrap::task_of_cap` 解出的目标 task id——证明 `init` 建起了一个**真实子任务**（非 init）。
- `hello=0x5e5600000000000b`：该任务**自报存活**的 payload（`SERVICEMGR_HELLO_TAG` 高 16 位 ‖ 自身
  task id 低 8 位）。判决性在**交叉核对**：自报 id 低 8 位（`0xb`）**恰等于**从管理 cap 解出的
  `task_id`——即存活自报来自 `init` 建起的**同一个**任务，而非任何编译期常量（反例：任一不符即拒绝，
  见 xtask 两条负例）。
- 该任务的代码来自**独立二进制** `prima-servicemgr`（与 `prima-smoke`/`prima-beta` 相异的第三个
  ELF），栈来自独立栈段。

## 3. 实现要点

- **新二进制** `prima-user/src/servicemgr.rs`：`_start` 用 `initial_cap0`（`init` 移交的 `Task(1)`+
  `TaskSend` 凭据）向 task 1 自报 `SERVICEMGR_HELLO_TAG ‖ id`，随后阻塞在 `recv`。
- **内核 cap 布置**（`bootstrap`）：新增 `SERVICE3_CODE_CAP_SLOT`(`+10`)/`SERVICE3_STACK_CAP_SLOT`
  (`+11`)/`SERVICEMGR_SEND_CAP_SLOT`(`+12`)；`prepare_task_create_caps` 建服务管理器的独立代码段
  （承载 `prima-servicemgr` 映像）与独立栈段，并铸造其回发凭据；`TaskCreateImages` 增 `code3`。
  usercap 固定块守卫扩到 `+13`（`47..59`，仍与 `UNSET_CAP`(60)/NS 顶端区间不相交）。
- **init 手写块**（`user.rs` 页 1 尾）：自足写请求块（code_seg_cap/offset/pages/entry/initial_cap0/
  initial_cap1/name）→ 第 6 次 `SYS_TASK_CREATE` → 把其独立栈段映射进其根 → `SYS_YIELD` 让其运行 →
  `recv` 循环收 `SERVICEMGR_HELLO_TAG` → 存 Y 页回执（`784`/`792`）。
- **kstack**：`USER_TASK_KSTACK_COUNT` 8→**9**（`next_free_id` 单调、`Dead` 记录不复用 id，服务管理器
  拿到 **id 11**，此前只覆盖到 id 10 ⇒ `register_created_task` 失败、`task_create` 返回 `ENOSPC`，
  随之的 `SYS_YIELD` 无后继而故障——**本片实测定位并修复**）。
- **见证** `ServicemgrWitness { task_id, hello }`，`evidence_ok` 作 `task_id > 1` + tag 正确 +
  自报 id 低 8 位 == `task_id` 低 8 位。

## 4. 回归面（手工事件流 diff，逐条登记；基线 = SPEC-102 HEAD `d5ec48d` 的 42 条）

43 条事件、seq 结构调整（**新增** seq 29 `servicemgr_ready`，其后 seq 顺移 +1）；值变化 9 条：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready` | `usable`/`image`/`free`/`kend` | 29930/1529/29930/0x15f9000 → **29717/1738/29717/0x16ca000** |
| `bootstrap_ready` | `image_len`/`image_frames`/`free_frames` | 6262296/1529/29930 → **7116640/1738/29717** |
| `ipc_ready` | `queued`/`blocks` | 8/12 → **9/13**（`direct`=9 不变） |
| `task_ready` | `tasks`/`caps`/`tasks_after`/`caps_after` | 6/63/6/63 → **7/69/7/69** |
| `service_selected_ready` | `client_stack_seg` | 0x16 → **0x18**（`alpha`/`beta` 栈段不变） |
| `preempt_ready` | `rsp*`/`kstack*_top` | 前移（`root*`/`cr3*`/`data*`/`mark*`/`ticks` 不变） |
| `frames_ready` | `universe`/`free`/`segment`/`table`/`image` | 31459/29776/41/113/1529 → **31455/29553/47/117/1738** |
| `self_root_ready` | `pages` | 697 → **907** |
| `state_audit_ready` | `total`/`to_running`/`to_ready`/`to_blocked`/`used` | 87/37/32/14/271 → **91/39/33/15/284** |
| `servicemgr_ready` | — | **新增**（seq 29，见 §2） |

根因分类：①**镜像 +209 帧**（嵌入第三个 ~0.84 MB 调试 ELF `prima-servicemgr`）；②**kstack +4 帧**
（BSS）+**3 条新 cap**（服务管理器代码/栈段 + 回发凭据）与**+1 任务**（自身 self/root-ns cap）⇒ caps
63→69；③**+2 段**（代码 5 页 + 栈 1 页 = +6 段帧）；④`Dead` 记录使新任务取 id 11，`client` 运行期
`memseg_create` 的栈段 id 顺移 +2（0x16→0x18）。

**判定的不变**：seq 25/26/27（`runtime_hello`/`service_reply`/`service_negotiated`）逐字节不变；
`namespace_ready.evidence_ok=1`、`platform_segments_ready`（`segments=13`）、`authority_ready`、
`timer_ready`、`audit_ready`、`exc_*`、`handle_ready`、`cap_lifecycle_ready`、`memseg_ready`、
`fault_ready`、`reclaim_ready`、`resume_ready` 逐字段不变。

同镜像连跑两次 43 条事件逐字节相同。

## 5. 验证（2026-10-14）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 146、cosir_state 42、lean_backtrace 3、kernel 369、prima-ostd 21、prima-runtime 1、xtask **206**（+2 servicemgr 负例） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=21288 total_lines=22550 ratio=5.60%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**43 条事件**，含 seq 29 `servicemgr_ready=ok`） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例/守卫（host）：xtask 新增 `rejects_servicemgr_when_the_liveness_report_is_not_from_the_created_task`
（自报 id 与被建任务 id 不符）与 `rejects_servicemgr_when_the_task_id_is_init`（id=1）；`ipc_ready`/
`task_ready` 期望值据新计数更新（`9/9/13`、`tasks=7`/`caps=69`）及其三条负例锚点同步。

## 6. 未决与登记

1. 本片**只立起服务管理器任务并证明移交**；注册表/按名解析/路由/转发**仍在 `init` 的手写机器码里**
   （SPEC-090/095/101 未动）——迁移的实质收益在 SPEC-104/105。
2. 服务管理器当前只自报存活即阻塞 `recv`；尚无注册表、尚无端点创建。
3. `next_free_id` 单调（`Dead` 记录不复用）⇒ 每加一个任务须相应扩 `USER_TASK_KSTACK_COUNT`；本片
   id 11→count 9。这是既有约束（登记为后续若要更多任务须同步）。
4. **不勾选** S4-D2/D3 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
5. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
