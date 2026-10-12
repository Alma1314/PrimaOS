# SPEC-094 报告：`ALPHA` 端点独立栈段（闭合 SPEC-093 §5.1）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[独立栈段设计](../specs/2026-10-12-stage4-alpha-independent-stack-design.md)
>
> 前置：[SPEC-093 独立二进制映像](2026-10-11-stage4-independent-service-images-report.md)

## 1. 本片要回答的问题

闭合 SPEC-093 §5.1 登记的残留：**第一个**服务端点（`ALPHA`）的栈与存活客户端（task 8）
共享 `Y[368]` 段（同一物理栈帧）。本片让 `ALPHA` 映射 **bring-up 另建的独立栈段**，判决性
见证升级为「客户端 / `ALPHA` / `BETA` 三者栈段**两两相异**」。

## 2. 判决性见证（`service_selected_ready`，seq 28 扩展）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;
alpha_ep=0x9;beta_ep=0xa;beta_self=0xa;
alpha_marker=0x414c504841000001;beta_marker=0x4245544100000002;
alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x8;client_stack_seg=0x16
```

- `alpha_stack_seg(0x12/18) != beta_stack_seg(0x11/17) != client_stack_seg(0x16/22)`：三者在
  `USER3_STACK_VA` 处映射的**所属 Segment id 两两相异**——证明 `ALPHA` 不再沿用客户端 `Y[368]`
  的共享物理栈帧（由内核 `bootstrap::mapped_segment_at` 读回，非凭句柄推断）。
- 其余字段（`alpha_ep≠beta_ep`、`beta_ep==beta_self`、标记相异、`negotiated_beta` 的 `max=1024`）
  沿用 SPEC-092/093。

## 3. 实现要点

- **独立栈段**：`prepare_task_create_caps` 另建 1 页栈段（cap 槽 `SERVICE_STACK_CAP_SLOT`），
  `ALPHA` 的 `memseg_map` 改用它（原用 `Y[368]`）；客户端与 `BETA` 不变。
- **cap 槽位避开 Namespace 预留区间**：新槽 = **61**（见 §5；`ns_create` 预留区间为 52..59）。
- **`MAX_CAPABILITY_SLOTS` 64 → 72**：boot 自测的 `task_create` 前置在 cap 达 63 时（64 上限下）
  失败并导致调度器 `SYS_YIELD` 无 Ready 任务而 panic；扩上限至 72 消除（见 §5）。
- **见证**：`ServiceSelectedWitness` 增 `client_ep`/`client_stack_seg`；`evidence_ok` 增加
  「三者栈段两两相异」。

## 4. 回归面（手工事件流 diff，逐条登记；基线 = SPEC-093 HEAD）

42 条事件、seq **无增删**；rel diff：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready`（3） | `usable`/`free` | 29943/29943 → **29944/29944** |
| `bootstrap_ready`（9） | `image_len`/`free_frames` | 6206248/29943 → **6207960/29944** |
| `task_ready`（24） | `caps`/`caps_after` | 62/62 → **63/63**（一条新 bring-up cap） |
| `preempt_ready`（30） | `rsp*`/`kstack*_top` | 前移（`root*`/`cr3*`/`data*`/`mark*`/`ticks` 不变） |
| `frames_ready`（36） | `universe`/`segment` | 31459/40 → **31460/41** |
| `self_root_ready`（37） | `pages` | 685 → **686** |
| `state_audit_ready`（39） | `used` | 250 → **251** |
| `service_selected_ready`（28） | `alpha_stack_seg`；新增 `client_ep`/`client_stack_seg` | 0x15 → **0x12**；**0x8/0x16** |

**判定的不变**：`ipc_ready`（23）仍 8/9/12；seq 25/26/27（`runtime_hello`/`service_reply`/
`service_negotiated`）逐字节不变；`platform_segments_ready`（34）逐字段不变；`namespace_ready`
（29）**`evidence_ok=1`**（完整）。

同镜像连跑两次 42 条事件逐字节相同。

## 5. boot 期 cap 计数耦合：定位与修复（判决性证据）

新增 bring-up cap 一度引发**级联退化**（`task_ready caps` 62→**59**、namespace 半途中止
`binds 2→1`/`evidence_ok=0`、`audit.used` 250→238）。逐层定位：

1. **收据级**：扩展 `namespace_ready` 临时输出逐条回执，发现 `r_cc = 0x…e4`（**ENOSPC**）——
   第 3 次 `ns_create` 失败；`r_ca=slot58`、`r_cb=slot59` 说明 namespace 根 cap 落在**高位槽**。
2. **代码级**：`ns_create`（`bootstrap.rs`）把根 cap 铸在固定预留区间
   `[NS_ROOT_CAP_SLOT_BASE=52, +MAX_NAMESPACE_OBJECTS=8)` = **52..59**；而 usercap
   （`USER_IPC_CAP_SLOT_BASE=47`，SPEC-087..093 已至 47..56）与该区间**重叠**，令
   `ns_create` 实际只剩 57/58/59 三槽——恰够 boot 的 3 次；新 cap 抢走一槽即 `ENOSPC`。
3. **panicky 反例**：把新 cap 放在区间**外**的 61 后，出现 Rust panic；
   `#[panic_handler]` 打印 `file:line` 定位到 **`kernel/src/syscall.rs::yield_call`**
   （`SYS_YIELD` 时 `pick_next` 无 Ready 任务）——即 boot 自测的 `task_create` 前置
   `live_count + 3 + initial_count ≤ MAX_CAPABILITY_SLOTS` 在 cap=63、上限 64 下失败，
   子任务未就绪，init 首个 `SYS_YIELD` 无任务可切。
4. **修复**：新 cap 槽 = **61**（区间外，且避开 `UNSET_CAP`=60）**且**
   `MAX_CAPABILITY_SLOTS` **64 → 72**（余量覆盖 `63 + 3 + 2 = 68`）。两者缺一不可。

实验对照（`cargo xtask test`）：仅 +1 cap → caps 59、`binds=1`；+1 cap 且扩上限 → caps 63、
`binds=1`（ns 仍缺槽）；+1 cap@区间外且扩上限 → **caps 63、`binds=2`、`evidence_ok=1`**（绿）。

## 6. 验证（2026-10-12）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 366、prima-ostd 21、xtask **203**（+1 反例） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20836 total_lines=22098 ratio=5.71%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**42 条事件**，含 seq 28 扩展见证） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例（xtask 单位测试，对合成输入）：
`rejects_service_selected_when_alpha_endpoint_shares_the_client_stack_segment`（`ALPHA` 与客户端
栈段相同即失败），连同 SPEC-090/092/093 的七条。

## 7. 未决与登记

1. **cap 槽位区间重叠隐患（既有，自 SPEC-087）**：usercap `47..56`（`USER_IPC_CAP_SLOT_BASE=47`
   `+N`）与 Namespace 根 cap 预留区间 `52..59` 重叠。本片以「新 cap 放区间外 + 扩上限」绕过，
   **未重构布局**（重构会平移 SPEC-087..093 的 usercap 槽位、改动既有证据）。建议后续单独切片
   把两组区间**分离**（如把 Namespace 预留区间移到不重叠的地址段）。
2. 服务注册表仍为**编译期内联 `cmp`**、名字为 64 位码——由紧随的 SPEC-095（运行期服务注册表）闭合。
3. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
