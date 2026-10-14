# SPEC-112：日志/存储服务持内核 MemorySegment 落数据（S4-D3 收口）

**日期**：2026-10-15
**阶段**：阶段 4（S4-D3 收口）
**基线**：`94ec714`

## 目标

SPEC-106 报告 §7.2 登记三基础服务「目前是**骨架**」——日志服务只应答探针、**不落日志**；
存储服务**不持段、不落数据**。「服务语义是后续片」。SPEC-110 给了三者**有状态**协议，SPEC-111
让 Namespace 服务**代理内核解析**，收口其一。本切片收口余下两个：

- **日志服务**：持有内核 `MemorySegment`，据此**落日志**；
- **存储服务**：持有内核 `MemorySegment`，据此**持段、落数据**。

## 设计（零内核 ABI 改动）

**段由 ring3 建立与移交**（复用 SPEC-111 的「无 cap 唤醒 + 带 cap 消息」移交路径）：

1. **铸段**：服务管理器对 log/store 各 `SYS_MEMSEG_CREATE(1, RW)`，得根 cap
   （权限 `{Derive, SegDestroy, SegMapR, SegMapW}`）。
2. **见证**：管理器把段句柄写回共享段（`OFF_LOG_SEG`/`OFF_STORE_SEG`），供内核侧核验。
3. **移交**：管理器向该服务连发「无 cap 唤醒 + 带 cap 消息」（[`SERVICE_SEG_TAG`]）——服务常态
   阻塞在 `recv`，内核拒绝向 `BlockedRecv` 投带 cap 消息，故先唤醒。**移交先于定序请求**：
   同一发送者保序，服务必先收到段再收到推进/查询。
4. **持段**：服务把段 `memseg_map` 进**自身根**（`SERVICE_DATA_VA`，RW）。服务的 `self_cap` 是
   `task_create` 的 `self` cap（`MANAGER_TASK_RIGHTS`，含 `TaskManageVSpace`），故可映射自身根。
5. **落盘**：`op=ADVANCE` 把推进后的状态 `write_user_bytes` 进段；`op=QUERY` 从段
   `read_user_u64` 读回（而非本地寄存器）——数据路径**真正经过内核管理的内存**。

### 最小权限

| 服务 | 初始 cap | 追加 |
|---|---|---|
| log | `TaskSend`、角色令牌 | 运行期收到**一个**持久化 `MemorySegment`（`RW`） |
| ns | `TaskSend`、角色令牌 | 运行期收到一份 `NsResolve` 只读副本（SPEC-111） |
| store | `TaskSend`、角色令牌 | 运行期收到**一个**持久化 `MemorySegment`（`RW`） |

三者各持**互不为对方所有**的内核权限：ns 无段，log/store 无 Namespace 权限。

## 判决性

- **内核侧**（ring3 无法伪造）：`servicemgr_ready` 新增 `log_seg`/`store_seg` 两字段；`evidence_ok`
  以**新增内核访问器** `bootstrap::segment_of_cap` 认其为**活** `Segment` 对象，并断言两者互异。
  纯内部状态机产生不出内核句柄。
- **内核计数**：`task_ready.caps` 100→**102**（管理器两次 `memseg_create` 各铸一个段根 cap）。
- **数据路径**：服务应答值仍为 SPEC-110 登记的 5/8/8、5/0x53/0x53、5/0x503/0x503——但 `QUERY`
  的值现在**经段往返**：若写入段落空，读回的是建段时的零值（0），既有金标即失败。故金标通过
  即证明「写段→读段」的往返成立。

> **见证层级（如实登记）**：内核**不**直接读服务自己的段（它不在内核根里），故「段内字节内容」
> 非内核逐字节见证。内核见证的是「服务持有活的内核 Segment 对象」+「应答值经段往返」。这与
> SPEC-111 的 `ns_resolved`（内核派生句柄 + `namespace_ready.resolves` 计数）同级。

## 改动

| 文件 | 内容 |
|---|---|
| `prima-runtime/src/protocol.rs` | 新增 `SERVICE_SEG_TAG`、`SERVICE_DATA_VA` |
| `prima-user/src/service.rs` | 收 `SERVICE_SEG_TAG` 带 cap 消息 → `memseg_map` 进自身根；`ADVANCE` 写段、`QUERY` 读段 |
| `prima-user/src/{log,store}.rs` | 文档：持段、落日志/落数据 |
| `prima-user/src/servicemgr.rs` | probe 段：对 log/store 各 `memseg_create(1,RW)`、句柄写回共享段、移交段 cap |
| `kernel/src/bootstrap.rs` | 新增 `pub fn segment_of_cap`（句柄 → 活 Segment 的 id） |
| `kernel/src/user.rs` | 偏移 456/464；`ServicemgrWitness.{log_seg,store_seg}`；`is_live_segment` 谓词 |
| `kernel/src/protocol.rs` | `servicemgr_ready` 输出 `;log_seg=`、`;store_seg=` |
| `xtask/src/protocol.rs` | 解析/软断言 `log_seg`/`store_seg`；`ipc_ready` 夹具 queued 34→38；`task_ready` 夹具 caps 100→102；`TASK_READY_LIVE_CAPS` 100→102 |

## 事件登记（逐条）

| 事件 | 字段 | 旧 | 新 | 依据 |
|---|---|---|---|---|
| `ipc_ready`(23) | queued | 34 | **38** | log/store 各移交 1 段：无 cap 唤醒 + 带 cap 消息（+4） |
| `task_ready`(24) | caps / caps_after | 100 | **102** | 管理器两次 `memseg_create` 各铸一个段根 cap |
| `servicemgr_ready`(29) | data 键 | — | 新增 `log_seg`、`store_seg` | 服务持有的内核段句柄（软断言：非零、非 errno/哨兵、互异） |
| `servicemgr_ready`(29) | `ns_resolved` 值 | `0x…60` | `0x…62` | 句柄槽随分配位移（软断言，不硬编码；SPEC-111 已登记为随分配而变） |

**未变**：`namespace_ready`(30) 全字段（`binds=3`/`resolves=3`/`evidence_ok=1`）；三个
`*_state0..2` 金标值；`frames_ready`/`memory_ready`/`platform_segments_ready`（均为 early 事件，
段在其后创建，故不受影响）。

## 验证

- `cargo xtask test`：**6/6 PASS**（build kernel / image bios / image uefi / qemu bios run=1 / run=2 / stage1 boot slice）
- `cargo xtask lint`：**4/4 PASS**（fmt / `clippy -D warnings` / 内核 `deny(unsafe_code)` / 机制层零 `unsafe`）
- `cargo test --workspace`：**788 passed, 0 failed**
- `cargo xtask tcb`：`trusted=1262 mechanism_lines=21166 total=22428 ratio=5.63%`，机制层 `unsafe=0`
- QEMU：`servicemgr_ready` `log_seg=0x100000060`、`store_seg=0x100000061`（互异的内核段句柄），
  `status=ok`（`evidence_ok=1`）；`task_ready.caps=102`；`ipc_ready.queued=38`
- `IMAGE_TOTAL_PAGES` 50→**53**，仍 ≤ `MAX_SEGMENT_FRAMES=64`（编译期断言把关，见源码注释）

## 未决

1. **段内字节非内核逐字节见证**（见上「见证层级」）：若需更强证据，须让内核经句柄取段帧并直读
   物理内存——需新增「句柄 → 帧」访问器，本片未做。
2. log/store 的段当前各仅 1 页、状态仅一个 u64：尚无**追加式日志记录**与**容量/溢出**语义；
   真正的日志/文件语义属后续片。
3. 本片**不勾选** S4-D3 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
