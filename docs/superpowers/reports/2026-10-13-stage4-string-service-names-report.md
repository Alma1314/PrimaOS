# SPEC-101 报告：字符串服务名与端点名的运行期指派（闭合 SPEC-095 §5.1）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[字符串服务名与端点名的运行期指派设计](../specs/2026-10-13-stage4-string-service-names-design.md)
>
> 前置：[SPEC-095 运行期服务注册表](2026-10-12-stage4-runtime-service-registry-report.md)
>
> 基线：SPEC-100 HEAD `9673a11`

## 1. 本片要回答的问题

闭合 SPEC-095 §5.1 登记的「名字仍是 64 位码（`1`/`2`），非字符串路径；注册仍是编译进二进制
（端点不能运行期改名）」——服务名改为 ASCII 字符串令牌，端点注册名由服务管理器在 `task_create`
时**运行期指派**（请求块新字段 → 新任务初始帧 `rcx`）。

## 2. 判决性见证（`service_selected_ready`，seq 28）

```
name=0x61746562;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;
alpha_ep=0x9;beta_ep=0xa;beta_self=0xa;alpha_marker=0x414c504841000001;beta_marker=0x4245544100000002;
alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x8;client_stack_seg=0x16;
reg_a_name=0x6168706c61;reg_b_name=0x61746562
```

- `reg_a_name=0x6168706c61`（`"alpha"`）、`reg_b_name=0x61746562`（`"beta"`）：两个端点回报的注册名
  是**运行期指派**的 ASCII 令牌。判决性：`prima-smoke` 二进制里唯一的名字字面量是客户端请求名
  `"beta"`，而 `ALPHA` 端点回报的是 `"alpha"`（相异）⇒ 它只可能来自创建时写入的 `rcx`；
  `prima-beta`（纯端点二进制）**无任何名字字面量**，其 `"beta"` 同样只可能来自 `rcx`。
- `name=0x61746562`：客户端从共享段写下的请求名（众所周知常量）；经按名扫描命中注册表 B 项 →
  `resolved=0x2`。`unknown=0x0`：未注册名 `"ghost"` 未命中 → `NONE`。

## 3. 实现要点

- **字符串名**：`u64` 低 40 位打包 5 字节 ASCII（`"alpha"`/`"beta"`/负例 `"ghost"`）；内核
  `user::service_name` 与运行时 `protocol::pack_service_name` 各一份同款 `const fn`。
- **运行期指派**：`task_create` 请求块第 11 个 qword `name`（`[rdx+0x50]`）→ `TaskCreateRequest.name`
  → `register_created_task` 写入新任务初始帧 `rcx`（第 4 个入口参数）；`init` 创建两端点时写入
  `SERVICE_NAME_ALPHA`/`SERVICE_NAME_BETA`，端点 `_start(..., name_raw)` 回报之。
- **自报布局**：`tag(48..63) | name(8..47) | id(0..7)`（原 `name(16..31) | id(0..15)`）；`init` 侧
  `id = payload & 0xff`、`name = (payload>>8)` 经 `shl/shr 24` 掩出低 40 位。
- **客户端请求名保持众所周知常量**（客户端是请求方）；运行期指派的是**端点**注册名。

## 4. 回归面（手工事件流 diff，逐条登记；基线 = SPEC-100 HEAD `9673a11`）

42 条事件、seq **无增删**；rel diff（5 条）：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready`（3） | `usable`/`free`/`image`/`kend` | 29932/29932/1527/0x15f7000 → **29931/29931/1528/0x15f8000** |
| `bootstrap_ready`（9） | `image_len`/`image_frames`/`free_frames` | 6253008/1527/29932 → **6255736/1528/29931** |
| `service_selected_ready`（28） | `name`/`reg_a_name`/`reg_b_name` | 0x2/0x1/0x2 → **0x61746562/0x6168706c61/0x61746562** |
| `preempt_ready`（30） | `rsp*`/`kstack*_top` | 前移（`root*`/`cr3*`/`data*`/`mark*`/`ticks` 不变） |
| `frames_ready`（36） | `free`/`image` | 29778/1527 → **29777/1528**（`universe`/`segment`/`table` 不变） |

镜像增长（`image` 1527→1528、`kend` 0x15f7000→0x15f8000）源于 init 页 1 代码增长（端点名写入 ×2 +
自报名提取位数变化）与 `prima-user` 新入口参数随内核嵌入。

**判定的不变**：seq 25/26/27（`runtime_hello` 的 `map`/wire、`service_reply`、`service_negotiated`）
逐字节不变；`ipc_ready`/`task_ready` 逐字段不变；`namespace_ready.evidence_ok=1`；
`platform_segments_ready` 逐字段不变；`self_root_ready.pages=695`、`state_audit_ready`
（`total=87;used=271;cap=512`）不变——本片不追加任何 Audit 事件。

同镜像连跑两次 42 条事件逐字节相同。

## 5. 验证（2026-10-13）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 146、cosir_state 42、lean_backtrace 3、kernel **369**（+1 正例）、prima-ostd 21、xtask **204** |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=21067 total_lines=22329 ratio=5.65%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**42 条事件**，含 seq 28 的新名字令牌） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例/守卫（host 单测）：新增 `service_names_pack_ascii_tokens`（钉死 `"alpha"`/`"beta"`/`"ghost"`
打包值与截断）；`task_req_bounds_rejects_misaligned_and_cross_page` 据请求块 80→88 B 更新边界；
xtask 负例 `rejects_service_selected_when_the_registry_is_not_built_from_self_reports` /
`rejects_service_selected_when_name_is_hardcoded` 的锚点据新令牌同步。

## 6. 未决与登记

1. 服务名上限 **5 字节**（自报 payload 位宽约束）；更长名字需把名字移入共享段或改多条消息。
2. 注册表仍是 `init` 内 Y 页 **2 项**表 + 线性扫描；未引入内核侧注册对象或 N 项动态扩容
   （SPEC-095 §5.2 仍未消）。
3. **设计受既有约束塑形（如实登记）**：init 页 0 的 S2-D4 块用**字面字节偏移**编写且被约 70 条
   host 断言钉死，故未在其中为历史任务 3 追加名字写入；该任务走客户端角色、其请求名不被任何见证
   读取。因此本片只让**端点注册名**运行期化，**客户端请求名**仍是众所周知的共享常量。
4. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
5. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
