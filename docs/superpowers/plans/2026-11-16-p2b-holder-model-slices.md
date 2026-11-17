---
id: plan-p2b-holder-model-slices
kind: plan
authority: execution-plan
status: in-progress
lifecycle: current
updated: 2026-11-17
---

# P2b 与 holder 模型依赖切片计划

> 2026-11-18 进度：P2b-0 至 P2b-6 已全部落盘，P2b 作为证据包收口；workspace/lint、
> 既有 Linux 差分、`test-linux-proxy` 两次启动与完整 36 组 72 轮 BIOS 套件通过。
> P2 不签署、7-A 未勾选；P2b-6 的组织级复核仍待办，holder H0–H7 未开始。证据见
> [P2b-0/P2b-1 报告](../reports/2026-11-17-p2b-store-batch-ops.md)、
> [P2b-2 报告](../reports/2026-11-17-p2b-linux-frontend.md)、
> [P2b-3 报告](../reports/2026-11-17-p2b-fork-exit.md)、
> [P2b-4 报告](../reports/2026-11-18-p2b-proxy-wiring.md)、
> [P2b-5 报告](../reports/2026-11-18-p2b-proxy-matrix.md) 与
> [P2b-6 收口](../reports/2026-11-18-p2b-closeout.md)。

## 目标、范围与基线

把两条尚未闭合的底座工作拆成可独立验收的切片：P2b（Linux 数据面迁移、
fork 生命周期、镜像接线与 QEMU 正反例）和 holder 模型（Process/Thread/
ShareGroup 的能力持有者语义）。本计划只确定顺序、交付物、验收输入和停止条件；
不冻结 holder 语义，不产生运行证据，不签署 7-A，不改变任何退出勾选。

- 基线：`5a163e9`（P2a 已提交；此前基线 `4830638`）。P2a 的代码与报告均在该提交内，
  P2b 从干净工作树继续。
- 上位契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md)、
  [LibOS 数据通路计划](2026-11-10-libos-data-path.md)、
  [COSIR 原语规范](../../../COSIR%20原语规范.md)、
  [双人格采纳审核](../reports/2026-10-14-cosir-dual-personality-adoption-review.md)、
  [阶段7范围裁决](../specs/2026-10-31-stage7-scope-decisions.md)。
- 排除项：SMP、配额、完整撤销、透明恢复、动态链接、设备/持久化、Windows
  personality、7-A 退出。除 H 切片明确要求外不新增 syscall。
- 关键事实：内核已有隐式 holder——`Process.cap_space` 代表线程组的能力空间，
  `space_of()` 按 Process 解析 holder（`kernel/src/bootstrap.rs:3304`、`:10078`）；
  COSIR 尚未形式化该语义。H 切片的目标是**形式化并一般化现有实现**，不是另起炉灶。

## 依赖总览

```text
P2b-0 → P2b-1 → P2b-2 → P2b-3 → P2b-4 → P2b-5 → P2b-6
H0 ───→ H1 ───→ H2 ───→ H3 ───→ H4 ──────────┐
                                              └→ H5 → H6 → H7
                    （H5 同时依赖 P2b-6）
```

| 切片 | 依赖 | 类型 | 关闭时提供 |
|---|---|---|---|
| P2b-0 协议与 fixture 冻结 | P2a | 契约 | op 矩阵、容量与根策略 |
| P2b-1 Store 批量操作补全 | P2b-0 | 实现 | host 操作语义与负例 |
| P2b-2 Linux 前端切换 | P2b-1 | 实现 | 数据面不再走本地 Ram |
| P2b-3 fork/exit 生命周期迁移 | P2b-2 | 实现 | 预留/发布/回滚/关闭闭环 |
| P2b-4 镜像与启动 fixture 接线 | P2b-3 | 实现 | 两 ABI 程序可启动 |
| P2b-5 QEMU 正反例矩阵 | P2b-4 | 验收 | 代理路径机器证据 |
| P2b-6 P2b 收口 | P2b-5 | 证据 | 报告与组织级复核 |
| H0 holder 设计提案 | 无（可与 P2b-1 并行） | 设计 | 显式决策草案 |
| H1 采纳评审与裁决 | H0 | 裁决 | 采纳/暂缓/否决记录 |
| H2 COSIR 与 Lean 修订 | H1（须采纳） | 语义 | 规范、模型、证明 |
| H3 内核 generalized holder | H2 | 实现 | 位置/空间/审计 |
| H4 Process/Thread/ShareGroup 对象 | H3 | 实现 | 创建与共享语义 |
| H5 P2b 绑定迁移 | H4、P2b-6 | 实现 | 代理绑定映射到新 holder |
| H6 holder 验证与证据 | H5 | 验收 | host/QEMU/Lean/故障注入 |
| H7 holder 收口 | H6 | 证据 | 组织级复核记录 |

排序规则：

1. P2b-1 的线协议与错误映射冻结后，才允许 P2b-2 改前端；否则会把局部实现
   当成跨 personality 契约。
2. P2b-3 的 fork 生命周期先于 P2b-4 接线；镜像先跑起来会掩盖 owner 回滚缺口。
3. H0/H1 可与 P2b-1～P2b-3 并行；**H1 未采纳前不得开始 H2 及之后的语义实现**。
4. H2 先改规范与 Lean，再改内核；不允许实现先成为事实标准。
5. H5 在 P2b-6 与 H4 之后；它只迁移绑定，不改变 P2b 已冻结的数据面语义。
6. P2b-6 与 H7 都是 7-A 的输入，但都不关闭 7-A；7-A 还需要独立语义状态/根/能力
   视图、真实双白名单、单侧故障与耗尽隔离证据。

## P2b 切片

### P2b-0 协议、容量与 fixture 冻结

- 依赖：P2a。
- 交付路径：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) 的 P2b 节、
  `prima-runtime/src/bulk.rs`、`prima-runtime/src/protocol.rs`。
- 工作：
  1. 按 D12 冻结白名单（`busybox-static 1.36.1` 六 applet + 2 探针）列出 Linux
     VFS 必需的 op：openat、read、write、stat/fstat、getdents、lseek、close；
     明确 fork/exec 生命周期需要但不在白名单的 op 一律标 unsupported。
  2. 冻结 fd→Store OFD 映射、共享游标、`O_APPEND`、目录流位置归属和 errno 映射表；
     错误必须来自 Store，不由前端猜测。
  3. 冻结根策略：`/linux`、`/native`、`/shared`；`shared` 同名真实目录的遮蔽规则
     必须二选一显式化（禁止或定义优先级），不得留隐式行为。
  4. 冻结 fixture 来源与哈希（当前编译在 `prima-user/src/service.rs`）；记录容量上限：
     `SERVICE_FILE_HANDLES`、`MAX_CLIENTS=18`、`MAX_OWNERS=32`、`MAX_PROXIES=8`、
     Linux fd 表 16。
- 2026-11-17：契约文本与 op 编码已落盘；16 B 中性 stat、wire fd、错误映射与
  fixture 哈希和当前代码一致。编解码与 Store 执行由 P2b-1 完成并有 host 证据。
- 验收：契约文本 + `bulk` 编解码 host 测试；不需要 QEMU。
- 停止条件：白名单所需 op 无法在现有 VFS/OFD 抽象中保持语义时停止并修订契约；
  不得在前端模拟。
- 不包含：任何数据面实现。

### P2b-1 Store 批量操作补全

- 依赖：P2b-0。
- 交付路径：`prima-user/src/service.rs`、`prima-runtime/src/bulk.rs`、
  `prima-runtime/src/vfs/handles.rs`、`prima-runtime/src/vfs.rs`。
- 工作：
  1. 扩展 `bulk::execute` 覆盖 open（视图直接来自登记表 `entry.policy`，不依赖
     `PROXY_SELECT`）、stat、getdents、lseek、close；目录游标保存在 Store 的 OFD，
     不保存在 Linux 进程。
  2. append 与共享游标按 P2b-0 实现；每次提交都按代理登记表校验 owner、视图与序号；
     payload 中的 fd/NodeId/PID 不能成为授权。
  3. `VfsError` 到 Linux errno 的映射与现有 `fs_error` 一致；失败路径保持原子性
     （读回滚游标、写不重放、目录位置不前进）。
- 验收：host 测试覆盖两 owner 共享 OFD 游标、append、连续 getdents、越权 owner、
  重放/跳号、EINVAL/ENOENT/EACCES/EFAULT 映射；后续 QEMU 复用同一矩阵。
- 停止条件：共享游标或 append 无法保持时停止；不得用“每 fd 独立游标”替代并宣称兼容。
- 不包含：Linux 前端切换、镜像接线。

### P2b-2 Linux 前端切换

- 依赖：P2b-1。
- 交付路径：`prima-user/src/linux.rs`（`Shared`、`Process::{open,read,stat,getdents}`）、
  `prima-user/src/linux_process.rs`；客户端代理逻辑可新增 `prima-user/src/linux_store.rs`。
- 工作：
  1. 删除 Linux 服务内的 `Ram`/`OpenFiles` 数据面；`Shared` 只保留 personality
     私有状态（输入位置、段表等）。
  2. Process fd 表改为 Store OFD 句柄；每个 VFS syscall 通过已登记代理会话提交批量；
     open 路径的受限根视图由登记策略决定，不依赖 `PROXY_SELECT`。
  3. 保留 Linux 私有语义（fd 标志、标准 fd、errno），但不再持有文件内容。
- 验收：host 用模拟 transport 覆盖 open/read/write/stat/getdents/lseek/close；
  冻结白名单路径不再访问本地 Ram；QEMU 证据留到 P2b-5。
- 停止条件：某 syscall 需要 Store 无法表达的私有状态时，保留在域内并登记；
  不得把未授权路径偷偷转成 NodeId 访问。
- 不包含：fork/exit 生命周期、镜像。
- 2026-11-17：前端已切到 `linux_store::Session`，`Shared` 不再持有 `Ram`/`OpenFiles`。
  管理器签发的真实代理会话尚未接线，QEMU 暂由进程内 `Local` transport 提供字节；
  fork/exec/exit 的 Store owner 预留与关闭按切片顺序留给 P2b-3。见
  [报告](../reports/2026-11-17-p2b-linux-frontend.md)。

### P2b-3 fork/exit 生命周期迁移

- 依赖：P2b-2。
- 交付路径：`prima-user/src/linux_process.rs`（`Family::fork_inner`）、
  `prima-user/src/service.rs`（owner 关闭与回收）、`prima-user/src/sessions.rs`、
  `prima-user/src/proxy.rs`、`prima-runtime/src/vfs/handles.rs`。
- 工作：
  1. `fork_owner` 预留、子进程发布、失败回滚、退出关闭全部走 Store owner；
     当前本地 `Shared.files` 的预留路径删除。
  2. fd 表复制、共享 OFD 游标、CLOEXEC 与 exec 关闭按 P2b-0 的范围实现；
     超出白名单的语义登记为 unsupported。
  3. 旧 incarnation、已释放 owner、owner 0 与 legacy 槽不得授权。
- 验收：host 反例覆盖 fork 失败回滚、退出关闭、槽位复用、旧 cookie 拒绝；
  两进程共享游标的可见结果与 Linux 参考一致。
- 停止条件：fork 失败或退出窗口无法保持“要么完整预留、要么无副作用”时停止。
- 不包含：多线程 fork、完整 execve。
- 2026-11-17：owner 预留/发布/回滚/退出关闭已迁到 Store 客户端接口，并新增
  `PROXY_RESERVE/PUBLISH/RELEASE` 代理消息；`Local` 以按 owner 序号与槽位复用
  提供 host 证据。真实代理会话发送端仍属 P2b-4；fork 成功后发布失败的窗口（内核
  子已存在）未闭合，已在报告登记。见
  [报告](../reports/2026-11-17-p2b-fork-exit.md)。

### P2b-4 镜像与启动 fixture 接线

- 依赖：P2b-3。
- 交付路径：`xtask/src/image.rs`（`SERVICE_IMAGES`）、`xtask/src/stage6.rs`、
  `prima-user/src/servicemgr.rs`（`PROXY_REGISTER` 签发）、
  `prima-user/src/service.rs`（`prepare_policy`）、`prima-user/src/proxy.rs`。
- 工作：
  1. 镜像装入 Store、管理器授权与两种真实应用；启动时由管理器签发代理会话并建立
     受限根视图。
  2. 按 P2b-0 创建 `/linux`、`/native`、`/shared` 与 fixture；同名遮蔽规则显式执行。
  3. 修改 `prima-user` 后显式重建并清理 artifact 目录，避免 `xtask` 复用旧二进制。
- 重点复核：服务 ELF 容量（58/64 页）、入口 ABI 只到 r8、后加服务 id 可能被最小
  id 调度器饿死、fd/owner 槽位上限。
- 验收：`cargo xtask build` 成功；专用 QEMU 场景能启动两 ABI 程序并观察到代理注册。
- 停止条件：容量或入口 ABI 需要扩大但没有契约时停止并登记；不得只调大页数掩盖。
- 不包含：完整正反例矩阵。
- 2026-11-18：`test-linux-proxy` 启动 Linux personality（真实 `Remote` 经
  Store 读 `/linux/fixture.txt`）与原生/POSIX 应用（直接会话读 `/README`），
  管理器签发会话并收齐 `S7ABI` 见证；场景栈 64 页/基址 `0x0B00_0060_0000` 与
  personality 根 `Derive|NsResolve` 已登记为容量契约。代理 fork 子注册、共享资源
  授权与正反例矩阵留待 P2b-5。见
  [报告](../reports/2026-11-18-p2b-proxy-wiring.md)。

### P2b-5 QEMU 正反例矩阵

- 依赖：P2b-4。
- 交付路径：`xtask/src/qemu.rs`、`xtask/src/stage6.rs`（或新增阶段 7 场景模块）、
  后续报告。
- 工作：
  1. 正例：Linux 与 Prima 原生/POSIX 两个真实 ABI 程序同时运行；同一 `/shared`
     资源显式授权可访问；各自私有资源互不可见；原有 Linux 差分与原生回归保持。
  2. 负例：伪造代理 owner、旧 incarnation、猜 fd/NodeId、`..`/绝对符号链接根逃逸、
     未授权共享、fork 失败回滚、单侧退出后另一侧继续。
  3. 保持既有 36 组 72 轮 BIOS 套件；不得静默改写既有事件或回执。
- 验收：命令、退出码、trace 断言与必要输出摘录进报告；失败保留，不以重跑覆盖。
- 停止条件：既有 72 轮回归失败或需要修改冻结事件时停止；逐条登记后才可继续。
- 不包含：7-A 退出、SMP、配额、恢复。
- 2026-11-18：矩阵分两次启动（`0x105`/`0x107`），覆盖 Linux 私有读、代理 fork
  子注册与 exec、`/shared` 显式授权、fork 限额，以及原生策略视图的七条负例；
  `S7HALF` 记录单侧先完成。发现并修正两处真实路径缺陷：受管 gate fork 无内核
  Process 父系（改为 TaskSend 能力绑定）、代理页槽位复用的延迟销毁未重试。
  既有 72 轮 BIOS 套件未在本片实跑，留作 P2b-6 停止条件。见
  [报告](../reports/2026-11-18-p2b-proxy-matrix.md)。

### P2b-6 P2b 收口

- 依赖：P2b-5。
- 交付路径：新报告（建议 `docs/superpowers/reports/2026-11-16-p2b-data-plane.md`）、
  [阶段跟踪](../../../Prima%20OS%20实现阶段.md)、
  [LibOS 数据通路计划](2026-11-10-libos-data-path.md)。
- 工作：登记实际命令、结果、失败与限制；更新 P2 状态；链接阶段跟踪与索引；
  需要组织级复核的部分单独记录。
- 验收：文档工具通过；复核记录存在。
- 停止条件：任何“72 轮 PASS 但代理路径未执行”的表述不得写成双 personality 完成。
- 不包含：7-A 勾选。
- 2026-11-18：收口报告已落盘，完整 BIOS 套件 36 程序/72 轮通过；`Direct` 裁决为
  保留。组织级复核尚未执行，故 P2b-6 仍不是“已复核关闭”。见
  [收口报告](../reports/2026-11-18-p2b-closeout.md)。

## Holder 模型切片

### H0 holder 设计提案

- 依赖：无；建议与 P2b-1 并行启动。
- 交付路径：新设计文档（建议 `docs/superpowers/specs/2026-11-16-holder-model-design.md`）。
- 必须显式回答：
  1. 是形式化现有 `Process.cap_space`（holder=线程组代表）还是引入独立
     ShareGroup 对象；两者如何迁移。
  2. 能力位置：`HeldBy(holder)` 的 holder 身份、generation、销毁与审计事件；
     `InNamespace`、`InMessage`、`HeldByKernel` 的相对顺序。
  3. 共享状态清单：vspace、cap 表、根 Namespace、fd/OFD 表、信号/凭据；
     线程本地清单：入口、栈、邮箱、wait request、pending upcalls、pending_exc。
  4. 等待者身份：`wait_objects` 等待 Thread 还是 Process；唤醒选择与
     `pending_upcalls` 的关系。
  5. Linux `clone` 的共享组合与 Windows Process/Thread 的映射；GPU/NPU
     `ComputeTask` 是否参与 holder。
  6. W3/W4/W7/W9/W15/W17 与 §17 证明义务的影响；旧句柄与 leader 死亡规则。
- 验收：文档评审通过，决策明确；不产生代码。
- 停止条件：关键决策无法定案时停止，不带开放问题进入 H1。

### H1 采纳评审与裁决

- 依赖：H0。
- 交付路径：新审核报告（建议 `docs/superpowers/reports/2026-11-16-holder-model-review.md`）。
- 工作：逐项给出采纳/暂缓/否决及理由；技术复核与组织级复核分开记录；
  明确是否授权修改 COSIR 规范。
- 验收：书面裁决；采纳项有规范修订授权，否决项保留反例与后续入口。
- 停止条件：否决或暂缓时回到 H0；不得开始 H2 实现。

### H2 COSIR 与 Lean 修订

- 依赖：H1 采纳。
- 交付路径：`COSIR 原语规范.md`（§4、§5.2、§8、§21、W3/W4/W9/W17、§19）、
  `lean/Formal/Location.lean`、`lean/Formal/State.lean`、`lean/README.md`、
  `cosir/src/authority.rs`、`cosir/tests/lean_backtrace.rs`。
- 工作：引入 holder 位置与 Process/Thread/ShareGroup 状态；更新操作、错误、审计、
  不变量与映照；Lean 模型给出相应保持性定理或有界检查；cosir 与 Lean 对拍。
- 验收：固定版本 `lake build`、`cargo test -p cosir`、映照测试通过。
- 停止条件：模型无法保持 W3/W4/W17 时停止并修订 H0；不得先改内核。
- 不包含：内核实现。

### H3 内核 generalized holder

- 依赖：H2。
- 交付路径：`kernel/src/bootstrap.rs`（`CapabilityLocation`、`Process`、`space_of`、
  `fork_views`、审计）、`kernel/src/syscall.rs`、必要的 `kernel/src/gate.rs`。
- 工作：把 `HeldByTask(u32)` 的语义一般化为显式 holder/能力空间；`space_of`
  保持兼容；派生/移动/丢弃/失效、fork 视图、审计与 leader 死亡规则按 H2 实现。
- 验收：host 内核测试覆盖位置唯一性、stale 拒绝、leader 死亡后 live 线程、
  槽位复用；现有 workspace 测试不回归。
- 停止条件：出现旧句柄复活、位置歧义或审计无法重建时停止。
- 不包含：新 personality 语义。

### H4 Process/Thread/ShareGroup 对象与创建语义

- 依赖：H3。
- 交付路径：内核 Process/Task 表、`prima-abi/src/lib.rs`（如需要新调用）、
  `prima-runtime` 封装、`prima-user` 线程/会话支持。
- 工作：实现 H2 冻结的共享/线程本地状态；创建、销毁、等待者身份、pending upcalls；
  Linux clone 映射由 LibOS 解释；保持 GPU/NPU Task 兼容。
- 验收：host + 有界 QEMU 见证；线程加入/退出/等待与共享表销毁。
- 停止条件：`ComputeTask` 兼容或终止顺序无法满足时停止。
- 不包含：完整 POSIX/Windows 线程语义。

### H5 P2b 绑定迁移

- 依赖：H4、P2b-6。
- 交付路径：`prima-user/src/proxy.rs`、`sessions.rs`、`service.rs`、
  `linux_process.rs`。
- 工作：把代理登记的 `(app Task, Process, OFD owner, 根策略)` 映射到新 holder；
  保持旧 incarnation/槽位复用拒绝；重跑 P2b QEMU 矩阵。
- 验收：P2b 正反例全部保持；授权歧义为零。
- 停止条件：无法唯一确定 holder 或出现跨域继承时停止。
- 不包含：改变 P2b 数据面协议。

### H6 holder 验证与证据

- 依赖：H5。
- 交付路径：报告、Lean/模型/host/QEMU 测试。
- 工作：Lean 保持性/有界检查、cosir 对拍、内核 host、QEMU 有界见证；
  故障注入覆盖等待中销毁、leader 死亡、共享表拆除、旧句柄与槽位复用。
- 验收：命令、退出码、范围与限制进报告；不用 host 替代 QEMU，不用技术证据替代签署。
- 停止条件：需要修改冻结事件时逐条登记；不得静默放宽。
- 不包含：7-A 退出。

### H7 holder 收口

- 依赖：H6。
- 交付路径：审核报告、阶段跟踪、相关索引。
- 工作：组织级复核；登记未覆盖项；明确本切片不关闭 7-A。
- 验收：复核记录与证据链存在。
- 停止条件：组织级复核未通过时保持未完成状态。

## 验证与停止条件

每片先 host 聚焦检查，再 workspace/lint；协议或启动改动追加相关 QEMU 与完整 BIOS。
文档改动运行：

```sh
python3 scripts/docs.py self-test
python3 scripts/docs.py build
python3 scripts/docs.py check
python3 scripts/docs.py check --base 5a163e9
```

其他最小命令：

```sh
cargo test --workspace
cargo xtask lint
cargo xtask test
cargo test -p cosir
cd lean && elan run leanprover/lean4:v4.35.0 lake build
python3 prima-user/musl/test_port.py
```

证据规则：

- host、QEMU、Lean、差分各自成立，不能互相替代；失败与反例留在报告。
- 原始日志放 ignored `target/` 或 `/tmp/`，报告只嵌入命令、退出、必要摘录与哈希。
- 既有 QEMU 事件/回执默认逐字节冻结；确需变化时逐条登记。
- 修改 `prima-user` 后显式重建并清理 artifact，避免复用旧服务二进制。
- 技术复核不构成组织级签署；harness 内独立审查也不替代组织级复核。

## Review Focus

以下输入最可能被测试遗漏，归属切片必须各加一条测试：

1. `fork` 后父子共享同一 OFD 游标：一方 read 后另一方的偏移必须可见（P2b-3）。
2. `O_APPEND` 与失败重放：写失败后不得前进游标或重放（P2b-1）。
3. 连续 `getdents`：目录位置只在 Store OFD 前进，fork/失败不重复或跳过项（P2b-1/P2b-3）。
4. 槽位复用：退出/回滚后旧 owner、旧 Task/Process cookie 不得授权（P2b-3/P2b-5）。
5. 根逃逸：`..`、绝对符号链接、跨根 rename 与 `shared` 同名遮蔽（P2b-5）。
6. holder 迁移后的 stale handle：leader 死亡但线程存活时旧句柄不复活（H3/H6）。

## 收口与生命周期

各切片完成后链接真实报告；未完成项保留而不记完成。本计划在 P2b-6 与 H7
落盘报告后可转 `historical`；若 holder 语义被否决或 P2b 契约修订，更新本计划
或显式指向后继，不在旧计划内滚动维护实现事实。P2b-6 与 H7 都不关闭 7-A；
7-A 仍需独立语义状态/根/能力视图、真实双白名单、单侧故障与耗尽隔离证据。
