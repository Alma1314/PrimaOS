# SPEC-102 设计：把服务管理器迁到 ring3 Rust（服务机制收敛）

> 状态：探索实现；不构成 S4-D2/D3 验收或稳定 ABI
>
> 前置：[SPEC-101 字符串服务名与端点名的运行期指派](2026-10-13-stage4-string-service-names-design.md)
>
> 基线：SPEC-101 HEAD `89f0406`

## 1. 本片要回答的问题

阶段 3 的探索自 SPEC-088 起，把「服务发现与路由」逐步堆进了 `kernel/src/user.rs` 的
**手写 x86 机器码**（`init` = task 1）：

- SPEC-088 端点解析与协商、SPEC-090 按名二次路由、SPEC-092/093 第二端点、SPEC-095 运行期注册表、
  SPEC-101 字符串服务名 —— 全部是「创建任务 + 映射栈 + 收自报 + 建表 + 按名扫描 + 转发 + 等 done」
  的**直筒式汇编**，每加一个端点 ≈ 200 行 `put_*`。

S4-D3 要再落**三个**服务（日志、Namespace、基本存储）。照旧手写意味着三份重复汇编，且注册表仍是
「2 项硬编码槽 + 线性扫描」——**不可扩展**。本片把**服务管理器这一角色**（注册表、按名解析、请求
路由/转发）从 `init` 的手写机器码**迁到一个独立的 ring3 Rust 任务**（`prima-servicemgr`），使
「加一个服务」的边际成本从「200 行汇编」降为「表里一行 + 一个二进制」。

## 2. 目标架构

```
         ┌─────────┐   ①建 servicemgr、移交 cap    ┌───────────────┐
         │  init   │ ───────────────────────────▶ │ prima-servicemgr │
         │ (task 1)│   ②建端点、移交发送 cap         │  (ring3 Rust)    │
         │ 手写汇编 │ ───────────────────────────▶ │                  │
         └─────────┘                                │  运行期注册表     │
              ▲                                     │  按名解析(Rust)   │
              │ ③自报(name,id)/done                 │  路由/转发        │
      ┌───────┴────────┐   ┌──────────────┐         └──────────────────┘
      │ prima-smoke    │   │  prima-beta  │                ▲
      │ (客户+ALPHA端) │   │  (BETA 端)   │                │ ④Hello+段 cap
      └────────────────┘   └──────────────┘                │
                                                    ┌──────┴──────┐
                                                    │   客户端     │
                                                    └─────────────┘
```

- **`init` 退化为引导 + 载入**：建 `servicemgr`、按 bootstrap 的段 cap 建端点任务、把所需 cap
  经 IPC 移交，然后靠边站。**不再**持有注册表、不做按名扫描、不做转发。
- **`prima-servicemgr`**（新二进制）：持有运行期注册表；收集端点 `(name,id)` 自报；按客户端写入
  共享段的**字符串请求名**在 Rust 里扫描解析；把 `prepare(behavior)` 与段 cap 转发给解析到的端点。
- **端点/客户端**：协议语义不变（`SERVICE_ID_TAG`/`SERVICE_DONE_TAG`/`SERVICE_PREPARE_TAG`），只是
  **通信对端**由 task 1 变成 servicemgr（对端 id 由创建时运行期指派，沿用 SPEC-101 的入口参数机制）。

## 3. 权限（Capability）流

关键事实（本仓库细化层）：`task_create` 返回的**管理 cap** 权利集恰为
`{Derive, TaskSend, TaskManageVSpace, TaskDestroy}`（`bootstrap::MANAGER_TASK_RIGHTS`），
因此**建任务者可从管理 cap 派生出指向新任务的 `TaskSend` cap**，并作为 `initial_caps`
（≤2 条，与消息携带 cap 同容量）或经 IPC 转交第三方。

- `init` 建 `servicemgr` → 得管理 cap `M`；从 `M` 派生 servicemgr 的 `TaskSend` cap。
- `init` 建端点 A/B → 各自得管理 cap；从中派生**指向 servicemgr 的发送 cap** 作为端点
  `initial_cap0`（使端点自报/done 直达 servicemgr，而非回 init）。
- `init` 把客户端段 cap（含 `Derive`，供 servicemgr 为两转发各派生一份）与客户端请求一并交给
  `servicemgr`（IPC，≤2 cap/条）。
- `servicemgr` 转发时用其持有的端点发送 cap + `prepare` payload（行为码，沿用 SPEC-090）。

> 说明：①「服务注册与路由」由 ring3 任务持有，符合「服务按最小权限拆分」（S4-X2）；
> ②`init` 仍建任务，是因为段 cap 与命名空间 cap 由 bootstrap 播种、`task_create` 前置要求
> 调用者持 `SegMapR|SegMapX` 与 `NsResolve|Derive`——把「创建」也搬进 ring3 是 §6 的后续片。

## 4. 见证策略（尽量冻结既有事件）

既有判决性事件 `service_selected_ready`（seq 28）等当前由 `init` 写 Y 页回执、内核按 VA 读。
迁移后这些值由 `servicemgr` 产生。为**不改变事件字节**：

- `servicemgr` 把解析结果（`name`/`resolved`/`unknown`/`done`/两 `(name,id)`/两 marker 相关值）
  写入**客户端共享段**的固定偏移（端点经转发到的段 cap 同样写 marker，语义不变）；
- 内核见证函数改为从**共享段偏移**读取这些值（而非 Y 页槽）；
- **值本身不变** ⇒ 事件流应逐字节相同（若因任务时序确有变化，按既有纪律**逐条登记**，不静默改写）。

新增区分性见证字段（证明「管理器是 ring3 的、非 task 1」）：`servicemgr` 把**自身 task id 与
自身二进制标记**写入段，见证读出 `id != 1` 且标记等于 `prima-servicemgr` 常量——证明注册表与解析
由**独立 ring3 映像**产出。

## 5. 前置（本设计随附的直接实现）

`prima-runtime::task::TaskSpec` 当前**缺 `name` 字段**：`Runtime::task_create` 只写 10 个 qword，
而内核 SPEC-101 后 `read_task_req` 读 **11** 个（第 11 个是 `name`），故经 Runtime 建任务会读到
越界的 `name`。本片补齐 `TaskSpec.name` 并写第 11 个 qword——迁移后 servicemgr 必然经
`Runtime::task_create` 建端点，这是它的硬前置。**此改动不触 QEMU 事件流**（现无任何调用方经
Runtime 建任务），可独立验证。

## 6. 分片计划（每片一次提交，逐片可验证）

| 片 | 内容 | 判决性见证 |
|---|---|---|
| **SPEC-102**（本片） | 迁移设计 + `TaskSpec.name` 前置（修越界缺口） | host 断言 `task_create` 写满 11 qword 且 `name` 生效 |
| SPEC-103 | 新增 `prima-servicemgr` 二进制 + boot/cap 布置；`init` 建它并移交 cap | 新事件证明 servicemgr 存活、id≠1、收到移交 |
| SPEC-104 | 注册表/解析/路由/转发迁入 servicemgr；端点与客户端改对端；退役 `init` 内 SPEC-090/095/101 手写块 | `service_selected_ready` 值不变（或逐条登记）且新增「由 servicemgr 产出」的区分字段 |
| SPEC-105 | servicemgr **表驱动创建端点**（自解 ELF 入口，免手写创建） | 加一个服务 = 表一行 + 二进制，事件证明它被创建/注册/路由 |

## 7. 未决与登记

1. 本片**只交设计与前置**，不含 servicemgr 任务本身；`init` 内手写服务块此片不动。
2. 迁移会**重定义** `service_reply/negotiated/selected` 事件的**产出方**；届时其字节是否逐变，
   按「默认冻结、变化逐条登记」纪律处理（见 §4）。
3. **不勾选** S4-D2/D3 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. servicemgr 的段 cap/栈段布置、`next_free_id` 分配、kstack 数组覆盖（id 3..8）在 SPEC-103 落实时
   需核；本片不预设其具体槽位。
