# SPEC-114：第一个 ring3 用户态驱动（`prima-driver`，S4-D4 第二步）

**日期**：2026-10-15
**阶段**：阶段 4（S4-D4 第二步，承接 SPEC-113 的内核侧机制）
**基线**：`8c148d6`

## 目标

SPEC-113 落了 `Device(IOPortRange)` 对象与受控 I/O 端口路径的**内核侧**，但「无 ring3 调用者」。
本片接入**第一个用户态驱动任务**：由 ring3 服务管理器铸出覆盖 COM1 的 `Device(IOPortRange)` cap、
**运行期移交**给驱动任务，驱动据此经受控端口路径做一次真实设备往返 + 一次越界探测，内核侧逐
字段见证。

驱动是服务表的**第 6 项**（注册名 `ioprt`），与三个基础服务走同一条表驱动创建路径；但它**不**参与
§15 的定序状态探针，而是走自己的「受控端口握手」。

## 判决性证据

`servicemgr_ready`（seq 29）新增五个字段，全部由内核按共享段偏移读回，ring3 无法伪造：

```
created=0x6                      ← 服务表 6 项全部由管理器建立
driver_id=0xf                    ← 驱动是真实 ring3 子任务（id 15）
driver_mark=0x494f500004         ← 驱动自报的**编译进独立二进制**的存在标记
driver_dev=0x10000006a           ← 持有的内核 Device 句柄，is_live_device 认其为活对象
driver_io0=0x5ec900060000005a    ← 区间内往返：读回值低 32 位 = 0x5A（写入的魔数）
driver_io1=0x5ec9000600000000    ← 区间外探测：低 32 位 = 0（内核拒绝；1 表示受控路径失效）
```

判定条件（`ServicemgrWitness::evidence_ok`，`kernel/src/user.rs`）：
`created == 6`、`driver_id > 1` 且与 log/ns/store 互异、`driver_mark == SERVICE_MARKER_DRIVER`、
`is_live_device(driver_dev)`（`bootstrap::device_of_cap` 认其为活 `Device`）、
`driver_io[0] & 0xffffffff == IOPORT_MAGIC_READBACK`、`driver_io[1] & 0xffffffff == 0`。

`driver_io0` 是**受控路径的硬证据**：内核只在端口落在该 `Device` 区间内时才放行 `outb`/`inb`，
故读回魔数同时证明「设备往返成立」与「区间确实覆盖 `0x3FF`」。

## 设计

| 环节 | 实现 |
|---|---|
| 服务表第 6 项 | `prima-user/src/servicemgr.rs` 的 `SERVICE_TABLE` + `SERVICES 5→6`、`DRIVER_SLOT=5`、`DRIVER_PROBES=2` |
| 注册名/行为码/标记 | `prima-runtime::protocol` 的 `SERVICE_NAME_DRIVER="ioprt"`、`SERVICE_BEHAVIOR_DRIVER=6`、`SERVICE_MARKER_DRIVER` |
| 铸设备 cap | 管理器 `SYS_DEV_IOPORT_CREATE(0x3F8, 8)`（`Device(IOPortRange)` 覆盖 COM1） |
| 运行期移交 | `SERVICE_DEV_CAP_TAG`（带 cap；前置一条无 cap 唤醒，接收方可能阻塞在 `recv`） |
| 受控端口请求 | `op=3` 区间内往返（写 `0x3FF` 魔数 `0x5A` 再读回）、`op=4` 区间外探测（写 `0x2F8` 须被拒） |
| 驱动映像 | 新二进制 `prima-user/src/driver.rs`（`prima-driver`），装载进服务映像段第 6 项 |
| 内核见证 | `kernel/src/protocol.rs` 的 `servicemgr_ready` 追加 5 字段；`bootstrap::device_of_cap` 认活设备 |

**最小权限**：驱动初始只持 `TaskSend`（指向管理器）+ 代码段只读角色令牌；唯一的设备权威是运行期
收到的那一份 `IOPortRange` 副本。它**不**参与 §15 探针，也不并入 `served` 计数（`services` 语义不变）。

## 过程中修掉的三处卡点（本片的主要工作量）

### 卡点 1：`MAX_VSPACE_ENTRIES` 96 → **128**（实测真实用量 98）

加入第 6 个服务（第 7 个用户任务）后，boot 末段逻辑映射条目达 **98**，越过旧上限 96 →
`TaskVspaceTable::insert` 返回 `CapacityExceeded`，后续 `memseg_map` 失败。**症状极具误导性**：
内核按 `USER_HELLO2_VA` 读共享段**全部读成「地址」**（`content(x) == x`），
`service_reply_ready.map` 变 `ENOENT`——像是共享段丢失，实为映射条目耗尽。

诊断法：临时给 `BootstrapState` 加 `debug_vspace_count()` 打印 `vspace.count`（上限 96 时卡在
`0x60`；上限 400 时真实值 `0x62` = 98 ⇒ **真实增长而非泄漏**，`teardown_vspace` 确有 `vspace.remove`）。

### 卡点 2：驱动任务**被创建却从不执行**——最小 id 调度器的饿死

`created=6` 证明 `task_create` 成功，但 `driver_*` 字段恒为 0。**诊断过程**（串口打印有观察者效应，
改用 QEMU 退出码编码探针）：

- 临时在 `resume.rs` 编入 `task_status(15)` → 退出码 `0x41` ⇒ 驱动恒为 **`Ready`**（不是 `Faulted`）。
- 临时在 `sched::pick_next` 记录「曾被选中的 id 位集」→ 退出码 `0x3f` ⇒ id 9..14 都被选中过，
  **15 从未**。
- 结论：`pick_next` 取**最小 Ready id**，驱动 id 最大（服务表末位创建）；管理器每条消息处理完都跑
  一遍事件循环尾部，若不等驱动应答就**在同一轮迭代内**继续把 `PONG`/`FINISH` 发完并阻塞，随后
  init（id 1）成为最小 Ready 任务——驱动再不被选中。

**修法**：把「收到驱动的两条受控端口应答」（`reply_count[DRIVER_SLOT] >= DRIVER_PROBES`）并入收尾
条件 ⑥。管理器于是在等待期间只是**阻塞在 `receive`**，驱动成为唯一 Ready 任务而被调度。
反例（实测）：只把收尾条件挂在「驱动**自报**」上时管理器仍在同一迭代收尾，`driver_io0/io1` 恒为 0。

### 卡点 3：任务状态审计环 512 → **768**（实测峰值 530）

驱动真正跑起来后，抢占阶段第二次 `switch_to` 触发内核 panic。用退出码探针定位：首个抢占 tick 时
`state_audit_ready.used` 已达 **509**，两次 `switch_to` 的 `yield_task`/`schedule_task` 各需 1 条，
越过 512 → `AuditFull` → `sched::switch_to` 的 `.expect` 变成 panic。

驱动的参与本身净增 36 条迁移事件（`total 135→146`；它自身两次被调度
`Ready→Running→BlockedRecv`，**并**令管理器多出若干 `Running↔BlockedRecv` 往返）。按本常量既有的
扩容先例（128→170→256→512，每次均附实测），提到 **768**（峰值 530，留 ~45% 余量）。该容量不占
`BootstrapState`（存储在 `AUDIT_ARENA` 静态），不受 16 KiB 守卫约束。

## 逐条登记的既有事件变化

既有 QEMU 事件默认逐字节冻结；本片**确需变化**者逐条登记如下（均已同步 `xtask/src/protocol.rs` 夹具）：

| 事件 | 旧 | 新 | 原因 |
|---|---|---|---|
| `ipc_ready.queued/direct/blocks` | 38/23/30 | **44/26/34** | 驱动自报（唤醒=direct、id/标记=queued）、管理器移交设备 cap 与两条请求、驱动两条应答，及各任务额外阻塞 |
| `task_ready.tasks`/`tasks_after` | 10 | **11** | 存活的第 6 个服务端点（用户态驱动） |
| `task_ready.caps`/`caps_after` | 102 | **110** | 驱动的两条初始 cap + 自建栈段根 cap + 自管理/结构性根 cap + 两次运行时各一条派生 `TaskSend` |
| `servicemgr_ready.created` | 5 | **6** | 服务表第 6 项 |
| `servicemgr_ready` 字段 | — | **+driver_id/driver_mark/driver_dev/driver_io0/driver_io1** | 本片见证 |
| `state_audit_ready` | total=135, used=494, cap=512 | **total=146, used=530, cap=768** | 驱动引入的迁移事件 + 容量扩容 |
| `frames_ready` / `self_root_ready` | image=1169 / pages=436 | **image=1177 / pages=444** | 服务映像段加入第 6 个 ELF 后随内核/映像尺寸位移 |
| `ns_resolved` 值 | `…0x62` | `…0x68` | 随 Capability 分配位移（形状判定不变） |

其余事件（`boot_start`..`preempt_ready`、`timer_ready`、`authority_ready`、`fault_ready`、
`platform_segments_ready`、`reclaim_ready`、`resume_ready`、`boot_complete` 等）逐字节不变。
QEMU 实时序列仍为 **43 个事件**，退出码 **33**。

## 改动

| 文件 | 内容 |
|---|---|
| `prima-runtime/src/protocol.rs` | `SERVICE_NAME_DRIVER`、`SERVICE_MARKER_DRIVER`、`SERVICE_BEHAVIOR_DRIVER=6`、`SERVICE_DEV_CAP_TAG`、`SERVICE_SERVE_OP_IOPORT_{RW,OOR}`、`IOPORT_{BASE,COUNT,SCRATCH,OUT_OF_RANGE,MAGIC}` |
| `prima-user/src/driver.rs`（新） | 驱动二进制：自报注册名/标记 → 收 `Device` cap → 两条受控端口应答 |
| `prima-user/src/servicemgr.rs` | `SERVICES 5→6`、`DRIVER_SLOT`/`DRIVER_PROBES`、驱动握手块 ⑤''、收尾条件 ⑥ 并入驱动应答、`served` 只计基础服务 |
| `prima-user/Cargo.toml` | `[[bin]] prima-driver` |
| `kernel/src/user.rs` | `PRIMA_DRIVER_ELF` 装载、目录第 6 项、`ServicemgrWitness` 五个驱动字段 + 判定、`created == 6` |
| `kernel/src/protocol.rs` | `servicemgr_ready` 追加五字段 |
| `kernel/src/bootstrap.rs` | `device_of_cap`、`MAX_VSPACE_ENTRIES 96→128`、`MAX_AUDIT_EVENTS 512→768` |
| `xtask/src/protocol.rs` | 夹具与判定同步（含新增 5 字段的解析与校验） |

## 验证

- `cargo xtask test`：**6/6 PASS**（QEMU BIOS 两次运行 + stage1 boot slice）
- `cargo xtask lint`：**4/4 PASS**（`cargo fmt --check`、`clippy -D warnings`、`deny(unsafe_code)`、机制层零 `unsafe`）
- `cargo test --workspace`：**795 passed, 0 failed**
- `cargo xtask tcb`：`trusted=1262 mechanism_lines=21580 total=22842 ratio=5.52%`，机制层 `unsafe=0`
- QEMU 实测：43 个事件、`servicemgr_ready status=ok`、`boot_complete ok`、退出码 33

**诊断探针已全部移除**（`kernel/src/{resume,preempt,sched}.rs` 三个文件对 HEAD 零差异）。

## 未决

1. **硬件 IOPL/TSS bitmap 路径（§15.5）**：仍走内核**代执行** `outb`/`inb`；使 ring3 自执行端口
   指令由硬件按 per-Task bitmap 仲裁是后续片（需动 GDT/TSS 与任务切换、处理越权 `#GP`）。
2. **`dev_destroy`（§15.8）** 与 MMIO/DMA/IRQ 三种 `DeviceKind` 仍未实现（承 SPEC-113 登记）。
3. **驱动的接入方式仍是「服务表第 6 项」**：它借用了服务创建路径（`task_create` + 栈映射都在管理器
   内），尚无独立的驱动装载/发现机制；`SERVICE_BEHAVIOR_DRIVER` 与 §15 的 `DeviceKind` 命名体系
   未完全对齐（`DeviceKind` 目前在 kernel 侧只落 `IOPortRange`）。
4. **驱动只做一次往返即阻塞**：无中断/轮询循环、无多驱动实例、无驱动崩溃回收路径。
5. 本片**不勾选**任何阶段 4 交付项/退出条件；闭合判断交复核者。
