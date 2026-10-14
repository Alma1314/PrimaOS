# SPEC-105 报告：ring3 服务管理器**表驱动创建端点**

> 状态：探索实现；不构成 S4-D2/D3 验收或稳定 ABI
>
> 设计依据：[把服务管理器迁到 ring3 Rust](../specs/2026-10-13-stage4-ring3-service-manager-design.md) §2/§3/§6
>
> 前置：[SPEC-104 主体：注册表/解析/路由迁入 servicemgr](2026-10-14-stage4-servicemgr-migration-body-report.md)
>
> 基线：SPEC-104 HEAD `10ff122`

## 1. 本片做了什么

把**端点任务的创建**从 `init` 的手写 x86 机器码迁入 ring3 `prima-servicemgr`。迁移后：

- `init` 的两大段手写创建块（SPEC-088 的 `ALPHA`、SPEC-092/093 的 `BETA`：各约 100 行
  `put_*`，含请求块写入、令牌/发送凭据派生、`SYS_TASK_CREATE`、栈段 `SYS_MEMSEG_MAP`）被删去，
  换成**一次 cap 移交**：三条 `SERVICE_SETUP_TAG` 消息（每条先发一条无 cap 唤醒），把
  **父 Namespace** + 每个服务的「代码段 cap + 独立栈段 cap」共 5 条 cap 交给管理器。
- `prima-servicemgr` 按 [`SERVICE_TABLE`]（名字 → 行为码）表驱动地
  `Runtime::task_create` + 映射独立栈段；**入口由管理器自己解析**代码段 ELF 头
  （`e_entry` 在文件偏移 24..32，未被装载器覆写）得到——不再来自 init 的字面量。
  创建者从自身 `self_cap` 派生指向自己的 `TaskSend` 凭据作端点 `initial_cap0`，
  从代码段派生只读令牌作 `initial_cap1`（非零 = 端点角色）。
- 任务 **id 顺序不变**：`servicemgr` 8 → 客户端 9 → `ALPHA` 10 → `BETA` 11
  （init 仍先建 servicemgr 与客户端，管理器随后建两端点）。

加一个服务从此在该片内 = **表里一行 + 一个二进制 + init 移交一条 cap**（最后一项尚未消除，
见 §7）。

## 2. 判决性见证

`servicemgr_ready` 新增三个字段（内核从共享段固定偏移读回，与既有 `self_id`/`marker` 同源）：

- `created`（偏移 272）：管理器**已创建的端点数**，应为 `0x2`；
- `entry_a` / `entry_b`（偏移 304/312）：两个端点**由管理器自解 ELF 头**得到的入口偏移
  （本轮 `0x4a50` / `0x2640`，非零）。

`xtask` 侧同步收紧：`created != 2` 或任一入口为 0 即判失败——「端点任务由 ring3 管理器建立、
且入口来自管理器的解析」是逐字段可核验的，而非仅凭 init 的回执。

`service_selected_ready`（seq 28）**逐字节不变**（`alpha_ep=0xa;beta_ep=0xb;beta_self=0xb;
alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x9;client_stack_seg=0x18;reg_a_name/reg_b_name`
全部相同）——迁移没有改变注册表/解析/路由的可观察结果。

## 3. 逐条登记的既有事件变化

按「既有 QEMU 事件默认逐字节冻结、确需变化逐条登记」纪律，本片实际变化如下（其余 36 个事件
逐字节不变）：

1. **seq 9 `bootstrap_ready`**：`image_len` `7156768` → `7159304`（内核映像随本片源码增大）。
2. **seq 23 `ipc_ready`**：`queued` `10` → `16`（init 新增 6 次 `SYS_SEND`：3 条无 cap 唤醒 +
   3 条带 cap 的建端点移交；`direct`/`blocks` 不变）。`xtask` 期望值同步改为 `16/14/18` 并在
   错误文案里登记原因。
3. **seq 27 `service_negotiated_ready`**：`token` `0x200000033` → `0x10000003c`
   （端点令牌改由**管理器**从代码段派生；`service`/`stack`/`svc_id`/`forward`/`done`/
   `negotiated`/`negatives` 全部不变）。内核该字段改读管理器写回的共享段偏移 `280`/`288`。
4. **seq 29 `servicemgr_ready`**：**新增** `created=0x2;entry_a=0x4a50;entry_b=0x2640`。
5. **seq 31 `preempt_ready`**：`rsp1/rsp2/kstack1_top/kstack2_top` 顺移
   （`0x10000366348/…` → `0x10000367028/…`）——内核 BSS 因 `MAX_VSPACE_ENTRIES` 扩容而位移。
6. **seq 37 `frames_ready`**：`segment` `51` → `53`、`free` `29537` → `29535`
   （两帧自空闲池转入段；`conserved=1;disjoint=1` 守恒与不相交不变量仍成立）。
7. **seq 38 `self_root_ready`**：`pages` `918` → `919`（自建内核根多复刻一页）。

## 4. 其他登记

- **`MAX_VSPACE_ENTRIES` 64 → 80**（`kernel/src/bootstrap.rs`）。本片改动后，boot 末段的
  vspace 逻辑条目数触到旧上限 64，使 `init` 收下共享段只读副本的 `memseg_map` 以
  `CapacityExceeded` 失败。该常量注释本就写明「存储已移出 `BootstrapState`，可自由扩容而不破
  13 KiB 软守卫；取值与 `MAX_CAPABILITY_SLOTS` 对齐」——故按设计意图扩到与
  `MAX_CAPABILITY_SLOTS`(=80) 对齐，而非放宽任何校验。扩容同时改变了内核 BSS 布局，
  即第 3 节第 5 条的地址位移来源。
- **端点角色标记 `initial_cap1` 的语义冻结**：仍为「非零 = 端点角色」（`prima-user`/`prima-beta`
  入口据此分支），只是派生者由 init 改为管理器。两个端点映像**未改动**。

## 5. 定位到的两处真实缺陷（实现过程中）

1. **非规范地址笔误**：`prima-runtime::protocol::ENDPOINT_STACK_VA` 初写成 20 位十六进制字面量
   （`0x0000_0B00_0000_0010_0000`），截断后端点 `stack_top` 落在**非规范地址**；端点用户代码
   `receive()` 把栈上 out 缓冲指针交给内核后，内核 `write_volatile::<u64>` 对该非规范地址写入
   触发 `#GP` → `#DF` → 三重故障。**症状**（RIP 落在无关函数、寄存器垃圾）由下述机制放大，
   一度难以定位。修正为 4 组写法 `0x0000_0B00_0010_0000`（= 内核 `USER3_STACK_VA`）。
2. **内核 `#PF` 分派器对任何非 COW 缺页一律按固定 3 字节跳过故障指令**
   （`kernel/src/cow.rs::dispatch` 返回 `rip + FAULT_SKIP_LEN`，而 `FAULT_SKIP_LEN=3` 只对
   探测用的 `mov qword [rax], rax`（3 字节）成立）。**任何**杂散缺页都会让内核从中途继续执行，
   把一次可报的故障伪装成控制流损坏。本片未改该行为（属既有切片 C/E 的探测语义），但它是
   调试期的主要迷惑源；已在 §7 登记。

## 6. 验证（2026-10-14）

- `cargo xtask build`：PASS（kernel / image bios / image uefi）。
- `cargo xtask test`：PASS（两次 BIOS 运行的事件流校验 + stage1 boot slice）。
- `cargo test --workspace`：PASS（610 项）。
- `cargo xtask lint`：PASS（fmt + `clippy -D warnings` + 内核 `deny(unsafe_code)` 与
  「机制层零 `unsafe`」扫描）。
- QEMU 直跑：`boot_complete`，退出码 33（成功）。

## 7. 未决与登记

1. **「加一个服务」仍需改 init**：本片的表在管理器里，但代码段/栈段 cap 仍需 init 移交，
   故新增服务要动 `kernel/src/user.rs`。真正「表里一行 + 二进制」需要把**代码段派生**
   （从映像根 cap 派生 `{SegMapR,SegMapX}`）与**栈段自建**（`SYS_MEMSEG_CREATE`）也搬进管理器，
   是后续片。
2. **`#PF` 分派器的固定 3 字节跳过**（§5.2）未改；建议后续片把「非已注册 COW 区域的缺页」
   改为显式诊断/失败，而不是静默跳过——它是本次调试耗时的直接原因。
3. **`frames_ready.segment` +2 的具体归属**未逐帧溯源；`conserved`/`disjoint` 不变量成立，
   故不阻塞本片，但登记为待复核项。
4. 本片**不勾选** S4-D2/D3 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
