---
id: report-stage6-m1-blocker-closure
kind: report
authority: scoped-implementation-and-design-review
status: verified-scoped
lifecycle: historical
updated: 2026-10-30
---

# S6-M1 当前评审阻塞整改、COSIR 技术评审与 7-A 有界探索

## 结论与范围

用户确认本轮范围为当前评审阻塞项，不是完成阶段 6 全部交付。R1–R6 的**定义阻塞已逐项收口**：
[协议](../specs/2026-10-29-stage6-m1-protocol.md)确定不可警报等待、单服务多客户端、整个目标用户
地址空间委托、私有 WaitObject、预留容量与审计耗尽 fail-stop；候选 A 通过本文范围内的
COSIR 技术设计评审。新增 host 可执行模型和负例，权限位集扩宽至 u64。
D14 的 7-A 有界探索在单核 ring3 QEMU 取得双轮正向见证，不需要先改内核对象模型。

本结论允许按已定义协议推进 M1a/M1b；**不是组织级签署、COSIR 冻结对象表修订、Lean 证明、
Gate trap 接线、Linux LibOS 或 S6-D2 完成**。阶段 6 仍进行中，5 项交付和 4 项退出均不勾选。
后续真实 Gate/Request 能力与最后授权丢失钩子、调度及硬件页表细化仍须按协议实现并通过 G1–G3。

## 基线与输入

Git 基线 `9a9f4a2eba4f779a73a23f700cc556c48d89c803`；进入本轮时工作区另含上一轮未提交的静态复审文档，
本轮将其与后继实现一并提交；原[发现报告 R1–R6](2026-10-29-stage6-m1-decision-review.md)保留其当时失败与未验证结论。
输入还包括 [D17–D22](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)、
[递送提案](../specs/2026-10-29-stage6-m1-delivery-design.md)、
[D14 与启动顺序调整](../specs/2026-10-28-stage6-7-release-decisions.md)、
[COSIR W9/W17/§21](../../../COSIR%20原语规范.md)、[Lean 映照边界](../../../lean/README.md)。

工具链固定 `nightly-2026-07-30`。QEMU 使用仓库参数：`pc-i440fx-8.2`、TCG 单线程、
`qemu64`、1 vCPU、128 MiB，未跑 KVM/实机性能测试。完整双轮串口保存在
[round1](artifacts/stage6-m1/domains-round1.txt)、[round2](artifacts/stage6-m1/domains-round2.txt)。
归档只将 CRLF 统一为 LF，未删改内容，以满足 Git 空白检查；两份转换前串口均为 3670 字节，
SHA-256=`0a41940946ab3019af2e968534b0102479b811aa20b923f17d3691156e44487b`。
镜像为构建产物，不入库；新增 Rust 源码与测试随本报告同一提交保存。

## R1–R6 逐项闭合

| 项 | 已落实的选择与证据 | 精确边界 |
|---|---|---|
| R1 活性 | 取消无依据的有界完成主张；模型保持 Delivered 直到显式 orphan，孤儿后配置值及后续 trap fail-closed | 活服务不答复仍可无限等待；审计满停止调度，不承诺清理进度 |
| R2 取消/重放 | v1 全阶段 `alertable=false`；模型调用真实 `task_alert`，Queued/Delivered 收信号都保持原请求，copy 副作用后也不重放 | 不再需要 M1 的取消确认新 ABI；reply/abort 是终结，信号与 EINTR 留 M4 |
| R3 内存 | 选择整个目标用户地址空间委托；直接 client_va；显式双 Task/root 身份、独占事务；0/64/4096、超长、溢出、跨页缺页/只读、stale root、同 VA 与 Arc-COW 负例 | Arc-COW 只是 host 私有/COW 页实验，不覆盖硬件 PTE、共享可写页、TLB、物理分配失败；M1c 必须再实现和验收 |
| R4 中立失败 | reply/abort 原始 u64、可信 attacher 提供 orphan_value，绑定不回退原生；模型客户端 number=`u64::MAX`/原生冲突号 18，回复值不查 errno 表 | 新增原生服务端错误仍走原生错误域；Linux ENOSYS 只能由用户态服务编码 |
| R5 W17/授权 | 真实 State WaitObject/WaitRequest 双向注册，私有对象不铸普通信号权；终结使全部 Request 别名失效；wait 输出/句柄预检失败、审计满均保持状态；u64 权利和高位传输实测 | 模型 token 是实验私有类型，真实 CapTable mint/move/drop 钩子及代次仍是 M1b 实现；不是完整 W1–W17 证明 |
| R6 容量 | 多客户端共用一个固定服务 Gate；64 个绑定各留一槽，8 个队列项，其余 WaitingCapacity；满载、65th 拒绝、销毁移除、FIFO 晋级及回收全部通过 | host 是逻辑资源账，使用 Vec/clone；内核预分配 arena，另留每 Gate 的服务等待对象（总预算≤80），不能在阻塞时动态分配 |

审计耗尽不再隐含为“总能返回配置值”：协议明确选择全局 fail-stop，与普通控制调用的失败返回分开。
模型 AuditFull 是内部提交拒绝，不是未修改客户端可见的 errno；内核接线须有预留诊断和全局停止钩子。
故障注入验证 reply/orphan 不会出现帧已改但请求/审计未提交的半状态。

## COSIR 候选 A 技术设计评审

评审对象是[收口协议](../specs/2026-10-29-stage6-m1-protocol.md)及
[gate 模型](../../../cosir/src/gate.rs)，不是从现有内核反推冻结契约。

| 义务 | 审查结果 |
|---|---|
| 零 Linux 专属状态/语义 | 没有 Linux 调用号、errno、fork/exec/signal 分支；number/args 只保存、记录，不决定迁移；u64 返回值只搬运 |
| W9/W17 的等待投影 | 每个活动 call 对应私有活动对象和单项 WaitRequest，BlockedWait 双向成立，pending 字段空；每个成功事务检查现有 `waits_well_formed` |
| 完成权不能被普通 WaitSignal 绕过 | 不铸私有 WaitObject 能力，信号只能排队；请求终结由匹配的固定服务身份与 Delivered token 授权 |
| 生命周期与别名 | 请求 id 单调不复用；服务身份绑定，不允许服务被门控/自调用环；销毁、orphan 及迟到 copy/reply 负例通过 |
| 失败原子性 | host 克隆事务同时提交 State、frame、请求、队列、内存和事件；输出/句柄不足及审计满不改变原模型 |
| 审计范围 | 新 gate 迁移事件为实验 ledger，与现有 State.audit 组合检查容量；没有声称已实现生产 AuditEvent schema、完整 W13 或 Lean 保持性 |

服务端空队列的调度未在此 host 模型执行：`wait` 返回 Busy，由 M1b 按协议的专属服务等待对象接线；
输出/句柄空间只用布尔预检入口建模，不代表真实用户缓冲或 CapTable 安装已验证。

结论：**A 在 M1a 的中立语义与 W17 投影范围内技术通过**，无需新增 BlockedCall。
冻结 COSIR 对象/权限表及机器细化另须按真实实现提交；不能以这次有限反例检查代替全称证明。
遇到无法细化的反例，重开对应 R 项，不允许省略取消/孤儿/审计安全路径以抢先接线。

## D14：7-A 有界探索

新增 opt-in 命令：

```sh
PRIMA_STAGE6_DOMAINS=1 CARGO_NET_OFFLINE=true cargo xtask test-domains
```

只在显式设置环境变量时启用。沿用现有 ALPHA/BETA 两个独立 ring3 二进制和当前服务/进程环境，
创建两个独立 Namespace 作为它们的结构根；不新增内核 syscall/对象，不增加任务槽。
这是用于证伪架构的**人格桩**，没有宣称两套完整 Linux/其它 LibOS 或原样 POSIX 应用已并存。
日志收集和监督角色同置于现有 ring3 servicemgr：每个根中的 `log` 绑定授权其 TaskSend，
客户端经各自根解析派生日志能力后发送，管理器依据内核认证的 sender 校验来源。

两个客户端先各在相同 VA 建私有页并写自身 id，双方 INIT 确认后才检查；互给另一根的原始
句柄值（不转移 token）尝试解析，必须得到真实 Runtime `Denied`/EACCES。
flags=7 分别表示跨域拒绝、同 VA 私有内容保持、经自身根得到日志授权三项同时成立。
A/B 各记录一次后，监督者实际 `process_terminate(A, 9)`，B 必须再完成同样检查并记录一次。
不是只在 host 数组中移除 A；没有重启 A 或由管理器代写 B 的检查结果。

双轮均观察到：

```text
S6DOM|roots a=4294967484;b=4294967485
S6DOM|log step=0;task=10;flags=7
S6DOM|log step=1;task=11;flags=7
S6DOM|log step=2;task=11;flags=7
S6DOM|PASS
```

服务根均成功绑定同名 `log`；如果两 token 实为同一 Namespace，第二次 bind 会失败。
检查器要求恰好五条记录、不同非零根、不同 A/B sender、B 的终止后进展、每次 flags=7，
并验证 QEMU debug-exit=33。负例覆盖漏记录/重复 PASS、同根、flags 失败、终止后错误 sender。
原始串口保留根句柄、认证后的 sender 和顺序，host COSIR 例子另从 Audit 还原成功消息与销毁。

该专用运行在判据满足后经已有设备能力退出 QEMU，**不计作默认 boot 套件轮次**。
域配额/故障恢复、恶意服务压力、SMP、完整 personality/文件根语义都不在探索结论内；
7-A 完整交付和阶段条件仍未勾选。这是 D14 的正向可行性检查点，不提前完成阶段 7。

## 权限位集与工程验证

`kernel::Rights` 与 Runtime 请求位集扩为 u64，去除六处入口的 `as u32` 截断；位 0–30 不变，
尚未定义的高位仍由 subset/well_typed 拒绝。测试逐一检查 64 位，对 bit32/bit63 的 Runtime
传输及未知高位混入已有权限的拒绝另有回归。COSIR/Lean 的 UInt32 权利属于另一命名空间，
本次不强行扩宽或声称新增 Gate 的 Lean 映照。

首次直接扩宽触发 BootstrapState 大小守卫（13336 > 13312 字节）。没有放宽栈限制：把
BootstrapSnapshot 三个永远固定的初始权限改为常量访问方法，保留生产编译期和 host 的
13 KiB 守卫；串口权限渲染和值不变。最终 kernel/Runtime 单测与默认启动回归验证该变化。

最初构建缺少本 worktree 的 musl snapshot，随后从同机主 checkout 的已锁定源副本复制到
本 worktree 的 ignored target，交由现有 build.py 验证 commit/干净状态/许可证 hash 后重建。
sbase 同样复用本机源副本。未修改上游源、锁文件或全局 Cargo 镜像；`CARGO_NET_OFFLINE=true`
让已有缓存的 bootloader 子构建不再尝试 rsproxy 更新，解决前序 DNS 验证阻塞。

## 命令与结果

最终验证于 2026-10-30 完成，文件名保留启动日期；临时 `/tmp/prima-s6-*.log` 不作为长期工件，关键命令与失败保留于此。

| 命令 | 结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p cosir --test stage6_gate` | PASS：8 个生命周期/复制/资源/权限模型反例组；不是 QEMU gate |
| `CARGO_NET_OFFLINE=true cargo run -p cosir --example stage6_domains` | PASS：两根、能力授权共享 log、跨域 NotHeld、A 销毁后 B 继续、Audit 消息序列可重建 |
| `PRIMA_STAGE6_DOMAINS=1 CARGO_NET_OFFLINE=true cargo xtask test-domains` | PASS：QEMU 2/2；已保留原始串口；不是 G1–G3 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1035 passed，1 个既有外部 ELF 样本测试 ignored；包含新模型 8 组及协议负例，未运行 Lean |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、workspace/all-targets Clippy -D warnings、裸机内核编译及 kernel/src 32 文件零 unsafe |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：获准的沙箱外单次完整命令 exit=0，36 个程序 / 72 轮 QEMU、全部主机参考及 musl 检查通过；[完整日志](artifacts/stage6-m1/full-suite.txt) |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base 9a9f4a2eba4f779a73a23f700cc556c48d89c803`，`git diff --check` | PASS：331 文档；生成目录、生命周期、新文档元数据、链接及变更守卫一致 |
| Gate G1/G2/G3、Linux 白名单差分、复制性能、Lean gate 证明 | 未运行：本轮不接线 Gate、不实现 profile；不能算阶段退出证据 |

[沙箱内首次完整命令记录](artifacts/stage6-m1/full-suite-sandbox-failure.txt)返回 1：Linux N1 参考创建套接字失败（全部网络标志为 0），
Rust R3 Linux 参考在 `posix.rs:88` 报 `Operation not permitted`，因此该组未进入 QEMU；
其余 70 轮 QEMU 全部通过。随后以同一 `target/prima-os/net-linux` 二进制在获准的
沙箱外环境、`env -i` 下单独重跑，exit=0，全部网络标志为 1。完整沙箱外后继结果见上表；
未修改测试源码、超时、检查器或预期输出来绕过环境失败。

## 后继与阶段影响

本报告在当前协议定义、候选 A 技术设计评审和 D14 探索范围内闭合 R1–R6 的前置缺口，
不删除或改写前序静态报告。D17–D22 六条路线保留，但取消、寻址、授权和资源耗尽按明确后继收口；
profile v2/SPEC-152 与阶段 6/7 退出条件不变。当前状态只在[阶段跟踪](../../../Prima%20OS%20实现阶段.md)维护。
