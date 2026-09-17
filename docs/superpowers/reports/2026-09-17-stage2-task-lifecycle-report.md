# 切片 T 报告：Task create/yield/destroy 用户原语与单核调度

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-17
>
> 对象：`kernel/src/{bootstrap,sched,user,syscall,preempt,protocol,resume,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 S2-D4（Task create/yield/destroy 与单核调度），收口退出条件 S2-X1 的实现侧，
> 并登记过程中发现的两处既有缺陷

## 0. 结论速览

- **S2-D4 交付**：新增 `SYS_TASK_CREATE=19`（完整 §8.1：代码段 cap、`load_base`/`entry`/
  `code_offset`/`code_pages`、`parent_ns_cap`、`root_rights`、`initial_caps`，返回 manager cap，
  铸 self cap、派生 `root_ns_cap`、移动 `initial_caps`）与 `SYS_TASK_DESTROY=20`（§8.2：地址
  空间拆除 + 页表帧与根帧归还 + 异常订阅移除 + cap/消息清理 + `TaskDestroyed`）。
  `SYS_YIELD=8`（§8.3）自切片 M 起已用户可调，本切片把它纳入 S2-D4 的证据面。
- **QEMU 主见证 `task_ready`（seq 24）**：
  `stack_seg=0x100000009;create=0x10000000a;bad_entry=EINVAL;no_x=EACCES;stack_map=0x0;
  recv=0x3;deny=EACCES;destroy=0x0;again=EACCES;map_dead=ENOENT;child=3;tasks=2;caps=28;
  root_cleared=1`。
- **子任务真的跑过**：`recv=0x3` 是子任务自己经 `SYS_TASK_ID` 拿到 id 后**经授权消息回发**的
  payload；`destroy=0` 证明 manager cap 在其存活期间通过了 `TaskDestroy` 授权。
- **销毁是可复查的**：`again`（重复销毁）与 `map_dead`（向已销毁任务映射）均被拒；
  `root_cleared=1` 由内核在事件发射时读取 `sched::root(child)`；`tasks=2`（init + task 2）证明
  子任务已不在任务表中；`caps=28` 证明子任务的 self/root_ns cap 已随销毁撤销而调用者的 manager
  cap 仍在。
- **两处既有缺陷被本切片暴露并修复**（见 §4）：8259 未屏蔽导致 IRQ0 以 vector 8（`#DF` 门）
  交付而三重故障；`trap_call` 的直投未消费 `pending`，留下能被后续 `SYS_RECV` 取到的幽灵消息。
- **不勾选任何交付项与阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛，见 §6）。

## 1. 方法与独立性声明

- 全部为真实内核实现 + QEMU 实测 + host 单测；无机器证明。
- 本报告复核独立性为 **harness 内会话级别**，**不是组织级第三方审查**，也不构成
  `REFINEMENT-PROOF`；不改变阶段 0/1/2 的独立性门槛。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| 审计 | 新增 `AuditEvent::TaskCreated`（§8.1 步骤 8）；`task_destroy` 前移容量预检以保持原子 |
| 授权 | 新增 `derive_capability_to_holder`（§8.1 步骤 6：子 cap 落在**指定**持有者）；`MANAGER_TASK_RIGHTS = {Derive,TaskSend,TaskManageVSpace,TaskDestroy}` |
| `TaskTable` | `create_with_entry`（步骤 1 写入入口 VA）、`next_free_id`、`set_root_ns_cap`、`remove`（仅回滚用）、`is_live` 公开 |
| `task_create` | 前置 1–8 与容量/审计预检先做；取根表帧 → `clone_root` → 清空槽 20/21/22 → 逐页 `{R,X}` 用户映射并记入 `TaskVspaceTable` → 建记录 → 铸 manager/self → 派生 root_ns → 移动 initial_caps → `TaskCreated`；任一提交步骤失败按已提交量回滚（退役 cap、删记录、归还表帧与根帧） |
| `task_destroy` | 先删目标全部页映射（`vspace::unmap_4k` + 归还空表帧）、归还目标根帧，再走既有的消息/cap/记录清理，新增 `ExcSourceTable::remove_subscriber` |
| 调度 | `sched::clear_task`（root/kstack/frame 归零）；`user::register_created_task`（登记 root、内核栈、初始 `SretFrame`） |
| 内核栈 | 新增按 task id 索引的静态内核栈数组（id 3..8，6×16 KiB）；task 1/2 仍用既有静态栈，故 `preempt_ready` 的 `rsp*`/`kstack*_top` 语义不变 |
| bring-up | 建新 Task 的代码段（**用户区 id 15**，内核写入 `task3_code()`）并铸槽 49 的 `{Derive,SegMapR,SegMapX,SegDestroy}` cap 给 task 1；铸槽 50 的 `Task(1)`+`TaskSend` cap 作为 `initial_cap1` 移入子任务 |
| syscall | `SYS_TASK_CREATE`（`rdi=req_ptr`，10 qword 请求块）、`SYS_TASK_DESTROY`（`rdi=task_cap`）、`task_create_errno`/`task_destroy_errno`、`TaskWitness` + 内核侧事实 |
| ring3 | task 1 `1795→2459` 字节（新块：建栈段 → §8.1 create → 两个创建负例 → 把栈映射进**子任务**根 → 两次让出 → `SYS_RECV` → 三个销毁负例/正例 → 唤醒 task 2）；task 2 `472→496` 字节（尾部改为阻塞接收循环）；新增 `task3_code()` 134 字节 |
| 见证 | 新事件 `task_ready`（seq 24）；`preempt_ready`..`boot_complete` 24..29 → 25..30 |
| 中断 | `preempt::mask_all_irqs`（进入 CPL3 前屏蔽 8259），见 §4 缺陷 1 |

### 2.1 细化层字段的显式声明

`SYS_TASK_CREATE` 的请求块含 `stack_top`——抽象模型 §5.2 的 Task **没有 SP**，它是细化层必须
补的自由度，**不冒充规范字段**。§8.1 步骤 2 只把**代码**装进新 Task 的 vspace，而 §20.3 又把
「新 Task 的初始代码映射」列为唯一免于 `TaskManageVSpace` 检查的映射；因此栈**不在 create 内
映射**，而是由调用者用 create 返回的 manager cap（含 `TaskManageVSpace`）经 `SYS_MEMSEG_MAP`
完成——授权记账完整，且调用者在让出新任务前必须完成该映射。

### 2.2 调度顺序的一处非平凡约束

`pick_next` 取**最小 Ready id**。子任务 id 为 3，而 task 2（id 2）在切片 S 的序列结束时是
`Ready`，因此 task 1 的一次让出会先调度 task 2 而不是新任务。处置：task 2 的代码尾部改为
**阻塞接收循环**（被唤醒后再次阻塞），task 1 在 create 块里**让出两次**（第一次把 task 2 排空成
`BlockedRecv`，第二次才轮到子任务），并在块末**直投唤醒 task 2**，使收尾的时钟抢占阶段仍有
`Ready` 任务可轮转（否则 `pick_next` 会返回 `None`）。这使 `ipc_ready` 的
`queued/direct/blocks` 由 `1/1/1` 变为 `2/2/2`（新增一次排队投递、一次直投、一次阻塞），已按
维护规则 5 显式登记。

## 3. 实测证据（2026-09-17）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30708;image=768;free=30708;regions=3;kstart=0x1000000;kend=0x1300000
PRIMA|v=1|seq=24|event=task_ready|status=ok|data=stack_seg=0x100000009;create=0x10000000a;bad_entry=0xffffffffffffffea;no_x=0xfffffffffffffff3;stack_map=0x0;recv=0x3;deny=0xfffffffffffffff3;destroy=0x0;again=0xfffffffffffffff3;map_dead=0xfffffffffffffffe;child=3;tasks=2;caps=28;root_cleared=1
PRIMA|v=1|seq=27|event=platform_segments_ready|status=ok|data=segments=13;first_id=2;last_id=14;roots=13;task_caps=2;mapped=11;direct=2;direct_ids=2,3;vspace=11;holder=0;slot_base=32;task1_cap=0x10000002d;task2_cap=0x10000002e;caps=0x100000020,...,0x10000002c;frames=2,3,1,1,1,1,1,1,1,1,1,1,1;unified=1
PRIMA|v=1|seq=28|event=authority_ready|status=ok|data=caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=0x100000002;init_running=1
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31476;free=30664;segment=19;table=25;image=768;conserved=1;disjoint=1
PRIMA|v=1|seq=30|event=boot_complete|status=ok
```

解读：

- `stack_seg=0x100000009`：调用者新建的栈段 cap（槽 9 gen1，由 `SYS_MEMSEG_CREATE` 返回）；
  `create=0x10000000a`：`task_create` 铸给调用者的 manager cap（槽 10 gen1，`HeldByTask(1)`）。
- `bad_entry=EINVAL`：同一请求块只把 `entry` 改成代码段之外 → `InvalidEntry`；
  `no_x=EACCES`：把 `code_seg_cap` 换成只有 `R|W` 的栈段 cap → `MissingRight`。两者都在
  **任何状态修改之前**被拒（host 单测另断言状态未变）。
- `stack_map=0`：栈经 manager cap 的 `TaskManageVSpace` 映射进**子任务**根（根由 manager cap
  的目标派生，不是当前任务）。
- `recv=0x3`：阻塞让出后子任务运行——它 `SYS_TASK_ID` 得 3，经 `initial_cap0` 移入的
  `Task(1)`+`TaskSend` 凭据把该值发回 task 1。这条链同时证明 §8.1 步骤 7（initial_caps 移动）
  有效：句柄在移动中不变，子任务代码内嵌的就是它。
- `deny=EACCES`：用不含 `TaskDestroy` 的栈段 cap 销毁 → 拒绝；`destroy=0`：manager cap 销毁成功；
  `again=EACCES`：目标已销毁使 manager cap 成为 stale token（`target_live` 假），重复销毁不再
  成功；`map_dead=ENOENT`：向已销毁任务的地址空间映射被拒（`sched::root(child)` 已被清理）。
- `tasks=2`、`caps=28`、`root_cleared=1` 均**由内核在事件发射时读取**，不是从用户回执推断：
  子任务已从任务表移除、其 self/root_ns cap 已撤销、其调度登记已清空。
- `frames_ready`：`30664 + 19 + 25 + 768 = 31476 = universe`，`conserved=1`、`disjoint=1`。
  `segment=17→19` 是本切片 bring-up 的代码段与运行期新建的栈段；`table=24→25` 是新建子任务的根
  页表帧；`image=714→768` 随内核镜像增长（新增静态内核栈数组与 ring3 代码）。
- 未变项：`platform_segments_ready`（13 段、`task_caps=2`、`unified=1`）与 `authority_ready`
  （`caps=3;segments=1`）**逐字段不变**——本切片新增的两条 cap 铸在预留槽 49/50，未动平台 cap。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel 273、xtask 149 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**30 条**事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 过程中发现并修复的两处既有缺陷（显式登记，不隐藏）

### 4.1 8259 未屏蔽 → IRQ0 以 `#DF` 向量交付 → 三重故障

新增用户序列把用户阶段拉长到超过 PIT 上电默认周期（约 55 ms）后，QEMU 出现三重故障。
`-d int` 定位：硬件 `INT=0x08` 被交付，而 8259 上电默认**不重映射**且 IRQ0 未屏蔽，vector 8 正是
IDT 的 `#DF` 门；`#DF` 门按「带错误码」出栈而硬件中断不压错误码，栈立即错位。

- **修复**：`preempt::mask_all_irqs()` 在进入 CPL3 前屏蔽 8259 全部中断；`init_timer` 自己用
  OCW1 只放行 IRQ0（`0xFE`），故时钟抢占阶段不受影响。
- **性质**：这是**既有**的潜在缺陷（此前用户阶段短于 55 ms 而未暴露），不是本切片引入的回归。

### 4.2 `trap_call` 的直投未消费 `pending`

`SYS_TRAP` 的直投路径把 payload 注入被唤醒任务的保存帧 `rax`，但没有像 `send_call` 那样
`take_pending`；那条通知滞留在目标的 `pending` 里，而 `receive_message` **先取 pending**，
于是任务之后任意一次 `SYS_RECV` 都会先拿到这条内核通知。

- **发现方式**：本切片新增的 `SYS_RECV`（期望拿到子任务回发的 id）实际拿到 `0x20002`——
  正是旧 `SYS_TRAP` 通知的 payload。
- **修复**：`trap_call` 在注入 `rax` 后 `take_pending_global(task)`，与 `send_call` 的直投路径
  一致；`exc_wait_ready` 各字段（`blocks=1;woken=1;payload=0x20002;wait_receipt=0x20002`）不变。
- **性质**：同样是既有缺陷，由本切片更长的用户序列与新增接收点暴露。

## 5. 既有事件的变化（显式登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| 事件序列 | 新增 `task_ready`(24)；`preempt_ready` 24→25、`exc_resume_ready` 25→26、`platform_segments_ready` 26→27、`authority_ready` 27→28、`frames_ready` 28→29、`boot_complete` 29→30；`EXPECTED_EVENTS` 29 → 30 条 |
| `memory_ready` | `usable` 30786→**30708**、`image` 714→**768**、`kend` 0x12ca000→**0x1300000**：内核镜像增 54 帧（静态内核栈数组 6×16 KiB + 新增 syscall/模型代码 + ring3 代码 664+24 字节）。`universe = usable + image` 随之 31500→**31476**（镜像区间由 bootloader 从可用帧中划走） |
| `bootstrap_ready` | `image_len` **3143552**、`image_frames`/`free_frames` 随上；三个初始 cap 句柄与三组权限**不变** |
| `frames_ready` | `universe` 31500→**31476**（同上）、`free` 30745→**30664**、`segment` 17→**19**（新代码段 + 新建栈段）、`table` 24→**25**（子任务根页表帧）、`image` 714→**768**；`conserved=1`、`disjoint=1` 不变 |
| `preempt_ready` | `rsp*`/`kstack*_top` 随内核镜像增长整体后移（现值 `rsp1=0x1000007bd28`、`rsp2=0x1000007fd28`、`kstack1_top=0x1000007bd50`、`kstack2_top=0x1000007fd50`：两任务的静态内核栈仍在 BSS 内、保持各自独立）；`root1=0xc000`、`root2=0x15000`、`cr31/cr32`、`data1=0x1e000`、`data2=0x22000`、`ticks=4`、`pre1=pre2=2`、`mark1/mark2` 不变 |
| `ipc_ready` | `queued/direct/blocks` 1/1/1→**2/2/2**（§2.2：新增一次排队投递、一次直投唤醒、一次阻塞）；其余字段逐字段不变 |
| `vspace_ready`/`cow_ready`/`user_cow_ready` | **逐字段不变**（`translated`/`wp_err`、`shared=0x6000`、`private=0xb000`、`frame_a`/`frame_b`、魔数与模式语义均保持原值） |
| `handle_ready`/`cap_lifecycle_ready`/`memseg_ready`/`syscall_*`/`exc_*`/`multitask_ready`/`exc_wait_ready`/`platform_segments_ready`/`authority_ready` | **逐字段不变**（槽 1..8 与全部回执保持原值）——新 cap 落在预留槽 49/50 |
| `user_code` | 1795 → **2459** 字节；`task2_code` 472 → **496** 字节 |

以上均按维护规则 5 记录，**不视为回归、也不隐藏**。

## 6. 边界与反例保留

- **自我销毁未实现**（新登记 `ISSUE-TASK-002`）：`task_destroy` 对 `target == caller` 返回
  `EACCES`，§8.2「删除自身是合法的」字面未满足。理由：自毁要在正被使用的 CR3 与内核栈上回收
  自身根页表帧，依赖「释放与 `write_cr3` 之间不得分配帧」的脆弱时序。host 单测钉住该拒绝路径，
  **QEMU 未走**（子任务代码不调用自毁）。
- **调度路径的 `TaskStateChanged` 未补**（新登记 `ISSUE-TASK-003`）：§13.1/W13 要求的
  `Running↔Ready`、`Running→BlockedRecv`、`BlockedRecv→Ready` 迁移事件仍缺；本切片只补
  `TaskCreated` 与 create/destroy 引发的 `TaskStateChanged`。留 S3-D5（「对象生命周期 Audit」）。
  依据：`BootstrapState` 实测 15240 B、guard 上限 16384 B，余量仅约 47 条 24 B 事件，为调度路径
  补齐事件必须做审计存储重构。
- **`CannotDestroyInit` 与 `CannotDestroySelf` 只由 host 单测覆盖**：用户态没有一条指向 init 且
  含 `TaskDestroy` 的 cap，故 QEMU 见证里没有这两条负例（QEMU 侧的三条销毁负例是
  「缺 `TaskDestroy`」「重复销毁」「向已销毁任务映射」）。
- **`initial_caps` 容量固定 2 条**：§8.1 是列表，本细化层按消息携带 cap 的同一容量实现。
- **`parent_ns_cap` 实际只能是 init 根 Namespace cap**：本切片不建 Namespace 层级，故
  `root_rights` 的衰减只由 `subset_of` 保证。
- **§8.2 步骤 5（Timer）与步骤 6（`pending_exc`/`kernel_resume_frame`）未实现**：依赖尚未存在的
  对象（Timer、`Faulted`，后者即既有 `ISSUE-TASK-001`）。
- **代码段必须由内核 bring-up 写入**：调用者无法把代码写进自己新建的段（那需要自身的
  `TaskManageVSpace`），故新 Task 的代码段由 bring-up 提供并铸 cap 给 task 1。这是务实取舍。
- **新增代码段使用用户区 id 15**：`PLATFORM_SEGMENT_ID_LAST`/`PLATFORM_SEGMENT_COUNT`/平台 cap
  槽 32..44 均未改动，代价是 ring3 既有 `memseg_create` 的 Segment **id** 顺移一位（无任何金标
  断言 id）。
- **`ISSUE-AUTH-002` 的第二项（平台段 cap 的用户可达移交）仍未做**：本切片只把**新**代码段 cap
  铸给 task 1（该 cap 用户可达），13 条平台段 cap 仍由 `KERNEL_ACTOR` 持有。
- **`ISSUE-FRAME-001`**（image Segment 仍为 `Contiguous`）保持 Open。
- 未实现：`dev_vspace`/W16.4、`REFINEMENT-PROOF`、`memseg_map` 的 `dev_vspace` 互斥。
- 既有 `cargo xtask test` 的 30s/无网络/精确序列语义不变。

## 7. 关闭口径与下一步

- **不勾选 S2-D4/S2-X1 任何退出条件、阶段 2 维持「进行中」**：证据虽为真实内核 + QEMU，
  独立性仍为 harness 内，需范畴外第三方复核——与阶段 0/1 同一门槛，不为「收尾」放宽。
- 下一步候选：
  1. 阶段 2 各退出条件（S2-X1..X4）的范畴外复核；
  2. `ISSUE-TASK-002`（自毁）与 `ISSUE-TASK-003`（W13 调度审计）的处置，后者与 S3-D5 同批；
  3. 平台段 cap（槽 32..44）的用户可达移交（`ISSUE-AUTH-002` 剩余项）；
  4. 阶段 3 起点（Namespace/Timer/Audit）。
