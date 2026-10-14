# SPEC-106 报告：S4-D3 三个基础服务（日志 / Namespace / 基础存储）接入 ring3 服务表

> 状态：探索实现；**不构成 S4-D2/D3 验收或稳定 ABI**，不勾选任何阶段 4 交付项/退出条件
>
> 设计依据：[把服务管理器迁到 ring3 Rust](../specs/2026-10-13-stage4-ring3-service-manager-design.md) §2/§3/§6
>
> 前置：[SPEC-105 表驱动端点创建](2026-10-14-stage4-servicemgr-endpoint-creation-report.md)
>
> 基线：SPEC-105 HEAD `a0263d8`

## 1. 本片做了什么

把**服务表从硬编码 2 端点泛化为 5 项**，并接入 S4-D3 要求的三个基础服务。

- **服务表 2 → 5**（`prima-user/src/servicemgr.rs`）：`SERVICE_TABLE` 由 `alpha`/`beta` 扩为
  `alpha`/`beta`/`log`/`nsvc`/`store`，注册表/待创建表/创建标记数组随之为 `SERVICES = 5` 定长；
  `registry[slot]` 的就绪判据由「已收集 2 条」改为「槽 0、1 各有条目」，与基础服务的注册解耦。
- **三个独立 ring3 服务二进制**：`prima-log` / `prima-ns` / `prima-store`（`prima-user/src/
  {log,ns,store}.rs`），三者共用骨架 `prima-user/src/service.rs`（仅注册名/行为码/存在标记三个
  编译期参数相异）。启动时各向管理器自报**注册名 + 自管理凭据**（与 `ALPHA`/`BETA` 同款
  `SERVICE_ID_TAG`）与**编译进自身二进制的 40 位存在标记**（`SERVICE_MARKER_TAG`）。
- **表驱动创建，入口仍由管理器自解**：三者与 `ALPHA`/`BETA` 走**同一条** `create_service`
  路径（`init` 各移交一对「代码段 + 栈段」cap，管理器自解 ELF 头取 `e_entry`）。
- **路由探针**：管理器在三个基础服务都注册后，各发一条 `SERVICE_SERVE_TAG` 探针（无 cap），
  服务以 `SERVICE_SERVE_TAG | kind<<32 | arg` 应答。三个 `kind` 相异，构成「请求经服务表路由到
  了**正确的**服务二进制」的判决性字段。
- **最小权限**：三个服务初始各只持两条 cap——管理器从自身派生的 `TaskSend`（`initial_cap0`）与
  代码段派生的只读角色令牌（`initial_cap1`）；无段、无 Namespace 权限。

任务 id 顺序：`servicemgr` 8 → 客户端 9 → `ALPHA` 10 → `BETA` 11 → `log` 12 → `ns` 13 → `store` 14。

## 2. 判决性见证

`servicemgr_ready`（seq 29，`status=ok`）新增九个字段（内核从共享段固定偏移读回）：

- `created=0x5`：服务表五项**全部**由 ring3 管理器建立；
- `services=0x3` + `log_id=0xc;ns_id=0xd;store_id=0xe`：三个基础服务各自成为**独立任务**且 id 互异；
- `log_mark=0x4c4f470001;ns_mark=0x4e53560002;store_mark=0x5354520003`：三者各由**不同二进制**运行；
- `log_reply=0x5ec9000300000011;ns_reply=…0004…;store_reply=…0005…`：探针经服务表路由后由**正确的**
  服务应答（`kind` 各归其位）。

`entry_a=0x2ed0`/`entry_b=0x2390`（ELF 剥离调试段后重排）仍非零，证明入口来自管理器自解。
`service_selected_ready`（seq 28）除 `client_stack_seg`（见 §4）外逐字段不变——迁移未改变
注册表/解析/路由的可观察结果。

## 3. 逐条登记的既有事件变化

除下列字段外，其余事件逐字段不变（43 条事件序列无增删）：

1. `memory_ready` `usable`→30236 / `image`→1122 / `free`→30236 / `kend`→0x1462000；
   `bootstrap_ready` `image_len`→4592456 / `image_frames`→1122（内核映像**缩小**，见 §5.6）。
2. `ipc_ready` `queued` 16→**28** / `direct` 14→**23** / `blocks` 18→**30**（`init` 的建端点移交由
   3 条增至 6 条服务 = 12 次发送；三个新服务各自的自报 + 探针应答 + 其阻塞 recv）。
3. `task_ready` `tasks` 7→**10** / `caps` 77→**101**（`tasks_after`/`caps_after` 同）。
4. `service_negotiated_ready` `service` 0x3e→**0x44**（服务任务 id 因预先建立的端点/服务数变化）。
5. `service_selected_ready` `client_stack_seg` 0x10→**0x1e**（段 id 顺移）；`alpha_stack_seg`/`beta_stack_seg`
   （0x12/0x11）不变。
6. `servicemgr_ready` `created` 0x2→**0x5**、`entry_a`/`entry_b` 重排、**新增九字段**（§2）。
7. `preempt_ready` `root1/root2/cr31/cr32/rsp1/rsp2/kstack*_top` 顺移（BSS 因多张容量表扩容位移）；
   `ticks`/`pre*`/`data*`/`mark*` 不变。
8. `frames_ready` `universe`→31358 / `free`→30036 / `segment`→69 / `table`→131 / `image`→1122
   （六个新段 + 页面表帧；`conserved=1;disjoint=1` 不变量仍成立）。
9. `self_root_ready` `root`→0x5c000 / `pages`→406 / `translated`→0x1000000。
10. `state_audit_ready` `total` 51→135 / `to_*` 顺移 / `used` 131→475（`cap`=512）。

## 4. 登记：容量与不变量常量（均为「扩展模型」而非放宽校验）

| 常量 | 旧 → 新 | 触发原因 |
|---|---|---|
| `bootstrap::MAX_CAPABILITY_SLOTS` | 80 → **144** | 活动 cap 峰值实测 **95**，旧上限使 `BETA` 后创建的服务 `cap_derive` 以 `CapacityExceeded` 失败 |
| `bootstrap::MAX_SEGMENT_OBJECTS` | 32 → **48** | 六个新段触顶，后续 `memseg_create` `CapacityExceeded` |
| `frames::MAX_SEGMENTS` | 32 → **48** | 与上对齐 |
| `frames::MAX_TABLE_FRAMES` | 128 → **256** | 每个任务根各有一套页表结点，`BETA` 的 `memseg_map` 取页表帧 `NoMemory` |
| `mem::MAX_RECYCLED_FRAMES` | 320 → **512** | 必须 ≥ `MAX_FRAME_OWNERS + MAX_TABLE_FRAMES`(448)，否则归还失败、帧泄漏 |
| `bootstrap::MAX_MESSAGES` | 8 → **32** | `init` 连发 12 条移交消息，8 槽丢尾（父 Namespace 恰在其列） |
| `bootstrap::MAX_VSPACE_ENTRIES` | 80 → **96** | 五服务各贡献「代码 + 栈」映射 |
| `bootstrap::MAX_TASKS` / `sched::MAX_TASKS` | 12 → **16** | 端点/服务占 id 10..14 |
| `user::USER_TASK_KSTACK_COUNT` | 9 → **16** | 覆盖 id 3..18（留 4 个余量） |
| `user::KSTACK_SIZE` | 16 KiB → **32 KiB** | 见 §5.4（内核栈溢出） |
| `prima-*` 的 `debug = false` | 新设 | 见 §5.6（内核映像越引导器区间） |

**`BootstrapState` 守卫（≤ 13 KiB）未放宽**：Segment 对象表移入静态 arena（`SEGMENT_ARENA`，
`ArenaSlice` 句柄，同 SPEC-074 审计 / SPEC-081 ns / SPEC-091 vspace 的取向），结构体由 13.7 KiB
降到 **10.1 KiB**，故 `MAX_CAPABILITY_SLOTS` 才有余量扩容。守卫照实重测通过。

## 5. 实现过程中定位到的六处真实缺陷

1. **`alloc_segment_slot` 是 `&self`（不预留）**：`for (slot, id) in [(a, self.alloc()), …]` 三次
   连续调用返回**同一个**空闲 id，第二、三个段创建必然 `CapacityExceeded`。改为**循环内逐个分配**。
2. **IPC 队列 8 槽丢尾**（§4 表）：`init` 的 12 条移交消息挤入 8 槽队列，父 Namespace 被丢弃，
   管理器 `ns == None` → 表里五条**一条也建不出来**（症状：管理器与全部任务阻塞在 `recv`，
   `boot_complete` 前 `SYS_RECV: no Ready task` panic）。
3. **三处容量墙**（cap 槽 95/96、段表 32、页表帧 128）与一处**回收栈不足**（320 < 448）：症状分别是
   `cap_derive`/`memseg_create`/`memseg_map` 以 `CapacityExceeded`/`NoMemory` 失败。
4. **内核栈栈溢出**（**本次最隐晦的一处**）：`SYS_TASK_DESTROY` 路径上的
   `[0u64; MAX_VSPACE_ENTRIES]` 快照数组随 `MAX_VSPACE_ENTRIES` 扩容（640→768 B），叠加其它增长后
   使 16 KiB 内核栈**向下越界**，覆写相邻 BSS——实测把 `KERNEL_ROOT`/`KERNEL_IMAGE` 清零，使
   `self_root_witness()` 返回 `None` → `resume.rs` 的**非退出** `panic_event()` 分支继续执行，
   表现为「43 条事件序列里 seq 38 由 `self_root_ready` 变成 `panic|error`，但引导仍完成」。
   **定位手法**：在该处打印每个 Task 的 id/状态 + 在两个 `StaticCell` 上打读数标记 + 用 syscall
   号二分（`@@CUL14` = `SYS_TASK_DESTROY` = 20）——最终以「栈大小翻倍即消失」确认。
   **修法**：`KSTACK_SIZE` 16→32 KiB（与 `config.rs` 启动栈 80→256 KiB 的既有取向一致）。
5. **应答常量位段笔误**：`SERVICE_REPLY_{LOG,NS,STORE}` 初写成 `0x5EC9_0000_000K_0011`，
   而 `pack_service_reply` 产生的是 `0x5EC9_000K_0000_0011`（`kind` 在 bit 32..39）——`servicemgr_ready`
   报 `status=mismatch`。
6. **内核映像越过引导器区间**：为嵌入三个新服务 ELF，内核映像由 7.1 MB 涨到 9.8 MB，越过引导器给
   内核预留的界（`0x194f000`），`build_inventory` 报 `InvalidImageRange`。修法：对 `prima-user` /
   `prima-runtime` 关闭 debug 信息（它们不参与内核符号化），每个用户二进制 ≈0.9 MiB → ≈10 KiB，
   内核映像降到 4.6 MB。

**未改而登记**：SPEC-105 §5.2 的 `#PF` 分派器固定 3 字节跳过（`cow.rs::dispatch`）本片仍未改；
本例中它与栈溢出叠加使症状伪装成「控制流损坏」的倾向再次出现。

## 6. 验证（2026-10-14）

- `cargo test --workspace`：PASS（**788 项**：cosir 146 + cosir_state 42 + lean 3 + kernel 369 +
  prima-ostd 21 + prima-runtime 1 + xtask 206）。
- `cargo xtask lint`：四条 PASS（fmt / `clippy -D warnings` / 内核 `deny(unsafe_code)` /
  「机制层零 `unsafe`」27 文件扫描）。
- `cargo xtask tcb`：`trusted_lines=1262 mechanism_lines=21196 total_lines=22458 ratio=5.62%`，
  机制层 `unsafe=0`。
- `cargo xtask test`：六条 PASS（BIOS run=1/2 的事件流校验 + stage1 boot slice）。
- QEMU 直跑：`boot_complete`，退出码 33。

## 7. 未决与登记

1. **「加一个服务」仍需改 init**：表在管理器、段 cap 源仍在内核，新增服务要动
   `kernel/src/{user,bootstrap}.rs`（同 SPEC-105 §7.1）。真正「表里一行 + 一个二进制」须把
   **代码段派生**与**栈段自建**也搬进管理器。
2. **三个基础服务目前是「骨架」**：日志服务只应答探针、不落日志；Namespace 服务不持 ns cap、
   不做真实解析；存储服务不持段、不落数据。本片交付的是**接入**（独立任务 + 独立二进制 +
   最小权限 + 可路由），服务语义是后续片。
3. **`KSTACK_SIZE` 翻倍是治标**：`SYS_TASK_DESTROY` 的栈上快照数组随 `MAX_VSPACE_ENTRIES` 线性增长，
   建议后续把该快照改为静态/arena 存储，去掉这条「扩容即越界」的隐式耦合。
4. **`#PF` 分派器固定 3 字节跳过**（§5 末）仍未改；建议改为对非已注册 COW 区域的缺页显式诊断/失败。
5. `frames_ready.segment` 由 53 增至 69（+16）未逐帧溯源；`conserved`/`disjoint` 不变量成立，
   登记为待复核项。
6. 本片**不勾选** S4-D2/D3 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
