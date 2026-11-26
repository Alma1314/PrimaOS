---
id: reports-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-11-27
---

# Reports 索引

2026-11-11：[LibOS数据通路共享批量首片](2026-11-10-libos-data-path.md)；P1完整36组72轮BIOS通过，P2–P6待实现，未完成项和验证边界见正文。

2026-11-16：[P2a代理会话与受限根视图](2026-11-16-stage7-proxy-session.md)；管理器签发、Store绑定与视图原语通过host/lint及既有72轮BIOS回归；Linux数据面迁移、镜像接线和代理QEMU正反例仍未实现。

2026-11-17：[P2b-0/P2b-1 Store批量数据面](2026-11-17-p2b-store-batch-ops.md)；op矩阵与Store侧OPEN/STAT/CLOSE/GETDENTS/LSEEK/STAT_PATH通过host/workspace/lint；Linux前端、fork/exit、镜像与QEMU未做。

2026-11-17：[P2b-2 Linux前端切换](2026-11-17-p2b-linux-frontend.md)；前端经Session/Local覆盖8个op，workspace 1166与既有Linux差分22轮通过；真实代理会话、fork/exit、镜像与代理QEMU正反例仍未完成。

2026-11-17：[P2b-3 fork/exec/exit生命周期](2026-11-17-p2b-fork-exit.md)；owner预留/发布/回滚/退出关闭迁到Store接口并新增代理消息，workspace 1169与既有Linux差分22轮通过；真实会话接线、镜像与代理QEMU正反例仍未完成。

2026-11-18：[P2b-4 镜像接线与两 ABI 场景](2026-11-18-p2b-proxy-wiring.md)；管理器签发文件/代理会话，Linux personality 经真实 Remote 读 Store 的 `/linux/fixture.txt`，原生应用读 `/README`；workspace 1169、lint、`test-linux-proxy` 与既有差分通过，正反例矩阵待P2b-5。

2026-11-18：[P2b-5 代理路径正反例矩阵](2026-11-18-p2b-proxy-matrix.md)；两次 QEMU 启动覆盖 Linux 私有读/代理 fork/共享授权/fork 限额与原生策略视图七条负例；以能力绑定取代不可满足的内核父系校验，workspace 1170、lint 与既有差分 22 轮通过；P2b-6 收口待做。

2026-11-18：[P2b-6 P2b 收口](2026-11-18-p2b-closeout.md)；汇总 P2b-0…P2b-5 证据并裁决保留 `Direct`，完整 BIOS 套件 36 程序/72 轮通过；P2 不签署、7-A 未勾选，组织级复核待做，P3–P6 未开始。

2026-11-18：[P3-0 异步批量契约与状态机](2026-11-18-p3-async-state-machine.md)；把 P3 草案落成 Store/Producer 纯状态机，改用 IPC 顺序而非裸共享原子，host 有界穷举交错与回绕用例通过；未接线、无 QEMU，P3 未收口。

2026-11-18：[P3-1 Store v2 会话](2026-11-18-p3-store-v2.md)；5 页控制/数据段、ASYNC_* tag、公平排空、取消/重置/EBUSY/ESTALE 与死亡回收落盘；host 1181、lint、既有 72 轮与 `test-linux-proxy` 通过；Store 栈 4→8 页修复既有代理路径溢出；无 v2 前端/QEMU，P3 未收口。

2026-11-19：[P3-2 前端 v2 接线](2026-11-19-p3-frontend-v2.md)；Store 直连 v2、`Remote` v2 管线、exec 重开、独立代理 tag 族与服务栈 2→4/Store→16 页落盘，72 轮与代理矩阵通过；C 数据面 v2 在 W6/W7b 暴露 fork 后父进程 fd EACCES 已回退，C 锁拆分未开始，P3-2 未关闭。

2026-11-24：[P3-2 C 数据面 v2 与 fork 反例修复](2026-11-24-p3-frontend-v2-data-plane.md)；C 的 read/write/pread/pwrite 切到直连 5 页 v2 会话，fork/exec 统一关闭修复 W6/W7b，per-op 失败推进序号修复 V4 悬挂；workspace 1181、musl 7、lint、W4–W9、V1–V4 与完整 72 轮通过；C 锁拆分未开始，P3-2 未关闭。

2026-11-24：[P3-2 C 适配层数据面锁拆分](2026-11-24-p3-adapter-lock-split.md)；read/write/readv/writev/pread/pwrite 改为短表锁快照加每 wire fd 在途计数与延迟 CLOSE，不再跨同步 IPC 持进程锁；控制面 open/close/路径仍持表锁，工作项 3 未完整，P3-2 未关闭；musl 7、V1–V4、W4–W9 与完整 72 轮通过。

2026-11-25：[P3-2 C 适配层控制面锁拆分](2026-11-25-p3-adapter-control-lock.md)；open/close/dup/fcntl/stat/路径/目录/termios/socket 改为短表锁本地段加锁外 IPC，在途引用扩展到目录与 socket，fork 不再持表锁；host 1181、musl 7、lint、V1–V4、W4–W9、代理矩阵与完整 72 轮通过；工作项 3 完整。

2026-11-25：[P3-3 v2 正反例矩阵](2026-11-25-p3-async-matrix.md)；V5 验收镜像与 `test-async` 两轮 10 个 case：合并完成、满窗 EAGAIN、重放、回退/保留位重置、取消前 ECANCELED、两会话公平与单侧退出后继续；认领后取消与页改写快照登记为限制；P3 未收口、7-A 未勾选。

2026-11-25：[P3 收口证据包](2026-11-25-p3-closeout.md)；汇总 P3-0…P3-3 交付与命令、保留失败历史、登记未覆盖项与复核问题；作为组织级复核输入，不称 P3 已复核，7-A 未勾选。

2026-11-26：[P4-0 Gate/IPC 分段测量基线](2026-11-26-p4-baseline.md)；I1 64 次、N1 128 次、G5 128 次各两轮；本机 G5 的 `queue-deliver` 约占内核时间 48%、`deliver-reply` 约 29%；列出 gate 查找、调度/帧、`gate_copy` scratch 等候选；不改代码、不设门限。

2026-11-26：[P4-1 Gate 递送/恢复分段确认](2026-11-26-p4-segmentation.md)；把 queue-deliver/deliver-reply 映射到两次任务切换与 FXSAVE/FXRSTOR、CR3、iretq，确认现有 5 阶段不足以细分；候选排序先补细分探针，再评估 FPU 惰性、绑定索引与 `gate_copy` scratch；不改代码。

2026-11-26：[P4-2a G5 细分采样探针](2026-11-26-p4-segmentation-probe.md)；新增服务/客户端 resume 入口与 `GATE_WAIT` 内核入口，拆出 queue-wait/switch-service/wait-output 与 reply-dispatch/resume-client；旧 G4 事件与 G5 旧字段冻结，新增 `S7PHASE` 行；G4/G5 各两轮、完整 72 轮通过；未实现优化。

2026-11-26：[P4 收口证据包](2026-11-26-p4-closeout.md)；汇总 P4-0…P4-2a 方法与样本，P4-2 候选评估保留现状；P4-3 回归 G4/G5/I1/N1/C1 各两轮、workspace 1205、lint 与完整 72 轮通过；P4 未关闭、门槛未冻结、7-A 未勾选。

2026-11-26：[WIN-M1 PE/ABI 宿主模型](2026-11-26-windows-m1-pe-model.md)；新增 `prima-windows` 的有界 PE32+ 解析、严格 RVA/映射校验与 Win64 ABI 探针，PE 21/21、ABI 4/4 宿主通过；真实 PE 只验证头与 `.text`，无导入/重定位/TLS/装载/Windows 参考差分。

2026-11-26：[WIN-M2 前置平铺装载模型](2026-11-26-windows-m2-preflight.md)；`load_into` 在宿主完成平铺复制、DIR64/HIGHLOW 重定位与 IAT 填充，PE 32/32、ABI 4/4 通过；无页映射、Task 发布、TLS 初始化或 ring3 执行，真实 PE 仅首个基址装载。

2026-11-26：[WIN-M2 ring3 原型](2026-11-26-windows-m2-ring3.md)；专用 feature 下 servicemgr 把 clang/lld 原样 PE32+ 经平铺/重定位/IAT 写入 R+X 代码段，两轮 QEMU 各运行两个子任务并自毁，结果 42、exit=self、父服务继续、exit=33；替代导入为手写桩，非完整 M2。

2026-11-27：[WIN-M2 真实 DLL 导出与发布失败清理](2026-11-27-windows-m2-real-dll.md)；解析 `winpe-stub.dll` 导出表替换手写导入桩并把 DLL 以 R+X 映射，新增发布失败负例（held Task 已建 R+X 映射后强制失败，清理并用同一 VA 重新装载）；专用 QEMU 连续 8 轮逐字节通过、host 1223、lint 通过；首次运行一次未复现的 `task_create_held` 失败已登记，无 Windows 参考差分，WIN-M2 未通过。

2026-11-27：[WIN-M3 部分交付](2026-11-27-windows-m3-loader-runtime.md)；导出区分函数/转发器并新增 `export_by_ordinal` 与 `ModuleSet`（大小写/后缀归一、转发限深 8），新增 TEB/PEB 冻结子集与静态 TLS 模板宿主模型，ring3 改为三 DLL + `Fwd->winpe-stub.Stub` 转发；QEMU 两轮逐字节通过、host 1234、lint 通过；无用户 GS，动态 TLS/运行期 TEB/PEB、API-set 清单、`DllMain`、堆/错误与参考差分未覆盖，WIN-M3 未通过。

2026-11-27：[P5-1 ACPI/MADT 解析与只读探测](2026-11-27-p5-1-acpi-madt-host.md)；新增 RSDP v0/v2、RSDT/XSDT、MADT 纯解析与 `smp_probe`（CPUID + LAPIC 自识别），专用 `test-smp` 在 `-smp 2/4` 各两轮通过并复用既有启动校验器，剔除 SMP 行后两种 vCPU 数日志逐字节相同；host 1245、lint、默认 72 轮通过；**AP 未启动**（`online=1`），7-B 未勾选，P6 未开始。

2026-11-27：[P5-0 SMP 状态所有权设计](2026-11-27-p5-0-state-partition.md)；按 D15 给出每 CPU/任务级/全局共享/只读的状态分类、AP 启动与 IPI/TLB 分期、锁顺序与六项待决，并把调度当前任务迁到 `cpu::local`（BSP 初值不变）；host 1236、lint、完整 BIOS 36 组 72 轮通过；未启动 AP、未做 `-smp 2/4`，7-B 未勾选，P6 未开始。

历史统一验收：[阶段0–5缺口后继](2026-10-25-stage0-5-gap-followup.md)，workspace1016、lint、Lean、Rust/musl、compat及完整单次BIOS68轮通过；保留失败修复和时序断言修订，最终范围/阶段签署待确认。

本目录保存**实现报告与审查/复核证据**：记录实际命令、结果、计数、反例和证据边界。历史报告保留其写作时的结论，后续复核可能改变适用范围。

> 本索引按阶段/主题分组，文件日期用于历史定位。新增文档同步更新本页和相关主题路由，再运行 `python3 scripts/docs.py build` / `check`。统一入口见[文档中心](../../README.md)。

## OSTD / 跨项目代码复用

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-asterinas-once-publication-report.md](2026-10-24-asterinas-once-publication-report.md) | Asterinas 参考续篇：一次初始化与启动发布验证 |
| 2026-10-24 | [2026-10-24-asterinas-pod-buffer-report.md](2026-10-24-asterinas-pod-buffer-report.md) | Asterinas 参考续篇：POD 与恢复帧缓冲边界 |
| 2026-10-24 | [2026-10-24-asterinas-reuse-report.md](2026-10-24-asterinas-reuse-report.md) | Asterinas 复用与 OSTD 临界区验证 |

## 跨阶段 / 文档体系

阶段0–6本轮后继：[跨阶段实现审查与整改](2026-11-06-stage0-6-review-remediation.md)，线程共享能力、旧代次容器清理、IPC/Task事务预算及Linux信号/向量I/O边界；实际验证与限制见报告。

文档维护：[日志附件内嵌整理](2026-10-31-documentation-log-inline.md)，131个日志／命令输出附件的关键证据迁入13份原报告，完整旧附件保留Git恢复路径；不新增OS验证或改变阶段结论。

源码细化：[生产 Rust 句柄方法→Lean](2026-10-31-rust-handle-source-refinement.md)，Charon/Aeneas 自动提取、生成物新鲜度、5 条一般定理与真实 Rust mutation；范围限构造/后继值，翻译器为可信边界。 主分支整合复核见该报告末节。

形式化后继：[有限能力槽表组合证明](2026-10-31-kernel-handle-table-proof.md)，10 条一般 Lean 定理、整表任意序列无复活、真实 Rust 三槽六步回归；不宣称 Rust 源码细化证明。

2026-11-25 形式化后继：[Lean TLB 缓存最小参考模型](2026-11-25-lean-tlb-refine.md)，`Formal.Tlb` 的 31 条定理通过固定版本 `lake build`、公理仅 `propext`/`Quot.sound`；抽象页表与守卫边界、7-B/7-H 不勾选、无源码细化均见正文。

2026-11-25 形式化后继：[Lean Memseg 双视图与帧归属最小模型](2026-11-25-lean-memseg-refine.md)，`Formal.Memseg` 的 29 条定理通过固定版本 `lake build`、公理仅 `propext`/`Quot.sound`；四级树、多页原子与 `StillMapped` 枚举的抽象边界见正文。

2026-11-25 形式化后继：[Lean Cow 组合最小模型](2026-11-25-lean-cow-refine.md)，`Formal.Cow` 的 25 条定理通过固定版本 `lake build`、公理仅 `propext`；COW 建立/写故障/私有化/调度/销毁覆盖，capability、异常源与 TLB 失效为抽象边界。

2026-11-25 形式化后继：[Lean Revoke 撤销域最小模型](2026-11-25-lean-revoke.md)，`Formal.Revoke` 的 25 条定理通过固定版本 `lake build`、4 条无公理其余仅 `propext`/`Quot.sound`；子树判定为可执行有界 `Bool` 遍历（`WellFormed` 下完备），Bootstrap 子树失效、HeldBy 删除与非 HeldBy 保留为抽象边界。

2026-11-25 形式化后继：[Lean cap_drop 最小模型](2026-11-25-lean-drop.md)，`Formal.Drop` 的 10 条定理通过可执行 `#eval` 锚与固定版本 `lake build`，公理仅 `propext`/`Quot.sound`；单位置删除、不传播与 structural 守卫覆盖，订阅终止与审计事件为边界。

2026-11-25 形式化后继：[Lean Move 位置迁移与 W3 唯一位置最小模型](2026-11-25-lean-move.md)，`Formal.Move` 的 15 条定理通过可执行 `#eval` 锚与固定版本 `lake build`，7 条无公理其余仅 `propext`；转移表、原子更新、从/到与直投 `MsgId` 记录、删除与跨节点不移动覆盖，W4 容器双向一致与审计事件为边界。

2026-11-25 形式化后继：[Lean W4 容器/位置双向一致最小模型](2026-11-25-lean-container.md)，`Formal.Container` 的 18 条定理通过可执行 `#eval` 锚与固定版本 `lake build`，1 条无公理其余仅 `propext`/`Quot.sound`；HeldBy/InMessage/InNamespace 双向一致与四个耦合更新的 W3+W4 保持覆盖，列表级重复/路径唯一、生命周期与审计事件为边界。
2026-11-25 形式化后继：[Lean §12.4/W15 订阅生命周期最小模型](2026-11-25-lean-subscription.md)，`Formal.Subscription` 的 39 条定理通过固定版本 `lake build`，2 条无公理其余仅 `propext`/`Quot.sound`；关系步骤与可执行函数桥接覆盖订阅表、usable 见证、subscribe/unsubscribe、失去最后授权 Cap 的原子终止与源销毁，lastness 检查器、活性与审计事件为边界。
2026-11-25 形式化后继：[Lean §13.1/W13 审计日志核心最小模型](2026-11-25-lean-audit.md)，`Formal.Audit` 的 19 条定理通过固定版本 `lake build`，2 条无公理其余仅 `propext`；信封、追加式日志、严格递增 ID、位置历史、created_at、Task 状态与对象创建事件覆盖，载荷完整性与 audit 查询为边界。
2026-11-25 形式化后继：[Lean §13.2 audit 查询最小模型](2026-11-25-lean-audit-query.md)，`Formal.AuditQuery` 的 20 条定理通过固定版本 `lake build`，仅 `propext`/`Quot.sound`；父链成员健全/长度上界、`WellFormed` 下燃料完备性、InvalidCap/NotHeld、结果字段与已销毁目标覆盖，完整载荷为边界。

阶段 1～5 的当前后继整改：[VFS 内容别名与 musl 检查护栏](2026-10-31-stage1-5-regression-remediation.md)，保留历史签署及其限制。

独立审查：[实现质量独立审查](2026-10-27-implementation-quality-review.md)，host 全门禁实测（1018 测试、lint、TCB 6.42%、docs check）、R1 双轮启动通过，并复现 musl 审计的 locale 依赖缺陷；不改阶段签署。

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-27 | [2026-10-27-implementation-quality-review.md](2026-10-27-implementation-quality-review.md) | 实现质量独立审查：门禁可复现性与结构负债（公开缺陷：musl 审计 locale 依赖） |
| 2026-10-24 | [2026-10-24-documentation-lifecycle-report.md](2026-10-24-documentation-lifecycle-report.md) | 文档生命周期治理与过时材料清理 |
| 2026-10-23 | [2026-10-23-documentation-restructure-report.md](2026-10-23-documentation-restructure-report.md) | 文档体系重构与验收报告 |

## 阶段 5

**出口签署（2026-10-27）**：[阶段 5 出口签署 SPEC-152](2026-10-27-stage5-exit-signoff.md)，冻结有界 profile v2、确认 D1–D10 处置、按阶段 5 范围关闭 BLK-003；签署复跑与两处夹具修复记录在该报告，历史失败保留。

POSIX std最新续做：[文件I/O与接收归账](2026-10-27-posix-std-file-io-report.md)，单次36组72/72轮、workspace1020、lint与三模式Clippy通过；初始69/70失败保留，底层支持仍有界。

本批增量：[启动、栈保护、退出与 Rust 工作目标](2026-10-25-stage5-gap-closure-report.md)。

本轮进程后继：[fork 续执行与进程边界整改记录](2026-10-24-fork-continuation-report.md)。

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-musl-port-boundary-report.md](2026-10-24-musl-port-boundary-report.md) | musl 移植补强：调用边界与 fd/stdio 验证 |
| 2026-10-23 | [2026-10-23-vfs-libos-foundation-report.md](2026-10-23-vfs-libos-foundation-report.md) | VFS / LibOS 基础重构验证报告 |
| 2026-10-22 | [2026-10-22-stage5-m11-file-mutation.md](2026-10-22-stage5-m11-file-mutation.md) | 阶段 5 M11：文件树变更（`mkdir`/`rmdir`/`unlink`） |
| 2026-10-22 | [2026-10-22-stage5-m12-c-startup.md](2026-10-22-stage5-m12-c-startup.md) | 阶段 5 M12：C 启动栈与退出清理（envp/auxv、自动关闭 fd） |
| 2026-10-22 | [2026-10-22-stage5-m13a-vfs-links.md](2026-10-22-stage5-m13a-vfs-links.md) | 阶段 5 M13a：符号链接/readlink/rename/fchdir、内容区压实 |
| 2026-10-22 | [2026-10-22-stage5-m13b-permissions-stdio.md](2026-10-22-stage5-m13b-permissions-stdio.md) | 阶段 5 M13b：节点 mode、chmod/fchmod/umask、readv 与 stdio 文件 API |
| 2026-10-23 | [2026-10-23-stage5-m19-loopback-udp.md](2026-10-23-stage5-m19-loopback-udp.md) | 阶段 5 M19：用户态 loopback IPv4/UDP、socket/端口/数据报 |
| 2026-10-23 | [2026-10-23-stage5-m14-tty-input.md](2026-10-23-stage5-m14-tty-input.md) | 阶段 5 M14：fd 0 tty 输入、canonical/raw、termios 子集 |
| 2026-10-23 | [2026-10-23-stage5-m15a-waitobject.md](2026-10-23-stage5-m15a-waitobject.md) | 阶段 5 M15a-1：WaitObject 创建/观察/信号/立即等待/销毁 |
| 2026-10-23 | [2026-10-23-stage5-m15a2-waitobject-blocking.md](2026-10-23-stage5-m15a2-waitobject-blocking.md) | 阶段 5 M15a-2：BlockedWait/唤醒/超时/结果注入 |
| 2026-10-23 | [2026-10-23-stage5-m15b1-process-metadata.md](2026-10-23-stage5-m15b1-process-metadata.md) | 阶段 5 M15b-1：Process/Thread 元数据 |
| 2026-10-23 | [2026-10-23-stage5-m15b2a-thread-create.md](2026-10-23-stage5-m15b2a-thread-create.md) | 阶段 5 M15b-2a：共享能力空间的 thread_create（QEMU 容量阻塞） |
| 2026-10-23 | [2026-10-23-stage5-thread-capacity-investigation.md](2026-10-23-stage5-thread-capacity-investigation.md) | ISSUE-THREAD-CAPACITY-001 调查：扩容破坏 task_ready 见证 |
| 2026-10-23 | [2026-10-23-stage5-m15b2b-thread-slots.md](2026-10-23-stage5-m15b2b-thread-slots.md) | 阶段 5 M15b-2b：线程槽位回收、W2 端到端唤醒 |
| 2026-10-23 | [2026-10-23-stage5-m16a-futex.md](2026-10-23-stage5-m16a-futex.md) | 阶段 5 M16a：futex WAIT/WAKE（用户态表 + WaitObject） |
| 2026-10-23 | [2026-10-23-stage5-m16b1-clone-basis.md](2026-10-23-stage5-m16b1-clone-basis.md) | 阶段 5 M16b-1：clone/线程退出内核基础与 musl clone.s 阻塞 |
| 2026-10-23 | [2026-10-23-stage5-m16b2-musl-clone.md](2026-10-23-stage5-m16b2-musl-clone.md) | 阶段 5 M16b-2：musl clone.s 补丁、pthread_create 成功、join 唤醒待解 |
| 2026-10-23 | [2026-10-23-stage5-m16b3-pthread-join.md](2026-10-23-stage5-m16b3-pthread-join.md) | 阶段 5 M16b-3：clear_child_tid、pthread create/join、共享内存可见性 |
| 2026-10-23 | [2026-10-23-stage5-m17b1-wait4-model.md](2026-10-23-stage5-m17b1-wait4-model.md) | 阶段 5 M17b-1：wait4 状态机模型（阻塞/唤醒/回收/ECHILD） |
| 2026-10-23 | [2026-10-23-stage5-m17b2-cow-primitives.md](2026-10-23-stage5-m17b2-cow-primitives.md) | 阶段 5 M17b-2a：COW 页表克隆/写故障原语 |
| 2026-10-23 | [2026-10-23-stage5-m17b2-cow-fork.md](2026-10-23-stage5-m17b2-cow-fork.md) | 阶段 5 M17b-2b：general COW、fork syscall、W6 部分通过 |
| 2026-10-23 | [2026-10-23-stage5-m17b3-zombie-wait.md](2026-10-23-stage5-m17b3-zombie-wait.md) | 阶段 5 M17b-3：fork 僵尸保留、唤醒回收（W6 未接入） |
| 2026-10-23 | [2026-10-23-stage5-m17b4-fork-wait.md](2026-10-23-stage5-m17b4-fork-wait.md) | 阶段 5 M17b-4：fork/wait 打通（W6 双轮） |
| 2026-10-23 | [2026-10-23-stage5-m17c1-elf-plan.md](2026-10-23-stage5-m17c1-elf-plan.md) | 阶段 5 M17c-1：静态 ELF 解析与初始栈（host） |
| 2026-10-23 | [2026-10-23-stage5-m18a-signal-kill.md](2026-10-23-stage5-m18a-signal-kill.md) | 阶段 5 M18a：kill 默认终止与 wait4 信号状态（W8） |
| 2026-10-23 | [2026-10-23-stage5-m18b-sigaction.md](2026-10-23-stage5-m18b-sigaction.md) | 阶段 5 M18b：sigaction/handler/sigreturn（W9） |
| 2026-10-23 | [2026-10-23-stage5-m17c3-exec.md](2026-10-23-stage5-m17c3-exec.md) | 阶段 5 M17c-3：exec_replace（W7） |
| 2026-10-23 | [2026-10-23-stage5-m17c4-execve-vfs.md](2026-10-23-stage5-m17c4-execve-vfs.md) | 阶段 5 M17c-4：VFS 镜像 exec（W7b） |
| 2026-10-23 | [2026-10-23-stage5-m20-compat-final.md](2026-10-23-stage5-m20-compat-final.md) | 阶段 5 M20：兼容矩阵定稿 |
| 2026-10-23 | [2026-10-23-stage5-m21-exit-review.md](2026-10-23-stage5-m21-exit-review.md) | 阶段 5 M21：出口评审（未签署） |
| 2026-10-21 | [2026-10-21-stage5-m10-anon-memory.md](2026-10-21-stage5-m10-anon-memory.md) | 阶段 5 M10：匿名内存与保护（`mmap`/`munmap`/`mprotect` + 内核 `memseg_protect`） |
| 2026-10-21 | [2026-10-21-stage5-m9-metadata-cwd.md](2026-10-21-stage5-m9-metadata-cwd.md) | 阶段 5 M9：`statx` 元数据、`chdir`/`getcwd` 与 sbase `cat`/`ls` |
| 2026-10-20 | [2026-10-20-stage5-m8-user-vfs.md](2026-10-20-stage5-m8-user-vfs.md) | 阶段 5 M8：用户态 VFS/rootfs、通用 fd 与多级路径 |
| 2026-10-19 | [2026-10-19-stage5-m7-fd-table.md](2026-10-19-stage5-m7-fd-table.md) | 阶段 5 M7：完整本地 fd 表、`dup` 与 `getpid` |
| 2026-10-19 | [2026-10-19-stage5-m6-time.md](2026-10-19-stage5-m6-time.md) | 阶段 5 M6：POSIX 时间首切片（clock_gettime / nanosleep） |
| 2026-10-19 | [2026-10-19-stage5-m5-sbase-echo.md](2026-10-19-stage5-m5-sbase-echo.md) | 阶段 5 · M5 首切片：sbase `echo` E1–E6 |
| 2026-10-18 | [2026-10-18-stage5-task16-qemu.md](2026-10-18-stage5-task16-qemu.md) | 阶段 5：服务后 Task 16 的 QEMU 见证 |
| 2026-10-18 | [2026-10-18-stage5-ptload-wx.md](2026-10-18-stage5-ptload-wx.md) | 阶段 5：`PT_LOAD` 写段/BSS 私有装载与 W^X |
| 2026-10-18 | [2026-10-18-stage5-native-abi-check.md](2026-10-18-stage5-native-abi-check.md) | 阶段 5：原生 ABI 版本握手见证 |
| 2026-10-18 | [2026-10-18-stage5-musl-m4-file.md](2026-10-18-stage5-musl-m4-file.md) | 阶段 5 / M4：文件与 fd 生命周期（首切片） |
| 2026-10-18 | [2026-10-18-stage5-musl-m3-base.md](2026-10-18-stage5-musl-m3-base.md) | 阶段 5 · musl M3：SSE/FPU 契约与静态 musl 基座 |
| 2026-10-18 | [2026-10-18-stage5-musl-m2-boundary.md](2026-10-18-stage5-musl-m2-boundary.md) | 阶段 5 · musl M2：C 系统边界与 per-task FS base |
| 2026-10-18 | [2026-10-18-stage5-musl-m0-inventory.md](2026-10-18-stage5-musl-m0-inventory.md) | 阶段 5：musl M0 依赖盘点 |
| 2026-10-18 | [2026-10-18-stage5-main-return-status.md](2026-10-18-stage5-main-return-status.md) | 阶段 5：同源 C `main` 返回码见证 |
| 2026-10-18 | [2026-10-18-stage5-kickoff.md](2026-10-18-stage5-kickoff.md) | 阶段 5 启动：首个新 Task 容量与 profile 草案 |
| 2026-10-18 | [2026-10-18-stage5-entry-analysis.md](2026-10-18-stage5-entry-analysis.md) | 阶段 5 准入分析：POSIX 源码兼容 |
| 2026-10-18 | [2026-10-18-stage5-capacity-watermarks.md](2026-10-18-stage5-capacity-watermarks.md) | 阶段 5：服务后静态容量水位 |
| 2026-10-18 | [2026-10-18-stage5-c-tty-write.md](2026-10-18-stage5-c-tty-write.md) | 阶段 5 片 C：tty 字节流、`write(1/2)` 与同源 `_exit` 见证 |
| 2026-10-18 | [2026-10-18-stage5-c-start-stack.md](2026-10-18-stage5-c-start-stack.md) | 阶段 5：C Task 初始栈见证 |
| 2026-10-18 | [2026-10-18-stage5-c-elf-qemu.md](2026-10-18-stage5-c-elf-qemu.md) | 阶段 5：首个 C ELF 的服务后 QEMU 见证 |

## 阶段 7（进行中）

2026-11-08发布输入后继：[根Cargo图依赖源码与NOTICE材料](2026-11-08-stage7-cargo-materials.md)，
68包/60份原始归档、110份文本候选与3处逐归档缺口；191成员包的锁定校验和、独立复核/确定性、
host/lint及两套打包测试通过，不构成完整镜像材料或新的QEMU/Lean证据。

2026-11-08工程首片：[源码/验收证据包及73056ba自身干净远端](2026-11-08-stage7-evidence-package.md)，
九组/97份归档核验与历史Git源码绑定、许可事实、154成员包的确定性及篡改/混版/失败反例通过；
本地host/lint通过，不包含镜像字节，完整二进制发布材料与镜像包仍待完成。

2026-11-08继续：[L1端到端LibOS基准与C1自身远端后继](2026-11-08-stage7-libos-benchmark.md)，
8933a5c干净远端九组/94份归档核验；五类单核方法/基线齐备；实现提交e6dce41自身干净远端九组/97份归档、普通36组72轮及L1专项通过，
本机两轮超时中断保留；多核、门槛及发布检查仍待完成。

2026-11-08后继：[C1 COW基准与整改自身远端核验](2026-11-07-stage7-cow-benchmark.md)，
a40c487干净远端九组/92份归档、本地C1九配置18轮与完整回归；五类基准已有四类有界单核基线，阶段7仍进行中。

2026-11-07后继：[IPC审计排序整改与I1远端核验](2026-11-07-stage7-ipc-audit-order.md)，
Task/异常/Timer投递排序及精确预算反例本地通过；I1加强逐条三事件检查，2e3b95f远端九组/92份归档通过。
新整改仍待自身远端；其余两类基准、多核、门槛与发布检查继续待完成。

2026-11-07后继：[I1授权IPC往返与N1远端核验](2026-11-07-stage7-ipc-benchmark.md)，
N1提交38f92bb远端九组/90份归档通过，I1方法与本地验证按该报告范围解释。

2026-11-07继续：[N1原生syscall基线与G5远端后继](2026-11-07-stage7-native-syscall-benchmark.md)，
fe83e59干净远端九组及88份归档核验通过；N1本地七配置14轮、host1137、lint与普通72轮通过。
原生syscall与Gate已有有界单核基线，其余三类、多核、门槛与发布检查仍待完成。

2026-11-07继续：[G5用户态Gate往返基线与普通回归](2026-11-07-stage7-gate-user-benchmark.md)，
faf551a合入master后远端九组通过，86份归档及探针身份核对通过；G5本地六配置双轮、host1136、
lint/特性Clippy与普通36组72轮通过。Gate一类取得有界单核user-wrapper基线，其余四类/多核/门槛待完成。

2026-11-07后继：[九组CI通过与可复核Gate基线](2026-11-07-stage7-ci-green-gate-baseline.md)，
1921757的干净远端九组通过；后继补齐逐步原始串口与探针子目录制品身份，本地Gate五配置双轮及host/lint通过。
G4为最终iret前的内核路径数据，不能代替五项基准、完整用户态往返或性能门槛。

2026-11-07：[首次远端CI与bootloader准备整改](2026-11-07-stage7-ci-remediation.md)，
首次九组中docs/formal通过，七组在嵌套依赖缺失处失败；本地修复与验证按报告范围解释。
五类基准名称已获作者确认，具体方法/门槛、修复后远端九组和发布检查继续待完成。

2026-11-06：[启动与7-0共享工件互斥](2026-11-06-stage7-start.md)。作者已指示启动，
首片提供跨进程镜像资源互斥；验证范围与剩余冻结门见报告，不关闭任何子轨退出条件。
后继[CI分组、锁定输入与验收身份](2026-11-06-stage7-ci-evidence.md)记录九组工作流接线、
本地实跑与失败链，远端CI和性能基准仍待完成。

## 阶段 6（有界技术退出复核通过）

2026-11-06继续：[Gate请求根的审计承载历史](2026-11-06-gate-request-audit-history.md)承接RV-1，
将请求根完整历史放入原交付审计、解除1279次累计交付上限；通用能力预算与审计fail-stop保持。
本次运行证据和限制以该报告为准，首轮边界登记保留。

2026-11-06：[阶段 6 实现复核](2026-11-06-stage6-implementation-review.md)：
独立复现 host 1118/1119、lint 41 文件零 unsafe、TCB 5.15%、Linux 同源 11 项双轮差分与完整
BIOS 72/72；记录 2 项中等（单次启动约 1279 次 Gate 交付上限、18 槽审计扫描）、4 项低风险与
1 项契约漂移（外部帧 768/1024）；不改变阶段状态与勾选。

2026-11-06：[实现复核](2026-11-06-stage6-implementation-review.md)记录RV-1–RV-7；
[整改后继](2026-11-06-stage6-review-remediation.md)登记交付上限与1024帧契约修订，补齐家族审计容量、
游标检查、D17单表与窄口径personality门禁；验证范围及既有原生clone适配器的审查纠偏见后继报告。

2026-11-02：[进程、线程信号及逐调用差分后继](2026-11-02-stage6-process-and-differential.md)：
87–89、pthread/进程/逐调用见证及原始审计已双轮差分通过；host、lint、G1–G4与七边界通过；最终完整72/72及有界出口复核通过，profile v1冻结。下列切片保留各自当时范围。

2026-11-01继续：[独立上下文、Linux信号和共享线程/futex](2026-11-01-stage6-context-signals-threads.md)：
83–86、TaskContext、GPR/FXSAVE答复、P1/P2原始ELF与八项差分工程，专用BIOS/UEFI构建成功；
编译通过，未执行任何测试/QEMU，剩余M4和整体验收不变。

2026-11-01续做：[受管理装载与Linux专用镜像](2026-11-01-stage6-managed-linux-loader.md)：
79–82、用户态personality、六applet差分harness与manifest v3，已编译并构建BIOS/UEFI，
未执行任何测试，不宣称Linux白名单已经运行或profile冻结。

2026-11-01：[计量探针、内核映射跨度与运行时 Gate 封装](2026-11-01-stage6-measurement-and-boot-span.md)：
本轮仅完成部分代码与编译检查；按“全部完成后统一测试”要求尚未执行测试。整阶段目标未完成，
未关闭 M1e/S6-D2 或任何退出条件；当前进度不以此报告替代阶段跟踪。

2026-11-01：[Gate代理复制与生命周期](2026-11-01-stage6-gate-copy-lifecycle.md)实现75/76，补齐G2/G3矩阵并修正满队列错误晋级、补齐孤儿trap原始审计；workspace1095、lint与G1/G2/G3各双轮PASS，最终完整72/72，首次71/72的I1差异保留；
[W7分段诊断](2026-11-01-stage6-w7-timing-diagnosis.md)记录BIOS读盘/执行阶段与临时strip失败。最终统一回归及限制以报告为准，不改写前序失败。

2026-11-01：[RAM复制、四段几何与八页栈](2026-11-01-stage6-loader-geometry.md)，S6-M2后继；workspace1091、lint和Linux参考通过，完整BIOS60/72；单次QEMU诊断26.63秒完成geometry=1，隔离W7双轮仍超时；合成入口，不代表冻结busybox运行。

2026-10-31：[原生 Gate 控制失败尝试审计](2026-10-31-stage6-gate-control-rejections.md)，73/74/78/88 的拒绝共用记录身份、原始参数和错误；workspace1090、lint、G1双轮9条拒绝PASS，完整BIOS68/72（默认/V1各首轮及W7双轮30秒超时）；不扩大至75/76或完整G2/G3。

2026-10-31：[Ready 挂接准入整改](2026-10-31-stage6-gate-admission.md)，拒绝待领取原生返回及恢复续体，24 个 host 拒绝组合与 ring3 注入反例双轮通过；workspace1089、lint PASS，完整BIOS70/72（W7双轮30秒超时）；完整 G2/G3、代理复制与失败尝试审计仍待完成。

2026-10-31：[trap 与四个控制入口及 G1](2026-10-31-stage6-gate-dispatch.md)，原生73/74/78/88、syscall/int80截获、只写rax、信号保留与wait输出事务；修复原生分派栈回归，最终BIOS71/72、W7一轮超时仍保留；75/76与完整G2/G3未完成。

2026-10-31：[最后服务授权丢失钩子](2026-10-31-stage6-gate-authority-loss.md)，drop/move/revoke、Namespace/IPC 转移共用总预算与孤儿化提交；请求根丢弃后的别名回收，host 证据，非 G1–G3。

2026-10-31：[S6-M1/M1b 内核门控对象与请求生命周期](2026-10-31-stage6-gate-lifecycle.md)，attach 预留、trap 入队/容量等待、出队交付、一次性答复、客户端销毁与服务者孤儿化在真实内核状态表上落地并有 10 组 host 反例；未分配 73–78、未接管 trap 入口、无 QEMU G1–G3。

主分支整合：[按需输入、私有容量与冻结台账](2026-10-31-stage6-main-merge.md)，明确长度窗口冲突的取舍与整合验证范围。

2026-10-31：[S6-M2 大映像帧容量与预算](2026-10-31-stage6-exec-frame-capacity.md)，私有exec≤640页、526页合成文件与失败回收验证；不代表冻结busybox或LibOS运行。

2026-10-31：[S6-M2 ELF 输入流式读取、源页故障与退栈回收](2026-10-31-stage6-streamed-elf-input.md)。范围限原生 exec 输入与有界 W7 见证；私有149页上限由容量后继推进，Gate/LibOS 仍未完成。

阶段6于2026-11-04满足有界技术退出条件4/4；交付5/5完成。历史切片的未完成结论按当时范围保留，当前状态只查阶段跟踪。
本节收录阶段 6 的实现证据与准备性证据。

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-11-01 | [2026-11-01-stage6-gate-copy-lifecycle.md](2026-11-01-stage6-gate-copy-lifecycle.md) | 75/76代理复制、队列清理与G2/G3验收 |
| 2026-11-01 | [2026-11-01-stage6-w7-timing-diagnosis.md](2026-11-01-stage6-w7-timing-diagnosis.md) | W7总预算分段、BIOS输入成本及临时strip反例 |
| 2026-10-31 | [2026-10-31-stage6-gate-control-rejections.md](2026-10-31-stage6-gate-control-rejections.md) | S6-M1 原生控制拒绝与容量极限审计 |
| 2026-10-31 | [2026-10-31-stage6-gate-admission.md](2026-10-31-stage6-gate-admission.md) | S6-M1 Ready 目标未决返回与恢复续体准入整改 |
| 2026-10-31 | [2026-10-31-stage6-gate-dispatch.md](2026-10-31-stage6-gate-dispatch.md) | S6-M1/M1b trap、四个控制入口与 G1 有界见证 |
| 2026-10-31 | [2026-10-31-stage6-gate-lifecycle.md](2026-10-31-stage6-gate-lifecycle.md) | S6-M1/M1b 内核门控对象与请求生命周期：预留/队列/交付/答复/清理事务、`AuditFull` 停止与 10 组 host 反例；未分配 ABI、未接 trap、非 G1–G3 |
| 2026-10-30 | [2026-10-30-stage6-freeze-ledgers.md](2026-10-30-stage6-freeze-ledgers.md) | S6-M2 机械项：D17 调用号/权限位序冻结台账、单一 `auxv` 键源、映像长度窗口；更正"整块 copy-in"旧判断，非 Gate 接线 |
| 2026-10-30 | [2026-10-30-stage6-audit-stop-qemu.md](2026-10-30-stage6-audit-stop-qemu.md) | 审计停止裸机验证：七个执行边界的专用 QEMU 注入与停止调用变异拒绝；非 G1–G3 |
| 2026-10-30 | [2026-10-30-stage6-audit-fail-stop.md](2026-10-30-stage6-audit-fail-stop.md) | M1b 审计停止控制器：首次诊断、不可恢复标记、分派/调度/IRQ 接线；非 G1–G3 |
| 2026-10-30 | [2026-10-30-stage6-private-completion-waits.md](2026-10-30-stage6-private-completion-waits.md) | M1b 私有等待基础：真实对象登记、不可警报、代次完成、销毁与独立预算；Gate 尚未接入，非 G1/G2 |
| 2026-10-30 | [2026-10-30-stage6-user-copy-transactions.md](2026-10-30-stage6-user-copy-transactions.md) | M1b/M1c 内存前置：真实页表全范围输出预检、COW 预留回滚；原生输出/IPC/wait 接入，W9 跨页 COW，非 Gate G3 |
| 2026-10-29 | [2026-10-29-stage6-m1-blocker-closure.md](2026-10-29-stage6-m1-blocker-closure.md) | R1–R6 协议收口、候选 A 技术设计评审、u64 权限及 8 组 host 反例；D14 ring3 人格桩 QEMU 双轮，非 Gate 接线/阶段退出 |
| 2026-10-29 | [2026-10-29-stage6-m1-decision-review.md](2026-10-29-stage6-m1-decision-review.md) | S6-M1 六项决定复审：保留路线，R1–R6 需修订；静态预审，非架构通过 |
| 2026-10-29 | [2026-10-29-stage6-start.md](2026-10-29-stage6-start.md) | 阶段 6 启动与首个 S6-M2 增量：确定性 `AT_RANDOM` |
| 2026-10-28 | [2026-10-28-stage6-loader-slice.md](2026-10-28-stage6-loader-slice.md) | 阶段 6 装载一致性切片（启动前）：静态 ELF 装载与初始 auxv，含未完成清单 |

## 阶段 4

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-native-runtime-service-report.md](2026-10-24-native-runtime-service-report.md) | 原生运行时与服务重构验证 |
| 2026-10-17 | [2026-10-17-stage4-manifest-index-report.md](2026-10-17-stage4-manifest-index-report.md) | SPEC-132：从 manifest 顺序确定 init 服务索引 |
| 2026-10-17 | [2026-10-17-stage4-init-dynamic-handles-report.md](2026-10-17-stage4-init-dynamic-handles-report.md) | SPEC-131：init 自检读取动态 Capability 句柄 |
| 2026-10-17 | [2026-10-17-stage4-exit-review-spec149.md](2026-10-17-stage4-exit-review-spec149.md) | SPEC-149：阶段 4 退出条件复核与签署 |
| 2026-10-17 | [2026-10-17-exo3m-kernel-service-witness-retirement-report.md](2026-10-17-exo3m-kernel-service-witness-retirement-report.md) | SPEC-138 / EXO-3M：内核服务 witness 组装退役 |
| 2026-10-17 | [2026-10-17-exo3l-shared-trace-only-report.md](2026-10-17-exo3l-shared-trace-only-report.md) | SPEC-136 / EXO-3L：共享 trace 成为唯一记录通道 |
| 2026-10-17 | [2026-10-17-exo3k-shared-trace-bytes-report.md](2026-10-17-exo3k-shared-trace-bytes-report.md) | SPEC-135 / EXO-3K：共享页原始 trace 字节流 |
| 2026-10-17 | [2026-10-17-exo3j-raw-trace-validation-report.md](2026-10-17-exo3j-raw-trace-validation-report.md) | SPEC-133 / EXO-3J：原始 trace 进入 QEMU 验收 |
| 2026-10-16 | [2026-10-16-stage4-native-shared-segment-app-report.md](2026-10-16-stage4-native-shared-segment-app-report.md) | SPEC-129：原生应用的消息控制与共享 Segment 数据路径 |
| 2026-10-16 | [2026-10-16-stage4-closure-report.md](2026-10-16-stage4-closure-report.md) | SPEC-130：阶段 4 原生应用、最小权限与故障隔离收口 |
| 2026-10-16 | [2026-10-16-exo3i-raw-trace-debug-channel-report.md](2026-10-16-exo3i-raw-trace-debug-channel-report.md) | SPEC-128 / EXO-3I：原始 trace 调试通道 |
| 2026-10-16 | [2026-10-16-exo3h-servicemgr-trace-report.md](2026-10-16-exo3h-servicemgr-trace-report.md) | SPEC-127 / EXO-3H：`servicemgr_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3g-service-selected-trace-report.md](2026-10-16-exo3g-service-selected-trace-report.md) | SPEC-126 / EXO-3G：`service_selected_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3f-service-negotiated-trace-report.md](2026-10-16-exo3f-service-negotiated-trace-report.md) | SPEC-125 / EXO-3F：`service_negotiated_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3e-service-reply-trace-report.md](2026-10-16-exo3e-service-reply-trace-report.md) | SPEC-124 / EXO-3E：`service_reply_ready` 迁移到通用 trace |
| 2026-10-16 | [2026-10-16-exo3d-generic-trace-runtime-hello-report.md](2026-10-16-exo3d-generic-trace-runtime-hello-report.md) | SPEC-123 / EXO-3D：通用 trace 通道 + `runtime_hello_ready` 迁移 |
| 2026-10-16 | [2026-10-16-exo3c-protocol-tag-decoupling-report.md](2026-10-16-exo3c-protocol-tag-decoupling-report.md) | SPEC-122 / EXO-3C：协议 tag 不再进入内核 witness |
| 2026-10-16 | [2026-10-16-exo2d-manifest-boot-abi-report.md](2026-10-16-exo2d-manifest-boot-abi-report.md) | SPEC-121 / EXO-2D：通用 entry ABI + 用户态 manifest |
| 2026-10-16 | [2026-10-16-exo2c-init-loader-report.md](2026-10-16-exo2c-init-loader-report.md) | SPEC-120 / EXO-2C：init 机器码迁移到 ramdisk 的 ring3 opaque ELF |
| 2026-10-15 | [2026-10-15-stage4-user-driver-ioport-report.md](2026-10-15-stage4-user-driver-ioport-report.md) | SPEC-114：第一个 ring3 用户态驱动（`prima-driver`，S4-D4 第二步） |
| 2026-10-15 | [2026-10-15-stage4-service-storage-segment-report.md](2026-10-15-stage4-service-storage-segment-report.md) | SPEC-112：日志/存储服务持内核 MemorySegment 落数据（S4-D3 收口） |
| 2026-10-15 | [2026-10-15-stage4-device-ioport-capability-report.md](2026-10-15-stage4-device-ioport-capability-report.md) | SPEC-113：设备 `IOPortRange` 对象与受控 I/O 端口路径（S4-D4 内核侧） |
| 2026-10-15 | [2026-10-15-exo5-mechanism-baseline-review.md](2026-10-15-exo5-mechanism-baseline-review.md) | DOC-008 / EXO-5：机制基线再评估（审查记录） |
| 2026-10-15 | [2026-10-15-exo3b-witness-expectations-report.md](2026-10-15-exo3b-witness-expectations-report.md) | SPEC-119 / EXO-3B：witness 期望值移出内核 |
| 2026-10-15 | [2026-10-15-exo3a-kernel-name-decoupling-report.md](2026-10-15-exo3a-kernel-name-decoupling-report.md) | SPEC-118 / EXO-3A：内核不再解释服务名 |
| 2026-10-15 | [2026-10-15-exo2b-exo4a-report.md](2026-10-15-exo2b-exo4a-report.md) | SPEC-117 / EXO-2B + EXO-4A：共享映像格式与设备可见撤销 |
| 2026-10-15 | [2026-10-15-exo2a-name-free-service-directory-report.md](2026-10-15-exo2a-name-free-service-directory-report.md) | SPEC-116 / EXO-2A：无服务名的映像目录 |
| 2026-10-15 | [2026-10-15-exo1-boot-image-report.md](2026-10-15-exo1-boot-image-report.md) | SPEC-115 / EXO-1：通用 boot image（ramdisk）装载 |
| 2026-10-14 | [2026-10-14-stage4-three-base-services-report.md](2026-10-14-stage4-three-base-services-report.md) | SPEC-106 报告：S4-D3 三个基础服务（日志 / Namespace / 基础存储）接入 ring3 服务表 |
| 2026-10-14 | [2026-10-14-stage4-stateful-service-protocol-report.md](2026-10-14-stage4-stateful-service-protocol-report.md) | SPEC-110：有状态服务请求协议（三服务语义下沉） |
| 2026-10-14 | [2026-10-14-stage4-stack-segment-creation-sink-report.md](2026-10-14-stage4-stack-segment-creation-sink-report.md) | SPEC-108 报告：服务**栈段创建**下沉到 ring3 服务管理器 |
| 2026-10-14 | [2026-10-14-stage4-servicemgr-migration-body-report.md](2026-10-14-stage4-servicemgr-migration-body-report.md) | SPEC-104（主体）报告：注册表/按名解析/路由转发迁入 ring3 servicemgr |
| 2026-10-14 | [2026-10-14-stage4-servicemgr-endpoint-creation-report.md](2026-10-14-stage4-servicemgr-endpoint-creation-report.md) | SPEC-105 报告：ring3 服务管理器**表驱动创建端点** |
| 2026-10-14 | [2026-10-14-stage4-service-image-segment-report.md](2026-10-14-stage4-service-image-segment-report.md) | SPEC-109：单一服务映像段（代码段移交下沉到 ring3 服务管理器） |
| 2026-10-14 | [2026-10-14-stage4-ring3-servicemgr-task-report.md](2026-10-14-stage4-ring3-servicemgr-task-report.md) | SPEC-103 报告：ring3 服务管理器任务的引入（迁移第一片） |
| 2026-10-14 | [2026-10-14-stage4-peer-id-entry-report.md](2026-10-14-stage4-peer-id-entry-report.md) | SPEC-104（前置片）报告：对端 task id 的入口 ABI |
| 2026-10-14 | [2026-10-14-stage4-namespace-service-proxy-report.md](2026-10-14-stage4-namespace-service-proxy-report.md) | SPEC-111：Namespace 服务代理内核解析（S4-D3 收口） |
| 2026-10-14 | [2026-10-14-stage4-destroy-stack-snapshot-report.md](2026-10-14-stage4-destroy-stack-snapshot-report.md) | SPEC-107 报告：销毁路径 vspace 栈快照治本（滚动拆除取代栈上 VA 快照） |
| 2026-10-13 | [2026-10-13-stage4-string-service-names-report.md](2026-10-13-stage4-string-service-names-report.md) | SPEC-101 报告：字符串服务名与端点名的运行期指派（闭合 SPEC-095 §5.1） |
| 2026-10-12 | [2026-10-12-stage4-runtime-service-registry-report.md](2026-10-12-stage4-runtime-service-registry-report.md) | SPEC-095 报告：运行期服务注册表（闭合 SPEC-093 §5.2） |
| 2026-10-12 | [2026-10-12-stage4-cap-slot-separation-report.md](2026-10-12-stage4-cap-slot-separation-report.md) | Capability 槽位区间分离（`ISSUE-CAP-002`） |
| 2026-10-12 | [2026-10-12-stage4-alpha-independent-stack-report.md](2026-10-12-stage4-alpha-independent-stack-report.md) | SPEC-094 报告：`ALPHA` 端点独立栈段（闭合 SPEC-093 §5.1） |
| 2026-10-11 | [2026-10-11-stage4-vspace-arena-report.md](2026-10-11-stage4-vspace-arena-report.md) | SPEC-091 报告：逻辑 vspace 存储移出 `BootstrapState`（arena 化） |
| 2026-10-11 | [2026-10-11-stage4-service-manager-report.md](2026-10-11-stage4-service-manager-report.md) | SPEC-090 报告：服务管理器（init 按名解析并路由，阶段 4 S4-D2 前置） |
| 2026-10-11 | [2026-10-11-stage4-multi-endpoint-service-report.md](2026-10-11-stage4-multi-endpoint-service-report.md) | SPEC-092 报告：独立第二服务端点（init 按名路由到不同任务） |
| 2026-10-11 | [2026-10-11-stage4-malformed-handle-bounds-report.md](2026-10-11-stage4-malformed-handle-bounds-report.md) | SPEC-089 报告：畸形句柄越界（阶段 4 缺陷修复，`ISSUE-CAP-001`） |
| 2026-10-11 | [2026-10-11-stage4-independent-service-images-report.md](2026-10-11-stage4-independent-service-images-report.md) | SPEC-093 报告：独立第二服务端点（独立二进制映像 + 独立物理栈段） |
| 2026-10-10 | [2026-10-10-stage4-service-negotiation-report.md](2026-10-10-stage4-service-negotiation-report.md) | SPEC-088 报告：服务端解析与协商（阶段 4 S4-D2 前置） |
| 2026-10-10 | [2026-10-10-stage4-ring3-runtime-hello-report.md](2026-10-10-stage4-ring3-runtime-hello-report.md) | SPEC-085 报告：独立 ring3 原生客户端与 IPC Hello 握手 |
| 2026-10-10 | [2026-10-10-stage4-mm-vspace-fixes-report.md](2026-10-10-stage4-mm-vspace-fixes-report.md) | SPEC-086 报告：阶段 4 期内的 mm/vspace 正确性与性能修复批次 |
| 2026-10-10 | [2026-10-10-stage4-init-client-handshake-report.md](2026-10-10-stage4-init-client-handshake-report.md) | SPEC-087 报告：init ↔ 存活 ring3 客户端双向握手 |
| 2026-09-25 | [2026-09-25-stage4-runtime-api-prototype-report.md](2026-09-25-stage4-runtime-api-prototype-report.md) | SPEC-084 报告：阶段 4 原生运行时 API 初始实现 |

## 阶段 3

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-18 | [2026-10-18-stage3-review-remediation.md](2026-10-18-stage3-review-remediation.md) | 阶段 3 实现复核与整改（2026-10-18） |
| 2026-10-17 | [2026-10-17-stage3-exit-review-spec148.md](2026-10-17-stage3-exit-review-spec148.md) | SPEC-148：阶段 3 退出条件复核与签署 |
| 2026-10-10 | [2026-10-10-stage3-audit-trace-restore-report.md](2026-10-10-stage3-audit-trace-restore-report.md) | SPEC-083 实现恢复报告：`audit(cap)` 在 master 上落地（§13.2） |
| 2026-09-25 | [2026-09-25-stage3-timer-report.md](2026-09-25-stage3-timer-report.md) | 切片 AK 报告：阶段 3 第二片 —— Timer（§10 全量） |
| 2026-09-25 | [2026-09-25-stage3-namespace-report.md](2026-09-25-stage3-namespace-report.md) | 切片 AJ 报告：阶段 3 第一片 —— Namespace 路径操作（§11 全量） |
| 2026-09-25 | [2026-09-25-stage3-audit-trace-report.md](2026-09-25-stage3-audit-trace-report.md) | 切片 AL 报告：阶段 3 第三片 —— `audit(cap)` 审计查询（§13.2） |

## 阶段 2 / Framekernel

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-kernel-handle-model-report.md](2026-10-24-kernel-handle-model-report.md) | 内核句柄、权限与 Lean 模型补强 |
| 2026-10-18 | [2026-10-18-stage2-ipc-review-remediation.md](2026-10-18-stage2-ipc-review-remediation.md) | 阶段 2 实现复核与 IPC 整改 |
| 2026-10-17 | [2026-10-17-stage2-exit-review-spec147.md](2026-10-17-stage2-exit-review-spec147.md) | SPEC-147：阶段 2 退出条件复核与签署 |
| 2026-09-25 | [2026-09-25-framekernel-skeleton-report.md](2026-09-25-framekernel-skeleton-report.md) | 切片 AE 报告：framekernel 分层与 unsafe 收敛（SPEC-076） |
| 2026-09-25 | [2026-09-25-framekernel-mm-pod-report.md](2026-09-25-framekernel-mm-pod-report.md) | 切片 AF 报告：物理内存与页表 POD 收敛（SPEC-077） |
| 2026-09-25 | [2026-09-25-framekernel-global-state-report.md](2026-09-25-framekernel-global-state-report.md) | 切片 AG 报告：内核全局状态 safe 化（SPEC-078） |
| 2026-09-25 | [2026-09-25-framekernel-entry-inversion-report.md](2026-09-25-framekernel-entry-inversion-report.md) | 切片 AH 报告：trap/syscall 入口回调反转 + IDT/GDT 下沉（SPEC-079） |
| 2026-09-25 | [2026-09-25-framekernel-closure-report.md](2026-09-25-framekernel-closure-report.md) | 切片 AI 报告：收口与 TCB 量化（SPEC-080） |
| 2026-09-24 | [2026-09-24-stage2-self-built-cr3-report.md](2026-09-24-stage2-self-built-cr3-report.md) | 切片 X 报告：自建内核 CR3 与镜像逐页映射（ISSUE-AUTH-001） |
| 2026-09-24 | [2026-09-24-stage2-scheduler-state-audit-report.md](2026-09-24-stage2-scheduler-state-audit-report.md) | 切片 Z 报告：调度路径的 `TaskStateChanged` 与审计容量（ISSUE-TASK-003） |
| 2026-09-24 | [2026-09-24-stage2-reclaim-predicate-tightening-report.md](2026-09-24-stage2-reclaim-predicate-tightening-report.md) | 切片 AC 报告：内核回收判据收紧（SPEC-074） |
| 2026-09-24 | [2026-09-24-stage2-kernel-reclaim-report.md](2026-09-24-stage2-kernel-reclaim-report.md) | 切片 AA 报告：§8.2 内核回收路径（`KernelActor`，无 `task_cap`） |
| 2026-09-24 | [2026-09-24-stage2-faulted-state-machine-report.md](2026-09-24-stage2-faulted-state-machine-report.md) | 切片 Y 报告：`TaskStatus::Faulted` 状态机与 §12.5 迁移入口（ISSUE-TASK-001） |
| 2026-09-24 | [2026-09-24-stage2-exc-resume-report.md](2026-09-24-stage2-exc-resume-report.md) | 切片 AB 报告：§12.10 `exc_resume` 用户化（`KernelActor` 代调） |
| 2026-09-24 | [2026-09-24-stage2-audit-storage-refactor-report.md](2026-09-24-stage2-audit-storage-refactor-report.md) | 切片 AD 报告：审计存储重构（SPEC-075） |
| 2026-09-23 | [2026-09-23-stage2-image-frame-vector-report.md](2026-09-23-stage2-image-frame-vector-report.md) | 切片 W 报告：image Segment 的真实逐帧向量（ISSUE-FRAME-001） |
| 2026-09-17 | [2026-09-17-stage2-task-self-destroy-report.md](2026-09-17-stage2-task-self-destroy-report.md) | 切片 V 报告：Task 自毁与延迟帧归还（ISSUE-TASK-002） |
| 2026-09-17 | [2026-09-17-stage2-task-lifecycle-report.md](2026-09-17-stage2-task-lifecycle-report.md) | 切片 T 报告：Task create/yield/destroy 用户原语与单核调度 |
| 2026-09-17 | [2026-09-17-stage2-platform-cap-handover-report.md](2026-09-17-stage2-platform-cap-handover-report.md) | 切片 U 报告：平台段 Capability 按属主移交（ISSUE-AUTH-002 剩余项） |
| 2026-09-16 | [2026-09-16-stage2-opening-report.md](2026-09-16-stage2-opening-report.md) | 切片 Q 报告：阶段 2 启动——句柄编码、Capability 生命周期、Segment 原语 |
| 2026-09-16 | [2026-09-16-stage2-ipc-user-surface-report.md](2026-09-16-stage2-ipc-user-surface-report.md) | 切片 S 报告：IPC 用户化与两 ring3 任务共享 Segment |
| 2026-09-16 | [2026-09-16-stage2-bringup-segment-migration-report.md](2026-09-16-stage2-bringup-segment-migration-report.md) | 切片 R 报告：bring-up 13 区域迁移为真实 Segment 对象 |
| 2026-08-16 | [2026-08-16-framekernel-gap-analysis.md](2026-08-16-framekernel-gap-analysis.md) | Prima OS × Framekernel 差距分析报告 |

## 阶段 1

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-18 | [2026-10-18-stage1-open-issues-spec151.md](2026-10-18-stage1-open-issues-spec151.md) | SPEC-151：阶段 1 遗留 Open ISSUE 整改（AUTH-003/004、AUDIT-007、CAP-004） |
| 2026-10-18 | [2026-10-18-stage1-4-implementation-review.md](2026-10-18-stage1-4-implementation-review.md) | 阶段 1–4 实现复核（阶段 5 输入） |
| 2026-10-17 | [2026-10-17-stage1-review-remediation-spec150.md](2026-10-17-stage1-review-remediation-spec150.md) | SPEC-150：阶段 1 补充复核与整改 |
| 2026-10-17 | [2026-10-17-stage1-exit-review-spec146.md](2026-10-17-stage1-exit-review-spec146.md) | SPEC-146：阶段 1 退出条件复核与签署 |
| 2026-09-16 | [2026-09-16-stage1-closure-report.md](2026-09-16-stage1-closure-report.md) | 切片 P 报告：阶段 1 退出条件——帧唯一归属与初始 Authority 物化 |

## 阶段 0 / 模型

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-17 | [2026-10-17-stage0-thirdparty-organizational-review-verdict.md](2026-10-17-stage0-thirdparty-organizational-review-verdict.md) | 阶段 0 第三方组织级复核结论（未通过） |
| 2026-10-17 | [2026-10-17-stage0-third-party-review-remediation.md](2026-10-17-stage0-third-party-review-remediation.md) | SPEC-137：阶段 0 范畴外复核发现项整改 |
| 2026-10-17 | [2026-10-17-stage0-review-remediation-spec143.md](2026-10-17-stage0-review-remediation-spec143.md) | SPEC-143：阶段 0 第三方组织级复核发现项整改（29 项） |
| 2026-10-17 | [2026-10-17-stage0-review-remediation-spec141.md](2026-10-17-stage0-review-remediation-spec141.md) | SPEC-141：阶段 0 第二轮组织级复核发现项整改（11 项） |
| 2026-10-17 | [2026-10-17-stage0-review-remediation-spec140.md](2026-10-17-stage0-review-remediation-spec140.md) | SPEC-140：阶段 0 对抗式复核发现项整改（12 项） |
| 2026-10-17 | [2026-10-17-stage0-re-review-spec144.md](2026-10-17-stage0-re-review-spec144.md) | SPEC-144：SPEC-143 复核残修（3 项） |
| 2026-10-17 | [2026-10-17-stage0-organizational-review-remediation.md](2026-10-17-stage0-organizational-review-remediation.md) | SPEC-139：组织级阶段 0 审查发现项整改（9 项） |
| 2026-10-17 | [2026-10-17-stage0-frozen-re-review-spec145.md](2026-10-17-stage0-frozen-re-review-spec145.md) | SPEC-145：阶段 0 冻结基线复核与签署 |
| 2026-10-17 | [2026-10-17-stage0-4-gap-audit.md](2026-10-17-stage0-4-gap-audit.md) | 阶段 0–4 缺口复核包 |
| 2026-10-17 | [2026-10-17-s0x2-coverage-increment-doc007.md](2026-10-17-s0x2-coverage-increment-doc007.md) | SPEC-134：S0-X2 逐操作覆盖报告（DOC-007 增量复核） |
| 2026-10-14 | [2026-10-14-cosir-dual-personality-gap-analysis.md](2026-10-14-cosir-dual-personality-gap-analysis.md) | COSIR/POSIR 双人格底座差距分析 |
| 2026-10-14 | [2026-10-14-cosir-dual-personality-adoption-review.md](2026-10-14-cosir-dual-personality-adoption-review.md) | COSIR/POSIR 双人格扩展候选采纳审核 |
| 2026-10-13 | [2026-10-13-stage0-bootstrap-taskcreated-report.md](2026-10-13-stage0-bootstrap-taskcreated-report.md) | 阶段 0 / 内核 Audit 修复：bootstrap init Task 的创建事件（ISSUE-EXC-010） |
| 2026-10-13 | [2026-10-13-lean-cdt-invariant-report.md](2026-10-13-lean-cdt-invariant-report.md) | 形式化推进（参考 seL4）：Lean 模型的能力派生树（CDT）不变量与保持性证明 |
| 2026-10-13 | [2026-10-13-lean-cdt-forest-report.md](2026-10-13-lean-cdt-forest-report.md) | 形式化推进（参考 seL4）：Lean 证成能力派生树的森林性（无环） |
| 2026-10-13 | [2026-10-13-cosir-cdt-parent-report.md](2026-10-13-cosir-cdt-parent-report.md) | cosir 侧补能力派生树 `parent` 并建模 CDT（关 ISSUE-FORMAL-001） |
| 2026-10-12 | [2026-10-12-stage0-code-review.md](2026-10-12-stage0-code-review.md) | 阶段 0（语义基线）代码评审报告 |
| 2026-09-13 | [2026-09-13-spec058-lifecycle-unification.md](2026-09-13-spec058-lifecycle-unification.md) | SPEC-058 统一修订记录：对象生命周期 19 项 Open ISSUE |
| 2026-09-13 | [2026-09-13-s0x2-operation-coverage.md](2026-09-13-s0x2-operation-coverage.md) | S0-X2 逐操作覆盖报告（修订后复核） |
| 2026-09-13 | [2026-09-13-s0x1-independent-lifecycle-review.md](2026-09-13-s0x1-independent-lifecycle-review.md) | S0-X1 独立规格审查报告：对象生命周期 |
| 2026-09-13 | [2026-09-13-s0x1-fix-reverification.md](2026-09-13-s0x1-fix-reverification.md) | S0-X1 修复独立复核报告：SPEC-056 九项生命周期修复的对抗式再验证 |
| 2026-08-17 | [2026-08-17-capability-derivation-model-gap.md](2026-08-17-capability-derivation-model-gap.md) | Capability 派生 / 位置 / 溯源建模缺口备忘 |

## 跨阶段 / 双人格

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-14 | [2026-10-14-dual-personality-holder-model-h0-review.md](2026-10-14-dual-personality-holder-model-h0-review.md) | 双人格 Holder 模型 H0 评审报告 |

## 跨阶段 / 架构评估

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-17 | [2026-10-17-kernel-primitives-comparative-review.md](2026-10-17-kernel-primitives-comparative-review.md) | Prima OS 内核原语对照评估 |

## 跨阶段 / 兼容性

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-15 | [2026-09-15-linux-trace-compat-report.md](2026-09-15-linux-trace-compat-report.md) | 切片 P 报告：Linux trace 与有界差分（`cow-write-fault`） |

## 跨阶段 / 文档治理

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-08-13 | [2026-08-13-task-6-historical-artifact-consistency.md](2026-08-13-task-6-historical-artifact-consistency.md) | Task 6 Historical Artifact Consistency Report |

## 跨阶段 / 其他

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-13 | [2026-10-13-task-create-capability-audit-report.md](2026-10-13-task-create-capability-audit-report.md) | Task Capability 审计与派生链（ISSUE-EXC-011）+ 审计存储扩容 |
| 2026-10-13 | [2026-10-13-ipc-capability-move-audit-report.md](2026-10-13-ipc-capability-move-audit-report.md) | IPC 携带 Capability 的位置变化审计（ISSUE-EXC-012） |
| 2026-10-13 | [2026-10-13-exc-source-destroy-report.md](2026-10-13-exc-source-destroy-report.md) | 异常源销毁 `exc_source_destroy`（§12.6 补齐） |

- [POSIX std 后端：目标、ABI、标准入口及验证](2026-10-27-posix-std-backend-report.md)
