# SPEC-108 报告：服务**栈段创建**下沉到 ring3 服务管理器

> 状态：探索实现；**不构成 S4-D2/D3 验收或稳定 ABI**，不勾选任何阶段 4 交付项/退出条件
>
> 依据：[把服务管理器迁到 ring3 Rust](../specs/2026-10-13-stage4-ring3-service-manager-design.md) §3 注
> （「把『创建』也搬进 ring3 是 §6 的后续片」）+ [SPEC-106 报告](2026-10-14-stage4-three-base-services-report.md) §7.1
>
> 前置：SPEC-107
>
> 基线：SPEC-107 HEAD `201fbc4`

## 1. 本片做了什么

把服务任务的**栈段创建**从 `init` 手写机器码 / 内核 boot 搬到 **ring3 服务管理器**——设计 §3 注
承认的「创建下沉」的第一半。

- **内核不再为 5 个服务建独立栈段**（`ALPHA`/`BETA`/`log`/`ns`/`store`）：`prepare_task_create_caps`
  删除这 5 个 `create_segment_object` 调用（管理器的 `SERVICE3_STACK` 保留——它的栈必须先于它自己
  存在，由 `init` 直接映射）。
- **`init` 每个服务的移交由「代码段 + 栈段」2 cap 降为「代码段」单 cap**：新 tag
  [`SERVICE_SETUP_CODE_TAG`]（`0x5ECD…`）与父 Namespace 的 `SERVICE_SETUP_TAG`（`0x5ECC…`，payload
  仅 tag）以高 16 位区分。每条仍先发无 cap 唤醒（管理器常态阻塞在 `recv`）。
- **[`prima-servicemgr`](../../../prima-user/src/servicemgr.rs) 自建栈段**：`create_service` 在
  `task_create` 前调 `SYS_MEMSEG_CREATE`（1 页，`SegMapR|SegMapW`），随后 `memseg_map` 进新端点根。
  内核 `memseg_create` 不设调用者权限门（仅容量/审计），且铸出的根 cap 属主是管理器自身——故
  「自建 + 自映射」在最小权限下直接成立。

## 2. 判决性见证

- **`servicemgr_ready`（seq 29）逐字段不变**：`created=0x5;services=0x3;log_id=0xc;ns_id=0xd;
  store_id=0xe;` 三段 `mark`/`reply` 与 SPEC-106 完全一致——栈段的产出方改变**没有**改变服务任务的
  创建结果（相同 id、相同二进制、相同路由）。
- **栈段 id 随创建方改变**：`service_selected_ready` 的 `alpha_stack_seg` 0x12→**0x1c**、
  `beta_stack_seg` 0x11→**0x1d**、`client_stack_seg` 0x1e→**0x19**——反映内核 5 个栈段消失后段 id
  前移、且三个端点栈现在由**管理器**在自己的时点创建（互异关系不变，见 §3）。
- 43 条事件序列**无增删**；`boot_complete`、退出码 33。

## 3. 逐条登记的既有事件变化

除下列字段外，其余事件逐字段不变（43 条 seq 无增删）：

1. `memory_ready` `image` 1122→**1120** / `kend` 0x1462000→**0x1460000**（内核代码变小：删除 5 段
   栈段的创建代码）。
2. `bootstrap_ready` `image_len` 4592456→**4585936** / `image_frames` 1122→**1120**。
3. `service_negotiated_ready` `token` 0x10000003c→**0x100000038** / `service` 0x100000044→**0x10000003d**
   （`init` cap 槽少占 5 个栈段 cap → 端点令牌/管理 cap 的槽索引前移）。
4. `service_selected_ready` `alpha_stack_seg` 0x12→**0x1c** / `beta_stack_seg` 0x11→**0x1d** /
   `client_stack_seg` 0x1e→**0x19**（§2）。
5. `preempt_ready` `rsp1/rsp2/kstack*_top` 顺移（BSS 位移，`KSTACK_TASKS` 数组随段对象减少而挪动）；
   `ticks`/`pre*`/`data*`/`mark*`/`root*`/`cr3*` 不变。
6. `frames_ready` `universe` 31358→**31356** / `image` 1122→**1120**（`free`/`segment`/`table` 不变；
   `conserved`/`disjoint` 不变量仍成立）。
7. `state_audit_ready` `used` 475→**465**（少 5 个段对象的创建/销毁审计条）。

**xtask 同步**：`SERVICE_NEGOTIATED_TOKEN` → `0x1_0000_0038`、`SERVICE_NEGOTIATED_CAP` →
`0x1_0000_003d`（校验器与真实输出比对的常量），`VALID` 夹具 seq 27 同步；其余为关系不变量
（stack_seg 非零/互异、`free+segment+table+image==universe`、`used<=cap` 等）**无需改**——本片
未放宽任何校验。

## 4. 未决与登记

1. **代码段仍未下沉**：本片只下沉了栈段。`init` 仍为每个服务移交**独立代码段 cap**（内核
   `prepare_task_create_caps` 仍建 6 个 code 段）。要达成「加一个服务 = 表一行 + 一个二进制 +
   `init` 零手写改动」，须把 6 个 ELF 合成**一个服务映像段**、以管理器 `initial_cap1` 零 IPC 移交，
   由管理器**自解每个 ELF 的布局**（program header → pages/entry）后按 `code_offset` 表驱动创建。
   **已知前置障碍**：单段需 `MAX_SEGMENT_FRAMES` 由 16 扩容（6 个 ELF 合计约 18 页）；该扩容会平移
   后续段/帧序列，属下一次登记范围。**本片不宣称**「零 `init` 改动」已达成。
2. `SERVICE_STACK_CAP_SLOT`/`SERVICE2_STACK_CAP_SLOT` 等 5 个栈段 cap 槽常量**保留定义**（仍被
   `bootstrap` 的槽位布局单测引用），但 SPEC-108 起**不再铸段**；后续可随布局整理一并清理。
3. SPEC-107 §3 的反向验证结论仍成立：内核 16 KiB 栈下 `destroy_task_self` 路径仍溢出，与本片无关。
4. 本片**不勾选**任何 S4-D2/D3 或阶段 4 交付项/退出条件；闭合判断交复核者。

## 5. 验证（2026-10-14）

- `cargo test --workspace`：PASS（**788 项**：cosir 146 + cosir_state 42 + lean 3 + kernel 369 +
  prima-ostd 21 + prima-runtime 1 + xtask 206）。
- `cargo xtask lint`：四条 PASS（fmt / `clippy -D warnings` / 内核 `deny(unsafe_code)` /
  「机制层零 `unsafe`」27 文件扫描）。
- `cargo xtask tcb`：`trusted_lines=1262 mechanism_lines=21146 total_lines=22408 ratio=5.63%`，
  机制层 `unsafe=0`。
- `cargo xtask test`：六条 PASS（43 条事件，§3 已逐条登记）。
- QEMU 直跑：`boot_complete`，退出码 33。
