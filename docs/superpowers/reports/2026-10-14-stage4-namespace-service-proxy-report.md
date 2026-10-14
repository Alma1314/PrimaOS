# SPEC-111：Namespace 服务代理内核解析（S4-D3 收口）

**日期**：2026-10-14
**阶段**：阶段 4（S4-D3 收口）
**基线**：`9f86feb`

## 目标

S4-D3 的三个基础服务此前只是**内部状态机**（SPEC-110）：不调用任何内核机制、不持有 Namespace
权限——名为「日志 / Namespace / 存储服务」而实无所指。本切片让 **Namespace 服务真正代理内核
`SYS_NS_RESOLVE`**：解析结果由**内核命名空间树**决定，而非服务内的任何编译期常量。

## 设计（零内核 ABI 改动）

权限与路径都由 **ring3** 建立：

1. **权限移交**：管理器从它持有的父 Namespace（`init` 经 `SERVICE_SETUP_TAG` 移交）派生一份
   **仅 `NsResolve`** 的只读副本，用「无 cap 唤醒 + 带 cap 消息」（`SERVICE_NS_CAP_TAG`）**运行期
   移交**给 Namespace 服务。其余两个服务永不收到 ⇒ `log`/`store` 无任何 Namespace 权限。
2. **路径绑定**：管理器把客户端共享段的**派生副本**绑到专用路径 `svc`（`ns_bind`，纯 ring3——
   管理器持带 `NsBind` 的父 Namespace）。用派生副本而非原段：`bind` 会把 cap 移入 Namespace，
   原段仍须交回 `init`。
3. **代理解析**：管理器对 Namespace 服务多发一条 `op=RESOLVE` 请求；服务把 `"svc"` 拷到**本任务
   栈**上、调 `SYS_NS_RESOLVE(cap, path, SegMapR)`，回传内核派生的句柄。

### 最小权限

| 服务 | 初始 cap | 追加 |
|---|---|---|
| log / store | `TaskSend`、角色令牌 | **无**（无 Namespace 权限） |
| ns | `TaskSend`、角色令牌 | 运行期收到一份 `NsResolve` 只读副本 |

## 判决性

- **内核侧**（ring3 无法伪造）：`namespace_ready.resolves` 2→3（服务真解析一次）、`binds` 2→3
  （管理器真绑了 `svc`）、`entries` 1→2。
- **共享段**：`servicemgr_ready.ns_resolved` 是内核派生的**活**句柄（非零、非 errno/哨兵）。
- **无状态回声端点**：既没有 `NsResolve` 副本，也无从产生内核句柄。

## 关键发现（本切片定位）

### ① `MAX_SEGMENT_FRAMES` 静默越界（真 bug，已修）

服务共用 `service.rs`，**任何改动都会放大全部服务 ELF**；SPEC-110/111 使映像段由 43 涨到 **50 页**，
越过 `MAX_SEGMENT_FRAMES = 48`。`SegmentFrames::Vector` 的内联帧向量写穿相邻字段 → **boot 早期
#PF**（`seq=12`，`[pf] cr0.wp=1`）。该容量约束**此前只写在注释里**。

修复：上限 48→**64**，并在 `kernel/src/user.rs` 加**编译期断言**
`IMAGE_TOTAL_PAGES <= MAX_SEGMENT_FRAMES`，杜绝再次静默越界。

> SPEC-109 报告的「内核体积/布局位移已证伪」针对的是**加静态数据**；**改服务源码**会改 ELF 大小
> → 映像段页数，是另一条路径，当时未被覆盖。

### ② 入口第 6 参数（`r9`）不可用（已排除）

曾试图把任务的 `root_ns_cap` 句柄经入口 `r9` 暴露给服务。实测**写死 `r9 = 0x1234` 也读不到**——
既有首次进入用户态的寄存器恢复路径只保证到 `r8`（SPEC-104 的 peer id），`r9` 被丢弃。故改用上文的
IPC 移交。

### ③ init 自测已清空 `lib` / `mnt`（已排除）

`init` 的 ns 自测末段执行 `unbind(init_ns,"lib")` + `unmount(init_ns,"mnt")`，故 boot 后期（管理器 /
服务阶段）解析这两个路径必然 `NotFound`（本切片最初即踩此坑，`ns_resolved` 返回 `NotFound` 编码）。
曾试图在 `init` 里重绑专用路径，但会让 **boot 早期死锁**（`SYS_RECV: no Ready task`），已放弃；改用
管理器自行 `bind`。

## 改动

| 文件 | 内容 |
|---|---|
| `prima-runtime/src/protocol.rs` | 新增 `SERVICE_SERVE_OP_RESOLVE`、`SERVICE_NS_CAP_TAG` |
| `prima-user/src/service.rs` | 收 `SERVICE_NS_CAP_TAG` 带 cap 消息；`op=RESOLVE` 时把 `"svc"` 拷到栈上调 `namespace_resolve` |
| `prima-user/src/ns.rs` | 文档：三者中唯一代理内核机制者 |
| `prima-user/src/servicemgr.rs` | probe 段：绑 `svc`（派生副本）、向 ns 服务移交只读副本、多发一条 `RESOLVE`；第 4 条应答落 `OFF_NS_RESOLVED(448)` |
| `kernel/src/bootstrap.rs` | `MAX_SEGMENT_FRAMES` 48→64 |
| `kernel/src/user.rs` | 编译期断言 `IMAGE_TOTAL_PAGES <= MAX_SEGMENT_FRAMES`；见证 `ns_resolved` |
| `kernel/src/syscall.rs` | `NsWitness::evidence_ok`：`binds` 2→3、`resolves` 2→3 |
| `kernel/src/protocol.rs` | `servicemgr_ready` 输出 `ns_resolved` |
| `xtask/src/protocol.rs` | `ns_resolved` 解析/软断言；`ipc_ready` / `task_ready` / `namespace_ready` 金标 |

## 事件登记（逐条）

| 事件 | 字段 | 旧 | 新 | 依据 |
|---|---|---|---|---|
| `ipc_ready`(23) | queued | 30 | 34 | ns cap 移交 2 条（唤醒 + 带 cap）+ 代理解析 1 请求 1 应答 |
| `task_ready`(24) | caps / caps_after | 97 | 100 | 管理器派生只读 Namespace 副本 + `svc` 绑定的段副本 |
| `namespace_ready`(30) | entries / binds / resolves | 1 / 2 / 2 | 2 / 3 / 3 | 管理器绑 `svc`；Namespace 服务代理解析一次 |
| `servicemgr_ready`(29) | data 键 | — | 新增 `ns_resolved` | 代理解析返回的内核句柄（软断言：非零、非 errno/哨兵；具体值随分配而变，不硬编码） |

**自动通过（关系不变量，无需改）**：`frames_ready` 的 image/segment/free、`service_*` 系列其余字段。

## 验证

- `cargo xtask test`：**6/6 PASS**（build kernel / image bios / image uefi / qemu bios run=1 / run=2 / stage1 boot slice）
- `cargo xtask lint`：**4/4 PASS**（含 `-D warnings`、`deny(unsafe_code)`、零 `unsafe`）
- `cargo test --workspace`：**788 passed, 0 failed**
- `cargo xtask tcb`：`mechanism_lines` 21123→**21133**，`ratio=5.64%`，机制层 `unsafe=0`
- QEMU：`servicemgr_ready.ns_resolved=0x5ec9000400000060`（内核句柄）；
  `namespace_ready` `resolves=3`、`binds=3`、`evidence_ok=1`
