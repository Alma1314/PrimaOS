---
id: report-stage6-m1-decision-review
kind: report
authority: scoped-design-review
status: changes-required
lifecycle: historical
updated: 2026-10-29
---

# S6-M1 六项决定复审：保留最小路线，纠正安全与活性主张

## 后继整改

本报告保留初次静态审查的失败与限制。[后继整改报告](2026-10-29-stage6-m1-blocker-closure.md)
以明确协议、8 组 host 模型反例、权限 u64 修复及 D14 ring3 QEMU 双轮探索，收口当前 R1–R6
定义阻塞与 M1a 候选 A 技术设计评审。它不追认本报告当时已通过，也不证明 Gate 内核接线或阶段退出。

## 结论与范围

阶段 6 已按作者指令启动，维持进行中。复审结论：**保留追加 ABI、rax 答复、代理复制、
暂缓 fork/exec、候选 A、阻塞背压六个方向；原设计需要修订，不能据此直接接线或冻结。**
本报告是源码与契约的静态审查，不是 COSIR 架构通过、7-A 原型验收或阶段签署。
没有新增 syscall、内核实现或运行期证据，S6-D1–D5 / S6-X1–X4 均不勾选。

基线：`9a9f4a2eba4f779a73a23f700cc556c48d89c803`（工作区初始干净）。审查输入：
[D17–D22](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)、
[递送提案](../specs/2026-10-29-stage6-m1-delivery-design.md)、
[M1–M6 计划](../plans/2026-10-27-stage6-7-refinement.md)、
[启动报告](2026-10-29-stage6-start.md)、
[装载切片及未完成清单](2026-10-28-stage6-loader-slice.md)、
[D14 启动顺序调整](../specs/2026-10-28-stage6-7-release-decisions.md#阶段-6-启动记录2026-10-29)、
[COSIR W17/§21](../../../COSIR%20原语规范.md)、[Lean 映照边界](../../../lean/README.md)。

## 六项审查结论

| 用户问题 / 原决定 | 结论 | 实现前必须落实的边界 |
|---|---|---|
| 0 / D17：73–78 或拼装 IPC | 支持追加；当前 `prima-abi` 最大号为 72，73–78 未定义 | 这是原生服务端控制接口，客户端原始调用号与它独立；逐项实现后登记。`gate_wait` 多字段结果须定义输出缓冲布局及失败原子性，不能假定一个返回寄存器装得下。无需重做 IPC，但可复用其唤醒/能力校验内部组件 |
| 1 / D18：rax 或整帧 | 支持 v1 只写 rax | 验证其余保存寄存器不被 reply 改写；架构固有的 syscall clobber 另计。完整用户帧、TLS、exec/信号注入留 M4，不能直接套用面向内核帧的 `exc.rs` 校验器 |
| 2 / D19：复制或映射 | 支持代理复制，不以性能原型为启动前置 | 需要显式目标 Task/root、请求存活校验、双端全范围预检及 COW 处理。4 KiB 限额不是地址授权；嵌套指针能力另需设计。M1e 测 0/64/4096 字节及跨页/COW，分别记录 TCG/KVM；没有实测不宣称复制开销可忽略 |
| 3 / D20：fork/exec | 支持暂缓，由 LibOS 用普通 reply 返回编码后的 ENOSYS | 内核不得比较 Linux 调用号，也不得只允许一种 Linux errno 的 abort。此为 M1 验证范围，不删除 M4 的 fork/exec 交付；未运行前不能把六 applet 都不派生当作证据 |
| 4 / D21：A 或 B | 优先 A，但尚未通过 COSIR 评审 | A 必须保留 W17 的真实 WaitObject/双向等待注册、专属完成授权、一次终结及审计原子性；不能仅改 Task 状态。B 不能自动修复这些问题，若 A 失败须按具体反例重审 |
| 5 / D22：阻塞或 EAGAIN | 支持有界资源下的阻塞背压；纠正“丢弃”论证 | 显式返回 EAGAIN 并审计是可审计的拒绝，不是请求凭空消失。阻塞需要已预留的请求槽、容量等待位置、FIFO 晋级及取消清理；可中断不等于有时间上界 |

## 接线前发现与反例

### R1（P1）：可中断不保证有界等待，原 G2 有不可满足的断言

提案 I4 和 D22 声称 reply/abort/中断/终止保证最终完成；G2 甚至要求服务在 Delivered
后不答复时客户端得到 ENOSYS。但活着的服务可以永不答复，请求者也可以没有信号。
无超时/监督者动作时，此执行可以无限等待，四条出口的存在不构成活性证明。

修订：I4 改为“退出路径完整、无孤儿等待注册”，明确服务存活但不答复时无有界完成保证。
G2 拆为“服务死亡自动清理”及“服务不答复，监督者显式终止服务后清理”；记录触发动作。
若坚持时间上界，必须另立超时/租约协议并重审，不能借文字宣称已满足。

### R2（P1）：Delivered 后取消并自动 RESTART 可重复外部副作用

反例：服务已把 write 的内容交给日志服务，但尚未 reply；客户端中断令请求 Cancelled，
rip 回退后建立新请求，同一内容再次写入。撤销旧请求能力只能阻止旧帧写回，不能回滚
日志。`gate_copy_out` 也可能已修改用户缓冲。

源码 `prepare_signal_resume` 使用原生调用号的特殊分支，`interrupt_for_upcall` 仅清理
既有等待注册；二者均没有 GateRequest 清理逻辑。把客户端不透明 number 放入现有
BLOCKED_CALL 表，还会错误套用原生号的 RESTART 例外。

修订：区分尚未 Delivered 的容量等待/队列取消与 Delivered 后服务执行。
前者可安全撤销并重建请求；后者 v1 不自动重放，不得仅凭收到信号就释放请求槽并返回
EINTR。需等待 reply 或服务端确认取消，并保持旧 copy/reply 在终结后全部失效。
Linux 信号/重启决策由 LibOS 在 M4 定义。一次性准确含义是**每请求至多一次终结与帧写回**，
不是所有外部副作用 exactly-once。取消完成协议仍须在模型评审中给出。

### R3（P1）：跨任务复制及其授权尚未由现有 helper 实现

`kernel/src/syscall.rs::copy_user_bytes` 读取 `sched::current_root()`，`copy_out_user`
从当前 Task 取 root 和 Segment；服务执行这些 helper 会访问服务自己的地址空间。
不能把其现有 COW 路径直接视为跨任务复制已经正确。

另一个反例：`args[i] = 0`，offset 取任意目标 VA，len=1；只检查加法溢出和 len≤4096
仍可访问几乎整个客户地址空间。即使要求 offset≤4096，也没有证明该页获得了授权。
`iovec`/`msghdr` 等嵌套指针又不直接等于某个 `args[i]`。

修订：明确服务是否被 Task 管理能力授权读取整个目标用户地址空间；若是，参数索引只是
寻址约定，不声称内存授权收窄到某个参数区间。若要参数级最小授权，必须定义可信范围
来源及派生规则。不得由不可信服务自己给出长度就宣称获得最小授权。
复制必须绑定活动 request 的目标 Task/root 及地址空间身份，检查两端范围/权限/溢出，
先完成所有可能失败的输入读取、目标页验证与 COW 准备，再提交目标字节。
冻结单个 Task 不等于冻结共享地址空间；预检到提交之间须防止其它线程改映射，不能
把 trap 时的参数地址误当作已固定的页或字节快照。
“无部分写入”只指用户可见目标字节；若还要求失败不改变 PTE/帧分配，需另做事务回滚。

### R4（P1）：零 Linux 语义与 abort/orphan 的固定 errno 冲突

I3 写“不产生 Linux errno”，ABI 又规定 abort 仅接受 ENOSYS，服务死亡和中断还由内核
生成 ENOSYS/EINTR。单纯把枚举改名不能消除数值约定依赖。

修订：普通 reply/abort 搬运不透明 u64；服务存活时由 LibOS 编码 ENOSYS。
孤儿失败值由可信挂接者在 attach 配置为不透明值，内核只搬运并另记机制终结原因；
具体 attach 参数布局待冻结，现有单参签名不是完整契约。中断返回策略同样须由服务协议
定义。最后管理/服务能力消失时，保留失败关闭的绑定：后续 trap 确定失败，不能因销毁
Gate 而恢复原生分派（Linux 原始号与 Prima 号可能碰撞）。

### R5（P1）：A 需要 W17 与能力闭合；新增两权利还超出当前位集

W17 要求每个 BlockedWait 的 wait_request 非空，关联真实活动 WaitObject，且 waiters
双向一致、pending_return/pending_wait_result 为空。把“等待对象＝请求”写在注释里
不足以满足它。普通 WaitSignal 也不能直接唤醒还没有写好返回帧的门控客户端。

请求句柄即使可派生/移动，任何别名都必须被终结状态统一拒绝；只 free 一个物理句柄
不保证一次性。服务者失去最后请求/服务能力（含位置移动）、目标销毁、审计或句柄
容量耗尽，均须纳入失败原子性与清理规则。

源码另有可复核的容量冲突：`kernel/src/rights.rs` 当前 ALL_RIGHTS=31、Rights(u32)，
追加 GateAttach/GateServe 后共 33 位，第 33 位无法表示。实现前须选定位集扩宽或按对象
编码的权限表示，并审查 `from_bits`、序列化、ABI 与 Lean 映照；不可直接追加枚举。
GateServe 的铸造/授权/交付与唯一服务者归属也尚未由只返回 GateAttach 的 attach 定义。
这些是接线前义务，现有 Lean 只覆盖其映照声明范围，不证明门控等待迁移。

### R6（P1）：队列满时缺请求安置与准入资源账

仅在 attach 检查“当前空闲”不是预留：后续 trap 仍可能耗尽 64 个请求槽，届时既无法入队，
也没有地方记录“阻塞的新请求”。需定义 WaitingCapacity → Queued → Delivered，且每个
绑定 Task 预留一个未决槽（同一 Task 同时最多一个同步 trap），满足总预留≤64；队列 8
只限制可递送项，容量等待项另有有界登记及 FIFO 晋级，不能放到无界隐含队列。

原 Gate 字段只列一个被门控 Task：若真是一对一，队列满压力例不可达；若多个 Task
共享一个服务门控，须先定义共享绑定与授权接口。此拓扑问题必须在 M1a 前确定。
准入失败、预留释放、取消/死亡、句柄/审计容量不足的状态与账目须一起验证。

## 启动执行顺序与待闭合项

1. 继续 S6-M2 的流式装载、映像容量和初始栈增量；不依赖本次把 M1 评审写成通过。
2. 按 D14 做有界 7-A 探索：两独立根视图、能力授权共享 log、跨域拒绝、单侧崩溃隔离。
   本次未运行该原型；此次是预审，不能越过正式 M1 架构评审前的检查点。
3. 在 M1a 前补齐拓扑/预留、服务能力交付、复制授权、取消完成协议、W17 映照及权限位集
   方案；增加上述反例的模型测试，再进行 COSIR 正式评审。通过后才推进 M1b trap 接线。
4. M1e 再计量复制，报告 CPU/虚拟化口径、样本量和分布。原 D19 的 20% 只作重审信号，
   不当作已有测量；映射还需独立的授权、撤销与生命周期证据。完整 7-D 并非临时映射
   的逻辑必要前置，若独立 grant 协议能证明生命周期闭合可重审；当前未有该证据。

## 命令与结果

本次只改文档。Rust/QEMU/Lean 的旧 PASS 不复用为本次证据。

| 命令/检查 | 本次结果 | 范围 |
|---|---|---|
| `python3 scripts/docs.py self-test` | PASS | Markdown/元数据/检索/生命周期/恢复及变更护栏 |
| `python3 scripts/docs.py build` | PASS：329 documents | 生成目录，不手改 catalog |
| `python3 scripts/docs.py check` | PASS | 目录一致性、链接与锚点 |
| `python3 scripts/docs.py check --base 9a9f4a2eba4f779a73a23f700cc556c48d89c803` | PASS | 新文档元数据与相对基线变更 |
| `cargo fmt --all --check` | PASS | 格式 |
| `cargo test --workspace` | 沙箱内及获准沙箱外重跑均退出 101：bootloader 构建依赖无法解析 rsproxy.cn | 未完成测试，不能计为 Rust PASS |
| `cargo xtask lint` | 沙箱内及获准沙箱外重跑均退出 101：同一 bootloader/registry DNS 问题 | xtask 前置构建失败，不能计为 lint PASS |
| `git diff --check`；基线与当前 S6 勾选比较 | PASS | 无空白错误；5 项交付和 4 项退出条件均保持未勾选 |
| QEMU / Lean / 差分 / 复制计量 / 7-A 原型 | 未运行 | 本次为文档和静态预审，未新增对应实现 |

保留失败：首次沙箱日志还包含 Cargo 缓存 `attempt to write a readonly database`；
获准沙箱外重跑仍在 registry DNS 失败，故未以放宽权限推断验证成功。
本机临时日志为 `/tmp/prima-stage6-review-workspace.log`、`/tmp/prima-stage6-review-lint.log`、
`/tmp/prima-stage6-review-workspace-escalated.log`、`/tmp/prima-stage6-review-lint-escalated.log`；
这些日志不入库且不是可长期恢复的工件，本报告保留命令、失败原因与实际退出状态。

## 阶段影响与后继关系

原 D17–D22 文书保留，新增复审指针；提案纠正本报告指出的冲突措辞并列出冻结前义务。
本报告只否定“当前设计已可接线/保证有界等待/现有 helper 可直接跨任务使用”等主张，
不撤销六条路线选择、不签署 COSIR 扩展、不更改 profile v2/SPEC-152。
当前阶段入口仍是[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。
重审触发：完成 7-A 原型、提交上述修订模型或新增运行期反例；需逐项引用 R1–R6 的闭合证据。
