# 切片 U 报告：平台段 Capability 按属主移交（ISSUE-AUTH-002 剩余项）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-17
>
> 对象：`kernel/src/{bootstrap,user,syscall,protocol,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 `ISSUE-AUTH-002` 的剩余项——把平台段 / Task Capability 从内核持有改为
> **用户可达**，并登记解读分歧与全部偏差

## 0. 结论速览

- **用户可达已建立**：11 条用户可见平台段根 Capability 与 2 条平台 Task Capability 由
  `HeldByKernel` 按属主移交给 task 1 / task 2；`SYS_MEMSEG_MAP` 的
  `authorize(caller, cap, …)` 第 3 步（`holder_of(cap) == caller`）在**用户侧**首次成立。
- **判决性证据来自 ring3**：task 1 用它**自持**的 `SEG_TASK1_X` cap（槽 36）+ 自持的
  `Task(1)` 管理 cap（槽 45）调用 `SYS_MEMSEG_MAP` → 返回 `0`，**读回该映射首 qword =
  `0xc0ffee0000000000`**（X 段共享帧的 COW 模式值，证明 PTE 真指向该 Segment 的帧，而不是
  「调用返回 0」）；同一请求改用 **task 2 所属**的段 cap（槽 38）→ `EACCES`。
- **范围收窄已登记**：`SEG_VSPACE`(id 2) / `SEG_COW`(id 3) 保留内核持有。理由：两者
  `allowed_access` 为空（`memseg_map` 对其必然 `RightsExceeded`），且根 cap 带 `SegDestroy`
  ——交给不可信用户等于授权销毁内核自建映射与 COW 私有帧区。因此「平台区域**全部**用户可达」
  这一解读**不成立**，交复核者裁定（见 §5）。
- **不勾选 `ISSUE-AUTH-002`、任何交付项与阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，**不是组织级第三方审查**，不构成 `REFINEMENT-PROOF`，
  不改变阶段 0/1/2 的门槛。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| 属主表 | 新增 `PLATFORM_SEGMENT_OWNERS: [(u32, u32); 11]`（`bootstrap.rs`）：id 4/5/6/7/11/12 → task 1，id 8/9/10/13/14 → task 2 |
| 移交 | 新增 `BootstrapState::handover_platform_caps()`，复用既有 `move_capability(cap, KERNEL_ACTOR, owner)`（同槽同 generation，句柄不变，审计记 `from = HeldByKernel`）；11 条段 cap + 2 条 task cap |
| 调用时机 | `main.rs`：在 `user::prepare_task_create` 之后、`configure_fast_path`/`activate_user` 之前。**必须在 bring-up 全部 `map_region` 之后**——那些映射以 `KERNEL_ACTOR` 为 caller，移交后内核不再持有这些 cap |
| 校验 | `platform_cap_ok` 拆为 `platform_cap_holder`（返回属主）：位置放宽为 `HeldByKernel` **或** `HeldByTask(存活)`；槽 Live、目标匹配、权限非空且 well-typed 三条保持 |
| 见证口径 | `PlatformSegmentsRecord` 新增 `user_roots`/`kernel_roots`/`holders[13]`/`task_holders[2]`；`roots` 语义不变（通过校验的根数 = 13） |
| 协议 | `platform_segments_ready` 删硬编码 `holder=`，改 `holders=`（逐槽）与 `user_roots`/`kernel_roots`/`task_holders`；`vspace` 11→12 |
| ring3 | `user_code()` 追加「自持 cap 映射」块（2458..2636，`USER_CODE_LEN` 2459→2637）：正例 → 读回 → 非属主负例 → 三条回执写 Y 页 448/456/464 |
| 新映射 VA | `USER_MAP_WITNESS_VA = USER_BASE_VA + 0x4000`（与既有映射同处一个 2 MiB 区域，**复用已存在的 PT 页**，故不新增页表帧——`frames_ready.table` 实测不变） |

### 2.1 移交为什么必须是「按属主」

`move_capability` 的目标位置是 `HeldByTask(owner)`，而 `authorize` 第 3 步要求
`holder_of(cap) == caller`、`platform_cap_holder` 又要求 `HeldByTask` 的目标 Task **存活**。
故属主必须是真实存在的 Task：13 个区域里有 6 个服务 task 1 的地址空间
（TASK1 CODE/STACK/X/Y 与 SPIN1 CODE/DATA）、5 个服务 task 2，剩余 2 个是内核私有。

## 3. 实测证据（2026-09-17）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30705;image=771;free=30705;regions=3;kstart=0x1000000;kend=0x1303000
PRIMA|v=1|seq=24|event=task_ready|status=ok|data=stack_seg=0x100000009;create=0x10000000a;bad_entry=0xffffffffffffffea;no_x=0xfffffffffffffff3;stack_map=0x0;recv=0x3;deny=0xfffffffffffffff3;destroy=0x0;again=0xfffffffffffffff3;map_dead=0xfffffffffffffffe;map_own=0x0;map_value=0xc0ffee0000000000;map_other=0xfffffffffffffff3;child=3;tasks=2;caps=28;root_cleared=1
PRIMA|v=1|seq=25|event=preempt_ready|status=ok|data=ticks=4;pre1=2;pre2=2;root1=0xc000;root2=0x15000;cr31=0xc000;cr32=0x15000;rsp1=0x1000007ccf8;rsp2=0x10000080cf8;kstack1_top=0x1000007cd20;kstack2_top=0x10000080d20;data1=0x1e000;data2=0x22000;mark1=0x1;mark2=0x2
PRIMA|v=1|seq=27|event=platform_segments_ready|status=ok|data=segments=13;first_id=2;last_id=14;roots=13;user_roots=11;kernel_roots=2;holders=0,0,1,1,1,1,2,2,2,1,1,2,2;task_caps=2;task_holders=1,2;mapped=11;direct=2;direct_ids=2,3;vspace=12;slot_base=32;task1_cap=0x10000002d;task2_cap=0x10000002e;caps=0x100000020,...,0x10000002c;frames=2,3,1,1,1,1,1,1,1,1,1,1,1;unified=1
PRIMA|v=1|seq=28|event=authority_ready|status=ok|data=caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=0x100000002;init_running=1
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31476;free=30661;segment=19;table=25;image=771;conserved=1;disjoint=1
```

解读：

- `holders=0,0,1,1,1,1,2,2,2,1,1,2,2`：按 id 2..14 顺序，前两项（VSPACE/COW）为 `0` =
  `KERNEL_ACTOR`，其余按属主；`user_roots=11` + `kernel_roots=2 = roots=13`。
- `map_own=0` / `map_value=0xc0ffee0000000000` / `map_other=EACCES`：ring3 三条回执。
  中间一项是「映射真的指向该 Segment 帧」的证据——它等于 X 段共享帧的 COW 模式值。
- `vspace=12`：比切片 R 的 11 多一条，即 ring3 用自持 cap 建立的 `USER_MAP_WITNESS_VA` 映射。
- `table=25` **未变**：新映射与既有映射同处一个 2 MiB 区域，复用已存在的 PT 页（设计时预测，
  实测证实）。
- `authority_ready`（`caps=3;segments=1;namespaces=1;…`）逐字段不变：移交只改位置，不改
  初始 Authority 的对象与权限集合。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **274**、xtask 149 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，30 条事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| `platform_segments_ready` | 删 `holder=0`；新增 `user_roots=11`/`kernel_roots=2`/`holders=`/`task_holders=1,2`；`vspace` 11→**12**（ring3 新映射）。`roots=13`/`task_caps=2`/`mapped=11`/`direct=2`/`direct_ids`/`caps`/`frames`/`unified`/`slot_base`/`task1_cap`/`task2_cap` 均不变（`move` 保持同槽同 generation） |
| `task_ready` | 新增 `map_own=0x0`/`map_value=0xc0ffee0000000000`/`map_other=0xfffffffffffffff3`；其余 13 个字段逐字段不变（含 `child=3`/`tasks=2`/`caps=28`/`root_cleared=1`） |
| `memory_ready` | `usable` 30708→**30705**、`image` 768→**771**、`kend` 0x1300000→**0x1303000**（`user_code` +178 字节、新增内核代码与结构体）；`universe = usable + image = 31476` **不变** |
| `frames_ready` | `free` 30664→**30661**、`image` 768→**771**；`universe=31476`、`segment=19`、`table=25` 均不变；`conserved=1`、`disjoint=1` 保持 |
| `preempt_ready` | `rsp1` 0x1000007bd28→**0x1000007ccf8**、`rsp2` 0x1000007fd28→**0x10000080cf8**、`kstack1_top` 0x1000007bd50→**0x1000007cd20**、`kstack2_top` 0x1000007fd50→**0x10000080d20**（镜像增长后整体后移，两任务内核栈仍各自独立）；`root1/cr31/cr32/data1/data2/ticks/pre1/pre2/mark1/mark2` 不变 |
| 其余 24 条事件（seq 1/2/4..23/26/30） | **逐字段不变** |
| `TASK_READY_LIVE_CAPS` | 保持 **28**：移交只改 cap 位置，不改存活槽位数（新映射不铸 cap） |
| `BootstrapState` 尺寸守卫 | 移交的 13 条 `CapabilityMoved` 是**运行时**审计追加，不改结构体尺寸；`size_of::<BootstrapState>()` 守卫测试仍通过 |

以上按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. 边界与反例保留

- **`SEG_VSPACE`/`SEG_COW` 不移交**：见 §0。故 `ISSUE-AUTH-002` 若被解读为「13 个平台区域
  全部用户可达」，本切片只完成 11/13；该解读分歧**交复核者裁定**，不自行宣告闭合。
- **平台路径仍经 `platform_create_segment`**（与 `memseg_create` 共用 `create_segment_object`
  构造核）而非 `memseg_create` 入口——SPEC-063 登记的第二项偏差**未变**，本切片不处置。
- **`move_capability` 无「反悔」路径**：移交后内核不再持有这些 cap，bring-up 无法再以
  `KERNEL_ACTOR` 代用户映射。这是有意的（正是「用户可达」的含义），但登记为一条不可逆的设计约束。
- **`handover_platform_caps` 非幂等**：重复调用会对已移交的槽再次 `move_capability`，因
  `positions.move_to_task` 要求 `from_task` 是当前持有者（此刻已是属主 Task 而非
  `KERNEL_ACTOR`）而返回错误。当前无调用点会重复调用；host 单测钉住单次移交的正确终态。
- **最小权限未主张最优**：task 1 同时持有自己 6 个区域的段 cap 与 `TaskManageVSpace`。
  「用户态是否该持有自己 code/stack 的映射权」属后续复核面，本切片只建立「可自持并使用」的机制。
- 未实现：`dev_vspace`/W16.4、`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。
- 既有 `cargo xtask test` 的 30s/无网络/精确序列语义不变。

## 6. 关闭口径与下一步

- **不勾选 `ISSUE-AUTH-002` 任何状态、不勾选阶段 2 交付项与退出条件、阶段 2 维持「进行中」**：
  证据为真实内核 + QEMU，但独立性仍为 harness 内，需范畴外第三方复核——与阶段 0/1 同一门槛。
- 下一步（按已确认的切片划分）：
  1. **SPEC-067 / 切片 V：Task 自毁**（`ISSUE-TASK-002`）——延迟释放队列、`sched::exit_to`、
     task4 承载 QEMU 见证；
  2. 两切片的范畴外复核；
  3. 阶段 3 起点（Namespace/Timer/Audit）。
