# SPEC-093 设计：独立第二服务端点（独立二进制映像 + 独立物理栈段）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 前置：[SPEC-092 独立第二服务端点](2026-10-11-stage4-multi-endpoint-service-design.md)、
> [SPEC-091 vspace 存储移出](../reports/2026-10-11-stage4-vspace-arena-report.md)、
> [SPEC-088 服务端解析与协商](2026-10-10-stage4-service-negotiation-design.md)

## 1. 本片要回答的问题

SPEC-092 §6.1 如实登记其简化：**两端点由同一嵌入 ELF 创建、沿用 SPEC-088 的共享物理栈帧**。
本片闭合该登记项：

1. **不同二进制**——第二个端点（`BETA`）由**独立 ELF**（`prima-beta`）创建，而非 `prima-smoke`
   的第二次实例；
2. **独立物理栈帧**——第二个端点映射**独立栈段**，不再与第一个端点共享同一可写栈。

判决性见证据此从「相异 task id」升级为「**相异映像标记 + 相异栈段 id**」。

## 2. 机制

### 2.1 第二个二进制映像

`prima-user` 新增第二个 `[[bin]]` `prima-beta`（`src/beta.rs`）——与 `prima-smoke` 是**不同的
ELF**（各含独立 `_start`），共享 `prima-runtime::protocol` 的协商助手（本片把 `pack_negotiated`/
`negotiation_word`/`negotiation_negatives`/`service_offer` 从 bin 收敛进 runtime，避免重复）。

内核经 `CARGO_BIN_FILE_PRIMA_USER_prima-beta` `include_bytes!` 第二个映像（`PRIMA_BETA_ELF`），
在 bring-up 于**第二个代码段**里装载（`RUNTIME_APP2_CODE_PAGES=8`、独立 `r_entry`）。

> **为何必须是第二个 Segment**：`MAX_SEGMENT_FRAMES=16`，而 `prima-smoke` 10 页 + `prima-beta`
> 8 页 = 18 页 > 16，二者无法合并进一个 Segment；且 `SegmentFrames` 内联进 `BootstrapState`
> 的 `SegmentObject`，扩容会破 13 KiB 软守卫。

### 2.2 独立物理栈段

SPEC-092 起两端点都把 `Y[368]` 那个 `SYS_MEMSEG_CREATE` 生成的栈段 cap 映射进各自根——**同一
物理帧**。本片在 bring-up 另建一条 1 页栈段（`{SegMapR, SegMapW}` 的 `allowed_access`，根 cap
自动含 `Derive`），第二个端点的 `memseg_map` 改为映射**这条**栈段。

### 2.3 bring-up 结构的变化

`prepare_task_create_caps` 现建三段并返回**两个**代码段帧向量：

- 代码段 A（`prima-smoke`，10 页，cap 槽 `TASK_CREATE_CODE_CAP_SLOT`）；
- 代码段 B（`prima-beta`，8 页，cap 槽 `SERVICE2_CODE_CAP_SLOT`）；
- 栈段 B（1 页，cap 槽 `SERVICE2_STACK_CAP_SLOT`）。

`TaskCreateImages { code, code2 }` 是新的返回类型；`load_runtime_elf` 参数化「期望页数/入口」
以装载第二个映像。

### 2.4 端点 B 的 create 请求

第 5 次 `SYS_TASK_CREATE` 的请求块改为：

| 字段 | 值 |
|---|---|
| `code_seg_cap` | `SERVICE2_CODE_CAP_HANDLE`（第二个代码段） |
| `code_offset` / `code_pages` | `0` / `RUNTIME_APP2_CODE_PAGES` |
| `entry` | `USER3_CODE_VA + RUNTIME_APP2_ENTRY_OFFSET` |
| `initial_cap0` | `SERVICE2_SEND_CAP_HANDLE` |
| `initial_cap1` | tokenB（从**第二个**代码段根 cap 派生 `{SegMapR}`；非零 = 服务角色） |
| 栈映射 | `memseg_map(SERVICE2_STACK_CAP_HANDLE, mgr2, 0, USER3_STACK_VA, 1, R\|W)` |

## 3. 判决性见证（`service_selected_ready`，seq 28，扩展）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;
alpha_ep=0x9;beta_ep=0xa;beta_self=0xa;
alpha_marker=0x414c504841000001;beta_marker=0x4245544100000002;
alpha_stack_seg=0x15;beta_stack_seg=0x11
```

- `alpha_marker != beta_marker`：两个端点**各写入编译进自身 ELF 的标记**——相异即证明结果来自
  **不同程序**（`prima-smoke` vs `prima-beta`），而非同一映像的两次实例。
- `alpha_stack_seg != beta_stack_seg`：init 各自 `USER3_STACK_VA` 处的映射**所属 Segment id
  相异**——即**独立物理栈帧**，而非共享同一栈段。
- `alpha_ep != beta_ep`、`beta_ep == beta_self`、`negotiated_beta` 的 `max=1024` 沿用 SPEC-092。

## 4. 回归面（逐条登记；基线 = SPEC-092 HEAD）

42 条事件、seq **无增删**。rel diff：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready`（3） | `usable`/`image`/`kend` | 30167/1293/0x150d000 → **29943/1516/0x15ec000** |
| `bootstrap_ready`（9） | `image_len`/`image_frames`/`free_frames` | 5294584/1293/30167 → **6206248/1516/29943** |
| `task_ready`（24） | `caps`/`caps_after` | 60/60 → **62/62**（两条新 bring-up cap） |
| `preempt_ready`（30） | `rsp*`/`kstack*_top` | 前移（`root*`/`cr3*`/`data*`/`mark*`/`ticks` 不变） |
| `frames_ready`（36） | `universe`/`free`/`segment`/`table`/`image` | 31460/30024/31/112/1293 → **31459/29790/40/113/1516** |
| `self_root_ready`（37） | `pages` | 464 → **685** |
| `state_audit_ready`（39） | `used` | 248 → **250** |
| `service_selected_ready`（28） | 新增 `alpha_marker`/`beta_marker`/`alpha_stack_seg`/`beta_stack_seg` | 见 §3 |

**判定的不变**：`ipc_ready`（23）仍 8/9/12（本片不新增 IPC）；seq 27 `service_negotiated_ready`
逐字节不变；`platform_segments_ready`（34）逐字段不变。

`image_frames` 1293→1516（+223）源于 `include_bytes!(prima-beta)`——debug ELF ≈ 912 KB。距
`MAX_IMAGE_FRAMES=2048` 余 532 帧（≈2.1 MB），是本片新引入的**已登记上限**。

## 5. 未决与登记

1. **`ALPHA` 端点的栈仍与客户端共享**：本片只闭合 §6.1 的「**两端点**共享栈帧」；第一个端点
   的栈段仍是客户端（task 8）也映射的那条 `Y[368]` 段——属 §6.1 范围之外的既有共享，如实登记。
2. 注册表仍为编译期常数；名字为 64 位码而非字符串路径。
3. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
