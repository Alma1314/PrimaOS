# SPEC-093 报告：独立第二服务端点（独立二进制映像 + 独立物理栈段）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[独立二进制映像设计](../specs/2026-10-11-stage4-independent-service-images-design.md)
>
> 前置：[SPEC-092 多端点](2026-10-11-stage4-multi-endpoint-service-report.md)、
> [SPEC-091 vspace arena](2026-10-11-stage4-vspace-arena-report.md)

## 1. 本片要回答的问题

闭合 SPEC-092 §6.1 登记的简化：两端点**同一 ELF、共享物理栈帧**。本片让第二个端点由**独立
二进制映像**（`prima-beta`）创建、映射**独立栈段**，判决性见证升级为**相异映像标记 + 相异栈段 id**。

## 2. 判决性见证（`service_selected_ready`，seq 28 扩展）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;
alpha_ep=0x9;beta_ep=0xa;beta_self=0xa;
alpha_marker=0x414c504841000001;beta_marker=0x4245544100000002;
alpha_stack_seg=0x15;beta_stack_seg=0x11
```

- `alpha_marker != beta_marker`：两个端点分别写入**编译进各自 ELF** 的标记（`prima-smoke` /
  `prima-beta`）——证明结果来自**不同程序**，而非同一映像的两次实例。
- `alpha_stack_seg(0x15) != beta_stack_seg(0x11)`：由内核读两任务在 `USER3_STACK_VA` 处映射
  **所属 Segment id**（`bootstrap::mapped_segment_at`）——证明**独立物理栈帧**，非共享栈段。

其余（`alpha_ep≠beta_ep`、`beta_ep==beta_self`、`negotiated_beta` 的 `max=1024`）沿用 SPEC-092。

## 3. 实现要点

- **第二个二进制**：`prima-user` 新增 `[[bin]] prima-beta`（`src/beta.rs`），与 `prima-smoke`
  是不同的 ELF；协商助手（`pack_negotiated`/`negotiation_word`/`negotiation_negatives`/
  `service_offer`）从 bin 收敛进 `prima-runtime::protocol` 复用，bin 变薄。
- **两个代码段**：`MAX_SEGMENT_FRAMES=16` 容不下 10+8 页，且 `SegmentFrames` 内联进
  `BootstrapState`，扩容会破 13 KiB 守卫——故第二个映像另建 Segment
  （`SERVICE2_CODE_CAP_SLOT`）。`prepare_task_create_caps` 返回 `TaskCreateImages{code,code2}`。
- **独立栈段**：bring-up 另建 1 页栈段（`SERVICE2_STACK_CAP_SLOT`），B 改映射它。
- **B 的 create 请求**：`code_seg_cap`=第二个代码段、`code_pages`/`entry` 用第二个映像、栈映射
  换 `SERVICE2_STACK_CAP_HANDLE`（见设计 §2.4）。
- **见证**：`ServiceSelectedWitness` 增 `alpha_marker`/`beta_marker`/`alpha_stack_seg`/
  `beta_stack_seg`；`evidence_ok` 增加「标记相异」「栈段相异且非零」。

## 4. 回归面（手工事件流 diff，逐条登记；基线 = SPEC-092 HEAD）

42 条事件、seq **无增删**；rel diff：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready`（3） | `usable`/`image`/`kend` | 30167/1293/0x150d000 → **29943/1516/0x15ec000** |
| `bootstrap_ready`（9） | `image_len`/`image_frames`/`free_frames` | 5294584/1293/30167 → **6206248/1516/29943** |
| `task_ready`（24） | `caps`/`caps_after` | 60/60 → **62/62** |
| `preempt_ready`（30） | `rsp*`/`kstack*_top` | 前移（`root*`/`cr3*`/`data*`/`mark*`/`ticks` 不变） |
| `frames_ready`（36） | `universe`/`free`/`segment`/`table`/`image` | 31460/30024/31/112/1293 → **31459/29790/40/113/1516** |
| `self_root_ready`（37） | `pages` | 464 → **685** |
| `state_audit_ready`（39） | `used` | 248 → **250** |
| `service_selected_ready`（28） | 新增 4 字段 | 见 §2 |

**判定的不变**：`ipc_ready`（23）仍 8/9/12（无新增 IPC）；seq 27 逐字节不变；
`platform_segments_ready`（34）逐字段不变。`segment` 31→40 = +8（第二代码段）+1（第二栈段）。

`image_frames` +223 源于嵌入第二个 debug ELF（≈912 KB）：距 `MAX_IMAGE_FRAMES=2048` 余 532 帧
（≈2.1 MB）——**本片新引入的已登记上限**，后续再嵌大二进制需先评估。

同镜像连跑两次 42 条事件逐字节相同。

## 5. 验证（2026-10-11）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 366、prima-ostd 21、xtask **202**（+2 反例） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20804 total_lines=22066 ratio=5.72%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**42 条事件**，含 seq 28 扩展见证） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例（xtask 单位测试，对合成输入）：
`rejects_service_selected_when_both_endpoints_run_the_same_binary`（两标记相同即失败）、
`rejects_service_selected_when_the_two_endpoints_share_a_stack_segment`（两栈段 id 相同即失败），
连同 SPEC-090/092 的五条。

## 6. 未决与登记

1. **`ALPHA` 端点栈仍与客户端共享**：本片只闭合 §6.1 的「**两端点**共享栈帧」；第一个端点的
   栈段仍是客户端（task 8）也映射的 `Y[368]` 段。属 §6.1 范围之外的既有共享，如实登记。
2. 注册表仍为编译期常数；名字为 64 位码而非字符串路径。
3. **S4-D2 与阶段 4 交付项/退出条件一律不勾选**；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
