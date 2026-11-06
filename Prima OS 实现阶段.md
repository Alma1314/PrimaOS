---
id: project-status
kind: reference
authority: current-status
status: maintained
lifecycle: current
updated: 2026-11-06
---

# Prima OS 实现阶段

> 文档整理更新：2026-10-23。当前阶段结论依据已有报告；本次文档/Rust回归见[整理报告](docs/superpowers/reports/2026-10-23-documentation-restructure-report.md)，不改变组织级签署。
> 架构见[白皮书](Prima%20OS.md)，抽象语义见[原语规范](COSIR%20原语规范.md)。
> [文档中心](docs/README.md) · [主题地图](docs/architecture/topics/README.md) · [证据规则](docs/architecture/evidence.md)

## 维护规则

- 本文是当前进度、退出条件、活跃阻塞与证据指针的唯一汇总来源；计划记录当时步骤，报告保存当时结果。
- 阶段状态为未开始、进行中、阻塞或已满足退出条件；设计/技术验收不自动构成组织级签署。
- 勾选前必须有可访问工件及相应范围证据。host、QEMU、Lean、差分不能互相替代；失败与反例留在报告。
- 阶段定义先改白皮书，语义先改原语规范。文档整理不代替 ABI/profile 裁决。

## 当前快照

| 项目 | 当前结论 | 依据与范围 |
|---|---|---|
| 阶段 0–4 | 已满足各自退出条件 | [SPEC-145](docs/superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md)、[146](docs/superpowers/reports/2026-10-17-stage1-exit-review-spec146.md)、[147](docs/superpowers/reports/2026-10-17-stage2-exit-review-spec147.md)、[148](docs/superpowers/reports/2026-10-17-stage3-exit-review-spec148.md)、[149](docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md)；阶段0限规范文本，各实现/证明范围分别解释 |
| 当前阶段 5 | **已满足退出条件（2026-10-27，SPEC-152）**；有界 POSIX profile v2 冻结 | [签署报告](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md)、[冻结 v2](docs/superpowers/specs/2026-10-25-stage5-profile-v2.md)；D6 A 已选定；M20/M21 保留为签署前评审 |
| 兼容范围 | 逐项读取当前台账，不宣称完整 POSIX/Linux 兼容；v2 的 30 行全部 partial | [compat-state](docs/superpowers/compat/compat-state.md)、[symbol-trace](docs/superpowers/compat/symbol-trace.md)、[M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md) |
| 最近验证记录 | 2026-11-06 RV-1后继host1125/1126、lint及Linux/Gate特性Clippy、G1–G4/七审计停止边界/同ELF11项各双轮、单次完整BIOS72/72及文档检查通过；1300交付为host范围，D14/Lean未在后继重跑 | [RV-1后继](docs/superpowers/reports/2026-11-06-gate-request-audit-history.md)、[首轮整改](docs/superpowers/reports/2026-11-06-stage6-review-remediation.md)、[前序统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)；历史失败保留 |
| 阶段 6 | **已满足退出条件，技术复核4/4**；有界Linux profile v1与D12八成员运行冻结 | [统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)；不自动构成组织级签署，不启动阶段7 |
| 阶段 7 | **未开始；综合交付范围已由作者确认，技术规格、白名单与量化门槛待冻结**；7-0/7-A…7-H 九轨必交付，退出仍未勾选 | [2026-10-31 范围裁决](docs/superpowers/specs/2026-10-31-stage7-scope-decisions.md)承接[D16](docs/superpowers/specs/2026-10-28-stage6-7-release-decisions.md)；Linux + Prima 原生/POSIX、双路径动态链接、x86_64 2/4 vCPU、核心及I/O配额、完整撤销、受限透明恢复与设备持久化；AArch64仅单核机制验证；QEMU正式验收。D14人格桩不等于7-A退出，D15状态分区要求保留 |
| TCB/容量 | 以对应版本测量为准 | 当前值运行 `cargo xtask tcb`；内核零 unsafe 规则不变 |

## 证据索引

| 范围 | 入口 | 如何解释 |
|---|---|---|
| 抽象状态与研究目标 | [规范](COSIR%20原语规范.md)、[白皮书](Prima%20OS.md) | 预期语义与研究假设，不是实现证明 |
| 阶段0–4签署 | 上方 SPEC-145–149 | 以各自退出条件及证据范围为限 |
| 阶段5签署 | [SPEC-152](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md) | 有界 profile v2 冻结；D1–D10 范围确认；BLK-003 按阶段5范围关闭；不表示完整 POSIX/Linux 兼容 |
| 签署后整改 | [SPEC-150](docs/superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md)、[151](docs/superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)、[阶段2 IPC](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)、[阶段3](docs/superpowers/reports/2026-10-18-stage3-review-remediation.md)、[1–4复核](docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md) | 旧签署后的局部修订，不抹去旧报告 |
| 阶段6首轮复核整改（e37c46f） | [RV-1–RV-7审查](docs/superpowers/reports/2026-11-06-stage6-implementation-review.md)、[整改](docs/superpowers/reports/2026-11-06-stage6-review-remediation.md) | 当时登记≤1279交付边界和1024帧预算；修复审计容量/游标、合并D17表并加personality门禁；RV-1上限由下行后继解除，历史发现保留 |
| RV-1审计承载后继 | [契约](docs/superpowers/specs/2026-11-06-gate-request-audit-history.md)、[报告](docs/superpowers/reports/2026-11-06-gate-request-audit-history.md) | 请求根历史保存在既有交付审计，解除通用历史槽对交付的累计上限；通用预算/只追加历史/审计fail-stop保持；实际验证范围见报告 |
| OSTD 跨项目复用 | [契约](docs/superpowers/specs/2026-10-24-asterinas-reuse-design.md)、[验证](docs/superpowers/reports/2026-10-24-asterinas-reuse-report.md)、[来源与许可](docs/architecture/asterinas-provenance.md) | 对齐/临界区、[POD/缓冲](docs/superpowers/specs/2026-10-24-asterinas-pod-buffer-design.md)及[一次发布](docs/superpowers/specs/2026-10-24-asterinas-once-publication-design.md)后继；不等于完整框架 soundness 或 SMP |
| 原生运行时/服务 | [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md)、[握手/路径后继](docs/superpowers/reports/2026-10-24-native-runtime-service-report.md)、[主题包](docs/architecture/topics/runtime.md) | 原生应用/服务收口不等于 POSIX 完整实现 |
| 阶段5当前子集 | [M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md)、[M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)、下方里程碑表 | 分别记录已覆盖、限制与未签署条件 |
| 阶段5余项/阶段6入口 | [收尾路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md) | 计划；其中旧基线的“未实现”由后继报告限定，不当成当前摘要 |
| S6-M3/M4 进程与差分后继 | [统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md) | 87–89、pthread/进程/M3与原始审计双轮通过；完整72/72及有界出口复核通过 |
| 阶段6实现复核 | [独立复核](docs/superpowers/reports/2026-11-06-stage6-implementation-review.md) | 独立复现 host 1118/1119、lint、TCB、Linux同源差分与完整BIOS 72/72；2项中等、4项低风险与1项契约漂移；不改变阶段状态与勾选 |
| S6-M4 上下文/信号/线程 | [P1/P2与专用镜像构建](docs/superpowers/reports/2026-11-01-stage6-context-signals-threads.md) | 83–86、TaskContext、GPR/FXSAVE、Linux信号和共享线程/futex；仅编译/构建，未执行任何测试，非阶段退出 |
| S6-M2/M3 停车装载与LibOS | [受管理装载与真实制品镜像构建](docs/superpowers/reports/2026-11-01-stage6-managed-linux-loader.md) | 79–82、manifest v3、用户态profile草案、六applet差分工程；仅编译/构建，无运行证据，非阶段退出 |
| S6-M2 私有映像容量 | [526页、额度失败与再次exec](docs/superpowers/reports/2026-10-31-stage6-exec-frame-capacity.md) | 私有640页及相关全局预算，普通段仍149页；host/QEMU范围分开，非冻结busybox运行 |
| S6-M2 按需 ELF 输入 | [元数据逐条读取与失败退栈回收](docs/superpowers/reports/2026-10-31-stage6-streamed-elf-input.md) | 原生 exec 移除 1 MiB/首个 4 KiB 程序头限制；W7 稀疏输入仅需两页段数据，非 526 页 busybox 或 Linux 运行 |
| S6-M1 审计停止裸机验证 | [七个边界与停止调用变异](docs/superpowers/reports/2026-10-30-stage6-audit-stop-qemu.md) | 专用配置 14 轮 QEMU 验证耗尽后停止与保存状态；非 Gate/Request 接线或 G1–G3 |
| S6-M1 审计停止控制器 | [首次诊断与全局停止](docs/superpowers/reports/2026-10-30-stage6-audit-fail-stop.md) | 私有等待耗尽不重试，分派/调度/IRQ 停止；host 耗尽测试及既有回归，非 Gate G1–G3 |
| S6-M1 普通控制失败审计 | [原始尝试、身份及返回错误](docs/superpowers/reports/2026-10-31-stage6-gate-control-rejections.md) | 四个入口在参数解码前预检，拒绝只追加尝试事件；host容量极限及G1范围见报告，非75/76或完整G2/G3 |
| S6-M1 Ready 挂接准入 | [未决返回与恢复续体](docs/superpowers/reports/2026-10-31-stage6-gate-admission.md) | 共用准入拒绝旧 IPC/等待/恢复/重启/信号续体；24 个 host 拒绝组合及 ring3 注入反例，非完整 G2/G3 |
| S6-M1 trap 与 G1 | [四个控制入口与两种硬件陷入](docs/superpowers/reports/2026-10-31-stage6-gate-dispatch.md) | 73/74/77/78、原生号先截获、只写rax、信号保留、wait重试与输出失败；75/76及完整G2/G3未完成 |
| S6-M1 门控对象与请求生命周期 | [预留、队列、交付、一次性答复与清理](docs/superpowers/reports/2026-10-31-stage6-gate-lifecycle.md) | 真实内核表上的 attach/trap/wait/reply/abort、FIFO 晋级、客户端销毁与服务者孤儿化；host 反例，未分配 ABI、未接 trap、非 G1–G3 |
| S6-M1 私有等待前置 | [真实登记与不可警报完成](docs/superpowers/reports/2026-10-30-stage6-private-completion-waits.md) | Task cookie/每次阻塞代次、销毁清理、公共/私有预算隔离；host 内核表测试，Gate 尚未接入，非 G1/G2 |
| S6-M1 内存事务前置 | [真实页表预检与 COW 回滚](docs/superpowers/reports/2026-10-30-stage6-user-copy-transactions.md) | 原生输出/IPC/wait 共用全范围预检；真实帧归属耗尽回滚与 W9 跨页 COW；非 Gate 接线/G3 |
| S6-M1 当前评审阻塞收口 | [R1–R6 整改与 D14 原型](docs/superpowers/reports/2026-10-29-stage6-m1-blocker-closure.md) | M1a 协议与候选 A 技术设计评审通过；u64 权限、host 模型、7-A ring3 人格桩 QEMU 双轮；未接线 Gate，非阶段退出 |
| S6-M1 六项决定复审 | [R1–R6 静态预审](docs/superpowers/reports/2026-10-29-stage6-m1-decision-review.md) | 当时六条路线保留，取消/复制/权限/容量协议需修订；静态报告无运行证据；当前收口见上行 |
| 阶段6–7前置裁决 | [D11–D16](docs/superpowers/specs/2026-10-28-stage6-7-release-decisions.md) | 范围裁决：白名单/`AT_RANDOM`/多 LibOS 顺序/SMP 前置/阶段 7 子轨；不是实现证据，不签署阶段，不启动阶段 6 |
| 阶段6准备性证据 | [装载一致性切片](docs/superpowers/reports/2026-10-28-stage6-loader-slice.md) | host 单测 + 本地静态 busybox 装载演练 + lint/全套件；只证明装载计划与初始栈，不证明任何 Linux 二进制可运行，阶段状态不变 |
| 阶段6机械项与冻结台账 | [冻结台账与装载机械项](docs/superpowers/reports/2026-10-30-stage6-freeze-ledgers.md) | D17 调用号/权限位序冻结、`auxv` 单一键源、当时映像字节窗口、白名单 526 页复核；输入/容量由后继推进；host + lint + BIOS 套件，非 Gate 接线，不勾选退出条件 |
| 历史切片与反例 | [全量目录](docs/architecture/catalog.md)、[主题阅读链](docs/architecture/topics/README.md) | 文件不因未进入精选表而失效；必须保留失败与后继 |

2026-10-18 压缩前记录保存在[历史版本](https://github.com/Alma1314/PrimaOS/blob/a434a2f0c515ac3421e999be3c436b7d7c6c5313/Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md)。本次重构前版本可用 `git show d48d5ef:Prima\ OS\ 实现阶段.md` 查看；[整理报告](docs/superpowers/reports/2026-10-23-documentation-restructure-report.md)记录陈旧摘要的修正依据。

VFS 后继实现与验证见[LibOS 基础重构报告](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)：原生打开对象、普通文件延迟回收和数据边界修复；不改变阶段出口勾选。

musl 后继实现见[调用边界与 fd/stdio 报告](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)：fcntl、fdopen/已有 append、向量 I/O、汇编边界与锁定构建；不改变阶段出口勾选。

内核句柄与 Lean 后继补强见[验证报告](docs/superpowers/reports/2026-10-24-kernel-handle-model-report.md)：generation 耗尽永久退役、权限未知位拒绝、分配失败不推进逻辑 CapId，以及单槽任意后续迁移中的旧句柄不复活证明；证据分层及未覆盖项以报告/Lean 映照为准，不改变阶段签署。

2026-10-31 后继见[有限槽表组合证明](docs/superpowers/reports/2026-10-31-kernel-handle-table-proof.md)：逐槽投影、跨槽隔离、整表良构及任意后续序列旧句柄不复活；10 条一般 Lean 定理与真实 Rust 有界遍历，不构成 Rust 源码 refinement proof，阶段勾选不变。

同日[源码细化后继](docs/superpowers/reports/2026-10-31-rust-handle-source-refinement.md)直接提取生产 Handle::new/next_generation，完成 5 条条件性源码细化定理及 Rust 语义 mutation 拒绝；限构造器和 generation 更新核心，不等于整张 Rust 槽表或完整内核证明。 与主分支阶段6冻结台账的整合验证见同报告末节。

原生运行时与服务后继见[握手/路径重构报告](docs/superpowers/reports/2026-10-24-native-runtime-service-report.md)：Hello 一致性验证、顺序分片、双路径事务重置与单次消费；仍限单客户端/单在途，未改变阶段签署。

Asterinas OSTD 复用后继见[移植报告](docs/superpowers/reports/2026-10-24-asterinas-reuse-report.md)：
对齐工具、StaticCell 原子互斥/IRQ guard、裸入口布局与不返回调度边界；不引入完整上游内核或修改阶段签署。

Asterinas 参考续篇见[POD 与恢复帧报告](docs/superpowers/reports/2026-10-24-asterinas-pod-buffer-report.md)：
字节视图的类型审计、TSS 下沉与恢复帧的溢出/借用/锁边界。验证范围以该报告为准，不改变阶段勾选或前轮超时结论。

Asterinas 一次发布后继见[报告](docs/superpowers/reports/2026-10-24-asterinas-once-publication-report.md)：Once错误/unwind重试、所有权析构及根/镜像描述整体发布；完整验收超时仍未收口，未改变阶段签署。

fork 续执行与进程边界后继见[报告](docs/superpowers/reports/2026-10-24-fork-continuation-report.md)：
COW写保护后同步TLB、分配失败回收、musl子退出与四字节跨页/COW等待输出；没有修改阶段签署。

本批增量见[2026-10-25 缺口补强记录](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)：启动栈空间、syscall保护页、托管退出交接、fork FPU继承及Rust最小工作目标；本次验证范围按报告解释。
[profile v2](docs/superpowers/specs/2026-10-25-stage5-profile-v2.md)保留提案；D6用户选定的原生单线程std A已验证，整体阶段签署仍待最终评审。

本次后继代码及待办见[阶段0–5工作记录](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)：
本地能力位置/事件组合、Task栈guard、独立fork能力视图、认证VFS/OFD/socket holder、exec新crt、信号/线程/IRQ及D6 A均已取得本轮有界证据。
遵照用户要求先完成实现再统一测试；最终完整68轮通过，历史失败保留。下表按实际范围更新；组织级退出勾选已由 [SPEC-152](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md)（2026-10-27）签署。

POSIX std后继（2026-10-27）见[工作契约](docs/superpowers/specs/2026-10-27-posix-std-backend-design.md)与
[本次报告](docs/superpowers/reports/2026-10-27-posix-std-backend-report.md)：独立Prima Unix/musl目标与完整std模块接入；
原A子集和组织级签署边界保留，具体支持范围以R3及台账为准。
[文件I/O与接收归账续做](docs/superpowers/reports/2026-10-27-posix-std-file-io-report.md)已通过单次完整36组72/72轮、workspace1020、lint、三种Rust模式Clippy与compat；V4扩展和I1新归账按契约范围解释。

阶段 1～5 后继复核见[2026-10-31 整改报告](docs/superpowers/reports/2026-10-31-stage1-5-regression-remediation.md)：VFS 空文件内容别名、musl 检查假 PASS 与干净工作树聚焦验收修复；验证结果与限制以报告为准，阶段签署范围不变。

## 已知问题与当前阻塞

2026-11-06复核整改：[RV-1–RV-7首轮](docs/superpowers/reports/2026-11-06-stage6-review-remediation.md)先登记
旧1279次交付上限；作者继续修复后的[审计承载后继](docs/superpowers/reports/2026-11-06-gate-request-audit-history.md)
将请求根完整历史存入原交付事件，解除通用历史槽对交付的累计限制。
通用1280项和审计16384条预算仍保持，审计满全局fail-stop；不构成长时运行保证，阶段7仍未启动。

2026-11-01 装载后继：[RAM复制、四段/BSS与八页栈](docs/superpowers/reports/2026-11-01-stage6-loader-geometry.md)。前序W7时限失败保留，本轮最终结果不外推为一般性能保证。

| ID / 事项 | 当前口径与限制 | 证据 / 下一步 |
|---|---|---|
| S6-M1 R1–R6 / D14（当前评审范围） | **有界技术范围收口**：G1–G4、七审计边界与D14原探针双轮通过；S6-D2整体复核完成 | [统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)；历史协议/失败及TCG/单CPU限制保留 |
| S6-M2 / 原生W7时限 | **本轮时限验收通过，历史超时因果未闭合**：前序完整60/72等失败及隔离W7双轮30秒超时保留；本轮隔离W7和首次完整回归中的W7均双轮PASS；首次完整71/72仅I1统计不一致，最终源码完整72/72 PASS。初次诊断21.473秒退出33，其中14.542秒才到内核入口；不能将总时限直接归因于exec复制，也不推断全部失败的唯一根因 | [分段采样与strip失败反例](docs/superpowers/reports/2026-11-01-stage6-w7-timing-diagnosis.md)、[本轮统一结果](docs/superpowers/reports/2026-11-01-stage6-gate-copy-lifecycle.md)、[前序失败](docs/superpowers/reports/2026-11-01-stage6-loader-geometry.md)；不放宽门限，不用单次诊断覆盖完整回归 |
| 历史普通启动/V1时限 | **历史因果未解释，当前验收已通过**：2026-11-04完整72/72含默认/V1双轮PASS；以下旧失败保留，非当前验收阻塞。普通失败审计后继整套68/72，默认启动和V1各第1轮30秒超时、各第2轮通过；分别止于cow_ready/authority_ready，未定位原因，不以另一轮通过覆盖失败 | [原始日志与限制](docs/superpowers/reports/2026-10-31-stage6-gate-control-rejections.md)；保持门限和验收输入 |
| S6-M2 装载资源 | **冻结制品范围收口**：640页预算、实际PT_LOAD/BSS/8页栈和D12双轮运行；任意VFS执行不在范围 | [统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)；不外推任意应用 |
| S6-M1 M1b–M1d 接线 | **有界技术范围收口**：G1–G4、七审计边界与D14原探针双轮通过；S6-D2整体复核完成 | [统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)；历史协议/失败及TCG/单CPU限制保留 |
| QEMU 启动超时记录 | 2026-10-23 VFS 多次超时；后继双轮通过。2026-10-24 原生服务首次 W9 无输出超时；OSTD首轮复用的 V1 停于 bootloader 物理映射、W1 完全无输出；当时仅分段覆盖62轮，完整命令未单次全绿。POD后继完整62轮通过，旧超时根因仍未定位；一次发布后继三次完整命令超时及多次后缀超时，最终成功覆盖60/62轮，保留该版本验证缺口；本轮fork后继分段覆盖62/62，完整命令在35轮后W8 run2超时，根因仍未定位；2026-10-25前批及2026-10-27本轮均分别取得单次68轮PASS；本轮另保留I1/N1计数失败和显式验收修订，见后继报告 | [一次发布超时](docs/superpowers/reports/2026-10-24-asterinas-once-publication-report.md)、[OSTD首轮 V1/W1](docs/superpowers/reports/2026-10-24-asterinas-reuse-report.md)、[W9](docs/superpowers/reports/2026-10-24-native-runtime-service-report.md)、[VFS 历史](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)、[musl 后继](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)；[本批完整验收](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)；保留门限/断言/失败证据，不以栈保护或退出整改自动解释全部历史超时 |
| I1接收归账验收（本轮发现跨轮偏差） | 前序有界技术收口及完整72轮PASS保留；本轮首次完整71/72，I1第2轮用户消息总数423→420变化，单轮各自归账成立、非UART出口不变。最终版本完整72/72，I1双轮PASS且消息总数均423；未放宽跨轮判据，首次差异根因仍未唯一确认 | [本轮原始数据与范围](docs/superpowers/reports/2026-11-01-stage6-gate-copy-lifecycle.md)、[历史归账修订](docs/superpowers/reports/2026-10-27-posix-std-file-io-report.md)；不据新失败改写旧签署，不据后继单次PASS倒推因果 |
| 系统调用栈保护边界 | syscall和全部Task中断栈下界guard通过host与本轮启动页表检查；32KiB可用容量、哨兵保留；exec准备缓冲在提交前退栈 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；stack_guard=1，不构成所有路径峰值证明 |
| BLK-003 | **已按阶段 5 范围关闭（2026-10-27，SPEC-152）**：原生号段 1–72 以 `prima-abi` 为源、52 保留；Prima 命名；错误码只登记实际实现；静态链接；线程共享能力视图；静态 persona/符号护栏。动态装载、Linux 号在用户态 LibOS 的完整映射、第二 personality 转入阶段 6/7 | [签署报告](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md)、[D1–D10评审工件](docs/superpowers/specs/2026-10-26-stage5-release-decisions.md) |
| S5-D1 / S5-D6 / S5-X3 | **已签署（SPEC-152）**：profile v2 冻结；D6 A 完成 R1/R2/R3 与 Clippy；S5-X3 限 sbase echo/cat/ls 冻结集 | [签署报告](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md)、[D6裁决](docs/superpowers/specs/2026-10-26-rust-d6-scope-decision.md)；阶段6于2026-10-29启动；当前状态见阶段6节 |
| fork 子任务 VFS 授权 | 认证会话/直接IPC/OFD holder、libc execve/envp/CLOEXEC、新C crt与连续exec本轮W6/W7/W7b双轮通过 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；有界静态映像及RAM VFS，不宣称完整execve |
| ISSUE-FORK-CAPS-001 | 用户选定A的独立视图、受控继承、认证服务交接/发布、OFD/socket holder在本轮host及W6/N1中通过；有界技术范围收口 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；多线程fork、每LibOS根视图不在范围；共享线程视图待最终语义确认 |
| ISSUE-PTHREAD-CAPS-001（工作决定） | 同 Process 线程共享 leader 能力空间，组织级语义确认仍待完成 | [M15设计](docs/superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)、[M15b-2a](docs/superpowers/reports/2026-10-23-stage5-m15b2a-thread-create.md) |
| 信号/线程未覆盖项 | mask/siginfo/EINTR/RESTART/SIGCHLD、硬件故障及取消/分离出口的W4/W9/I1/N1双轮通过 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；无altstack/实时排队/完整属性及任意异步取消保证；#AC仅host路由 |
| ISSUE-TTYIN-001（有界技术收口） | IRQ4驱动的输入、等待/取消/确认、EINTR/重启、handler嵌套回复本轮I1双轮通过；不再以轮询工作实现概括当前路径 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；无VTIME、流控或job control，IRQ时序验收边界见报告 |
| ISSUE-START-ARGS-001（有界子集收口） | 已定位256字节预留不足，改为按参数实际尺寸分配；A2独立版本化载体16/8项、长串、空值及UTF-8的Linux参考和QEMU双轮通过 | [本批后继](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)；不外推任意用户程序，无AT_RANDOM等边界保留 |
| ISSUE-CAP-001 | 本地Task/消息/Bound/Mount及Timer/异常/等待/远端组合的位置与溯源host回归通过；Lean位置保持性构建通过；有界模型范围收口 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)、[Lean映照](lean/README.md)；不是完整COSIR状态机或机器证明 |
| ISSUE-AUTH-001 | SPEC-146 接受的 image/vspace 细化分叉，不阻断 S1-X2 | 保留与§14完整细化的区别；新装载路径另按阶段5报告取证 |
| ISSUE-COW-001/002 | 抽象 COW 的映射/并发责任澄清，不是自动阻断项 | [SPEC-015](docs/superpowers/specs/2026-08-14-cow-refinement-design.md)；扩大语义时再验证 |

### 已收口问题的后继入口

- ISSUE-START-ARGS-001在M12有界参数载体范围技术收口：[A2后继](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)，保留M12原始失败，不外推任意用户程序或AT_RANDOM。

- ISSUE-FORK-CONTINUATION-001 在当前单核/单线程C/musl续执行范围技术收口：[fork后继](docs/superpowers/reports/2026-10-24-fork-continuation-report.md)，W6/W7/W7b/W8聚焦双轮通过，该历史版本完整命令仍超时；本轮后继已取得完整单次68/68，见[统一报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)。M17b-3/4的原生绕行和失败保留；cap/close隔离及VFS授权仍按各自问题跟踪。

- BLK-001 已关闭；Lean 证明范围见 lean/README。BLK-002 仅形成细化设计，不能外推为机器证明。
- BLK-004 容量门槛已关闭：[C ELF 见证](docs/superpowers/reports/2026-10-18-stage5-c-elf-qemu.md)。
- ISSUE-IPC-001 已由[阶段2整改](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)关闭。
- ISSUE-THREAD-CAPACITY-001 已由[M15b-2b](docs/superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md)关闭；保留 task_create 满容量负例。
- ISSUE-PTHREAD-CLONE-001 的 clone/join 阻塞由[M16b-3](docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)后继收口；M16b-1/2 的失败是历史记录。
- ISSUE-FORK-COW-001 的 COW 机制路径已由[M17b-4](docs/superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)取得 W6 见证；包装续执行与能力隔离问题继续单独跟踪，不将它们一并关闭。

## 决策日志

| 日期 | 决定与当前影响 | 来源 |
|---|---|---|
| 2026-11-04（技术退出） | 阶段6全部余项统一验收完成，冻结有界profile v1，D1–D5及X1–X4勾选；保留历史失败；不启动阶段7、不冒称组织级签署 | [统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md) |
| 2026-10-31（阶段7范围） | 作者逐项确认综合交付：Linux + Prima 原生/POSIX、双路径动态链接、x86_64 2/4 vCPU、核心及I/O配额、完整撤销、设备与持久化、冻结端到端协议内的用户态组件透明恢复；AArch64单核机制验证，QEMU正式验收；关键性质须证明，性能先基线后门槛。范围已确认，详细契约待冻结；不启动阶段7，不勾选退出条件 | [16项回答与范围裁决](docs/superpowers/specs/2026-10-31-stage7-scope-decisions.md) |
| 2026-10-23 | M21 技术出口评审未签署；阶段6 D10 不启动 | [M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md) |
| 2026-10-27（签署） | **阶段5签署（SPEC-152）**：冻结 profile v2；确认 D1–D10 处置；BLK-003 按阶段5范围关闭；阶段6具备入口但按 D10 不启动。范围限有界 partial profile，不代表完整 POSIX/Linux 兼容 | [签署报告](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md) |
| 2026-10-31（帧容量） | **S6-M2 有界推进**：私有exec149→640页，补齐全局预算及批量归属回收；526页内容/部分分配失败/四次回滚/大映像再次exec；不勾选阶段条件 | [契约](docs/superpowers/specs/2026-10-31-stage6-exec-frame-capacity.md)、[实测与失败](docs/superpowers/reports/2026-10-31-stage6-exec-frame-capacity.md) |
| 2026-11-01（复制与生命周期） | **S6-M1/M1c–M1d后继**：75/76、双端root/Segment/COW与复制审计，满队列销毁晋级修复；G1/G2/G2-death/G3及隔离W7各双轮PASS，workspace1095、lint及特性Clippy PASS；最终完整72/72，首次71/72的I1差异保留，不勾选阶段退出 | [报告](docs/superpowers/reports/2026-11-01-stage6-gate-copy-lifecycle.md)、[W7诊断](docs/superpowers/reports/2026-11-01-stage6-w7-timing-diagnosis.md) |
| 2026-10-31（普通失败审计） | **S6-M1 后继**：73/74/77/78共用入口在返回普通错误前记录身份、原始参数和结果，容量不足先停止；host容量极限、G1双轮9条拒绝、workspace1090与lint PASS；完整BIOS68/72，默认/V1各首轮及W7双轮超时保留；阶段退出仍0/4 | [报告](docs/superpowers/reports/2026-10-31-stage6-gate-control-rejections.md) |
| 2026-10-31（挂接准入） | **S6-M1 后继**：Ready 目标的旧 IPC/等待结果、异常恢复、进程状态写回、原生重启与信号续体在 Gate 预留前拒绝；24 个 host 组合、ring3 EAGAIN 与清除后挂接双轮通过；workspace1089、lint PASS，完整BIOS70/72（W7双轮超时）；不勾选阶段条件 | [报告](docs/superpowers/reports/2026-10-31-stage6-gate-admission.md) |
| 2026-10-31（trap接线） | **S6-M1/M1b 后继**：syscall/int80 截获、attach/wait/reply/abort 追加73/74/77/78、96字节输出预检与空队列重试、本地能力视图及信号保留；修复Gate快照引入的原生分派栈回归；workspace1086、lint、G1双轮通过，最终整套71/72（W7一轮超时）；G1和部分失败路径有界验证，完整G2/G3与代理复制待完成；不勾选阶段条件 | [报告](docs/superpowers/reports/2026-10-31-stage6-gate-dispatch.md) |
| 2026-10-31（授权丢失） | **S6-M1/M1b 对象层后继**：最后 Gate/请求服务别名的 drop/move/revoke、Namespace/IPC 转移触发总审计预检与孤儿化；根丢弃后的请求别名回收、空队列服务唤醒释放与容器退役身份取回；host 17 组（新增7组），workspace1083、lint PASS；无 ABI/trap/G1–G3 | [报告与限制](docs/superpowers/reports/2026-10-31-stage6-gate-authority-loss.md) |
| 2026-10-31（门控对象） | **S6-M1/M1b 对象层落地**：Gate/Binding/Request 表、attach 预留、trap 入队与容量等待、出队交付、reply/abort 一次性提交（只写 `rax`）、客户端销毁取消与服务者孤儿化；`GateAttach`/`GateServe` 追加位 31/32；10 组 host 反例，1076 passed、lint PASS；全量BIOS套件本轮因30秒门限未通过（环境争用，见报告）；未分配 73–78、未接 trap、无 QEMU G1–G3 | [报告](docs/superpowers/reports/2026-10-31-stage6-gate-lifecycle.md) |
| 2026-10-31（装载输入） | **S6-M2 有界推进**：原生 exec 逐条读取头部，移除 1 MiB 文件/首个 4 KiB 程序头限制；新增 W7 稀疏输入与源页故障，修复失败回收的内核栈 guard 越界；未勾选阶段条件 | [输入契约](docs/superpowers/specs/2026-10-31-stage6-streamed-elf-input.md)、[失败与实测](docs/superpowers/reports/2026-10-31-stage6-streamed-elf-input.md) |
| 2026-10-30（裸机审计停止） | **M1b 控制器故障注入补齐**：七个真实执行边界双轮停止、首次诊断与目标状态保持，停止调用变异被拒绝；Gate 接线/G1–G3 仍未完成 | [QEMU 验证报告](docs/superpowers/reports/2026-10-30-stage6-audit-stop-qemu.md) |
| 2026-10-30（审计停止） | **M1b 审计停止控制器接线**：独立首次诊断、持久停止、私有等待及销毁预检、分派/调度/IRQ 入口停止；Gate 完整事务/G1–G3 仍待完成 | [控制器报告](docs/superpowers/reports/2026-10-30-stage6-audit-fail-stop.md) |
| 2026-10-30（私有等待） | **M1b 等待基础落地**：真实 WaitObject/WaitRequest 的预留、不可警报登记、代次完成及任务销毁清理，普通权限/信号/超时出口隔离；Gate attach/trap/reply 与 G1–G3 仍未完成 | [私有等待报告](docs/superpowers/reports/2026-10-30-stage6-private-completion-waits.md) |
| 2026-10-30（内核前置） | **M1b/M1c 内存基础落地**：完整输出权限检查、显式 root 读入、真实 PTE/帧归属 COW 预留回滚；W9 增加 fork 后跨两页输出及父子隔离。Gate 对象/ABI/陷阱仍待接线，不勾选阶段条件 | [内存事务报告](docs/superpowers/reports/2026-10-30-stage6-user-copy-transactions.md) |
| 2026-10-30（整改） | **当前评审阻塞按 M1a 范围收口**：R1–R6 由明确协议与 8 组模型反例响应，候选 A 完成零 Linux 语义技术设计评审；权限位集扩为 u64；D14 ring3 探索双轮通过。尚未接线 Gate、不勾选阶段条件 | [整改评审与实测](docs/superpowers/reports/2026-10-29-stage6-m1-blocker-closure.md)、[协议](docs/superpowers/specs/2026-10-29-stage6-m1-protocol.md) |
| 2026-10-29（复审） | **阶段 6 继续进行；S6-M1 需修订**：六条路线保留，R1–R6 指出无时限保证、交付后取消/重放风险、跨任务复制与权限位宽缺口、W17/能力闭合和容量预留义务。此次为静态预审；7-A 检查点与 COSIR 正式评审未通过，不勾选任何阶段条件 | [六项决定复审](docs/superpowers/reports/2026-10-29-stage6-m1-decision-review.md) |
| 2026-10-29（裁决） | **D17–D22 关闭**：S6-M1 追加号段 73–78（逐项分配、不重编号 1–72）；v1 答复只写 `rax`；请求者内存用内核代理复制；门控任务 `fork`/`exec` 由 LibOS 答复 `ENOSYS` 留 S6-M4；抽象模型复用 `BlockedWait`＋请求对象；队列满阻塞不丢弃。不勾选 S6-D2 | [机制裁决](docs/superpowers/specs/2026-10-29-stage6-m1-mechanism-decisions.md) |
| 2026-10-29（启动） | **阶段 6 启动（作者指令）**：状态 未开始→进行中；D10 的实质约束（Linux persona 留用户态、内核只做中立机制）不变；D14 的 7-A 探索原型改为 S6-M1 评审前检查点；首个增量＝D13 `AT_RANDOM` + D12 白名单台账 | [启动报告](docs/superpowers/reports/2026-10-29-stage6-start.md) |
| 2026-10-28（裁决） | **D11–D16 关闭**：D11 动态链接维持阶段 7（7-E）；D12 白名单冻结为 `busybox-static 1.36.1` 六 applet + 2 探针（sha256 固定）；D13 提供公开确定的 `AT_RANDOM`（非机密、无 ASLR/canary 安全主张）；D14 多 LibOS 升 7-A P0 且阶段 6 前先做有界探索原型；D15 SMP 前先做状态按 CPU 分区；D16 阶段 7 拆为 9 条子轨。**不启动阶段 6**，S6-M1 递送仍需架构评审 | [前置裁决](docs/superpowers/specs/2026-10-28-stage6-7-release-decisions.md) |
| 2026-10-28（实现） | 阶段 6 准备性硬化：ELF 段同余对齐、`PT_LOAD` 上限 4→8、W+X/可执行栈拒绝、初始栈 auxv 收敛并补齐 `AT_EXECFN`；阶段状态不变、未勾选任何条件 | [装载切片报告](docs/superpowers/reports/2026-10-28-stage6-loader-slice.md) |
| 2026-10-27 | 本轮统一技术验收完成：host1016、BIOS单次68/68、Linux参考20项及分层检查通过；profile范围/组织级签署待确认（当日晚由 SPEC-152 签署） | [统一报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)、[具体评审工件](docs/superpowers/specs/2026-10-26-stage5-release-decisions.md) |
| 2026-10-26 | 用户选择D6 A：有原生OS接口的单线程std子集；开始统一验收（当时阶段5未签署，次日由 SPEC-152 签署） | [D6裁决](docs/superpowers/specs/2026-10-26-rust-d6-scope-decision.md) |
| 2026-10-22/23 | M12–M21 与 S6-A–E 按路线图推进；D2/D3/D5/D8 等工作决定不等于组织级关闭；号段从既有接口追加、Prima 命名 | [路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)、M15/M17/M19设计 |
| 2026-10-18 | 阶段5先冻结 profile 与 ABI 归属，再扩展 libc；DOC-002 仍属候选、非已评审权威约束 | [修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md) |
| 2026-10-17 | 阶段0–4按 SPEC-145–149 各自范围签署，阶段0限文本 | 各退出复核报告 |
| 2026-10-23 文档整理 | 移除重复的过程堆叠，按后继证据修正陈旧摘要；不改退出勾选、不新增技术结论 | [文档整理报告](docs/superpowers/reports/2026-10-23-documentation-restructure-report.md) |

## 阶段 0 — 语义基线

**状态：已满足退出条件（2026-10-17，SPEC-145；规范文本范围）**

### 交付检查

| ID | 交付物 | 状态 | 证据 |
|---|---|---|---|
| S0-D1 | 一致的对象能力状态模型 | [x] 已定义 | [原语规范](COSIR%20原语规范.md) |
| S0-D2 | 操作、错误、全局不变量和非目标 | [x] 已定义 | 规范及 SPEC-145 文本复核 |
| S0-D3 | 研究假设与反例标准 | [x] 已定义 | [白皮书](Prima%20OS.md)§3/§14 |
| S0-D4 | x86_64 状态到 POSIR 的细化草案 | [x] 设计，未完成内核细化证明 | 规范§20与[细化设计](docs/superpowers/specs/2026-08-07-blk002-refinement-design.md) |

### 退出条件

| ID | 条件 | 满足 | 证据与限制 |
|---|---|---|---|
| S0-X1 | 无已知且未记录的生命周期空洞 | [x] | SPEC-142 未通过 → [SPEC-143](docs/superpowers/reports/2026-10-17-stage0-review-remediation-spec143.md)/[144](docs/superpowers/reports/2026-10-17-stage0-re-review-spec144.md)整改 → [SPEC-145](docs/superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md)文本签署；旧“暂不勾选”是前序状态 |
| S0-X2 | 每项操作有明确成功与失败语义 | [x] | SPEC-145 对规范§19清单的文本复核；不等于所有原语已实现或证明 |
| S0-X3 | 形式化工具能够表达并执行状态与迁移 | [x] | [Lean设计](docs/superpowers/specs/2026-08-18-blk001-lean-formal-model-design.md)、[映照契约](lean/README.md)；只覆盖明确子集 |

前序未通过、整改与 CDT 证明链见[语义主题](docs/architecture/topics/semantics.md)，不将签署外推为完整机器细化。

## 阶段 1 — 启动与最小平台层

**状态：已满足退出条件（2026-10-17，SPEC-146）**

### 交付检查

- [x] S1-D1：QEMU 可重复启动；证据：TEST-001、TEST-002、TEST-003（固定 BIOS profile 连续两次）。
- [x] S1-D2：串口结构化输出与确定性退出；证据：IMPL-001、TEST-001、TEST-002、TEST-003。
- [x] S1-D3：物理内存发现；证据：页帧清单与保留区域测试（SPEC-017）。
- [x] S1-D4：初始页表、GDT/IDT 与 trap 框架；证据：异常和权限测试（SPEC-018）。
- [x] S1-D5：Bootstrap 创建 init Task；证据：SPEC-020 初始对象和 Capability 快照。

### 退出条件

- [x] S1-X1：脚本可无人值守启动并退出 QEMU；证据：TEST-001、TEST-002、TEST-003。
- [x] S1-X2：init 只获得规范定义的初始 Authority。证据：SPEC-061/TEST-008（image Segment、空 Namespace、结构性根、init `Ready→Running`、初始 Audit、`booted`/`AlreadyBooted`、入口范围前置、权限 bitset 去通配符；QEMU 实测 `authority_ready`）。**已勾选（SPEC-146，2026-10-17）**：`0b63bc1` 上复核 `create_state` 的 3 条根/权限/root_ns/Ready→Running/Audit/AlreadyBooted/入口前置；`ISSUE-AUTH-001` 的 §14 image/vspace 偏差按显式细化分叉接受，不阻塞 Authority。
- [x] S1-X3：活动 Segment 与空闲页帧满足唯一归属不变量。证据：SPEC-061/TEST-008（`FrameRegistry` 三分类、带标签取帧、守恒等式 QEMU 实测 `conserved=1`）。**已勾选（SPEC-146，2026-10-17）**：`FrameRegistry` 逐帧 image 向量 + owner 表 + 保留页表帧；`check_conservation`/`check_disjoint`；QEMU `frames_ready` 断言 `conserved=1/disjoint=1`；`ISSUE-FRAME-001/002` 关闭。

**阶段 1 关闭依据**：SPEC-061/TEST-008 的实现与 QEMU 证据 + SPEC-146 复核签署。S1-X2/S1-X3 已勾选；`ISSUE-AUTH-001` 的两条 §14 image/vspace 细化分叉保留为阶段 2/4 缺口，不阻塞 Authority/帧归属条件。

补充整改见 [SPEC-150](docs/superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md) 和 [SPEC-151](docs/superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)；后者关闭 4 项遗留问题。

## 阶段 2 — Capability、Segment、Task 与 IPC

**状态：已满足退出条件（2026-10-17，SPEC-147）**


阶段 2 的实现过程及历史缺口见 SPEC-062–075 报告；签署依据为 [SPEC-147](docs/superpowers/reports/2026-10-17-stage2-exit-review-spec147.md)。带 Capability 的阻塞直投已按[签署后整改](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)复验。

### 交付检查

- [x] S2-D1：不可伪造且不复用的句柄表示。**已勾选（SPEC-147，2026-10-17）**：`Handle` encode/decode + generation + 槽索引上界校验；stale/畸形句柄 host 单测。
- [x] S2-D2：Capability 派生、移动、drop 与 stale 检测。**已勾选（SPEC-147，2026-10-17）**：`SYS_CAP_DERIVE`/`SYS_CAP_DROP`、权限收窄、结构性根保护、stale token 可 drop；host 单测。
- [x] S2-D3：Segment create/map/unmap/destroy。**已勾选（SPEC-147，2026-10-17）**：`SYS_MEMSEG_*`、真实 PTE 装/卸、空表帧回收、帧归还、新段清零；`memseg_ready` QEMU。
- [x] S2-D4：Task create/yield/destroy 与单核调度。证据：SPEC-065/TEST-012（完整 §8.1 `SYS_TASK_CREATE`、§8.2 `SYS_TASK_DESTROY`、`SYS_YIELD`；QEMU 实测 `task_ready`）。**已勾选（SPEC-147，2026-10-17）**：`task_ready` 的 create/bad_entry/no_x/deny/destroy/again/map_dead/root_cleared 全部被 xtask 断言。
- [x] S2-D5：Send/Recv、FIFO 邮箱与阻塞恢复。**已勾选（SPEC-147，2026-10-17）**：`SYS_SEND`/`SYS_RECV`、FIFO/直投、接收者绑定授权；`ipc_ready` QEMU。 **2026-10-18 签署后整改**：ISSUE-IPC-001 已修复，ring3 `ipc_ready` 新增带 Cap 的阻塞直投见证；详见[阶段 2 复核报告](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)。

### 退出条件

- [x] S2-X1：每项成功和失败路径具有状态转换测试。证据（实现侧）：SPEC-062/063/064/065 的 host 单测 + QEMU 负例注入，调度迁移事件由 SPEC-071/TEST-018 补齐；**已勾选（SPEC-147，2026-10-17）**：`state_audit_ready`（`total=50;to_running=22;to_ready=22;to_blocked=5;to_faulted=1`）。
- [x] S2-X2：两个不可信 Task 只能经授权消息或共享 Segment 交互。**已勾选（SPEC-147，2026-10-17）**：`ipc_ready` 的 `cross=EACCES;send_denied=EACCES` 与 `task_ready` 的 `map_other=EACCES`。
- [x] S2-X3：页帧在对新 Segment 可见前清零。**已勾选（SPEC-147，2026-10-17）**：`memseg_ready.zero=0/zero2=0`；`create_segment_object` 在映射前调用 `mem::zero_frame`。
- [x] S2-X4：stale handle 不会因 ID 复用重新获得 Authority。**已勾选（SPEC-147，2026-10-17）**：generation 释放递增；`stale_handle_is_rejected_after_slot_reuse`；越界解码不 panic。

## 阶段 3 — Namespace、Timer 与 Audit

**状态：已满足退出条件（2026-10-17，SPEC-148）**

**2026-10-18 实现复核整改**：[阶段 3 复核报告](docs/superpowers/reports/2026-10-18-stage3-review-remediation.md)修复 4 项：Mount 图 DFS 栈按边数覆盖，`ns_destroy` 补齐逐条 `NsUnbound`/`NsUnmounted`，对象生命周期 Audit 按准确类型关联，派生链查询上界改为追加式历史容量。新增反例 host 测试；workspace tests、lint、BIOS QEMU 双跑通过。现行 `namespace_ready.unbinds=2`、`state_audit_ready.used=606`；SPEC-148 与 SPEC-151 报告中的旧数值保留为其当时基线。阶段 3 交付/退出条件维持已完成，其证据范围仍为 host/QEMU 有界见证。

### 交付检查

- [x] S3-D1：Namespace bind/unbind/mount/unmount/resolve。**已勾选（SPEC-148，2026-10-17）**：七个操作 + `namespace_ready` 的 binds/unbinds/mounts/unmounts/resolves 计数。
- [x] S3-D2：Mount 环检测与 stale Mount 清理。**已勾选（SPEC-148，2026-10-17）**：DFS + visited 位集；`cycles_rejected=1`/`acyclic_ok=2`；stale Mount 由 `ns_unmount` 清理。
- [x] S3-D3：绝对单调 Timer。**已勾选（SPEC-148，2026-10-17）**：`now` 由 PIT tick 推进；`InvalidTime` 保持状态不变；`timer_ready.now=4`。
- [x] S3-D4：`deadline ≤ now` 时原子立即触发。**已勾选（SPEC-148，2026-10-17）**：`timer_ready.immediate_fired=1`。
- [x] S3-D5：Capability 派生、位置变化与对象生命周期 Audit。**已勾选（SPEC-148，2026-10-17）**：`audit_ready.chain_len=2;derived=1;moves=2;created=1;stale_ok=1`；`SYS_AUDIT` 纯查询。

### 退出条件

- [x] S3-X1：所有 Namespace 解析均终止，活动 Mount 图无环。**已勾选（SPEC-148，2026-10-17）**：路径分量缩短 + DFS/visited；`cycles_rejected=1`/`acyclic_ok=2`。
- [x] S3-X2：每个 Fired Timer 恰有一次对应内核通知。**已勾选（SPEC-148，2026-10-17）**：`timer_ready.exactly_once_ok=1`。
- [x] S3-X3：Audit 能重建 Capability 派生链和位置历史。**已勾选（SPEC-148，2026-10-17）**：`audit_ready.chain_len=2;derived=1;moves=2`；查询不追加事件。
- [x] S3-X4：Task、Namespace、Message 或 Timer 容器销毁不破坏其他对象生命周期。**已勾选（SPEC-148，2026-10-17）**：按 recipient/条目清理；host 单测与 `namespace_ready` 的 c 销毁路径。

## 阶段 4 — Prima 原生运行时与服务

**状态：已满足退出条件（2026-10-17，SPEC-149）** S4-D1 Runtime API/版本协商（SPEC-084/085），S4-D2 ring3 服务管理器（SPEC-103–105、109），S4-D3 三个有状态基础服务（SPEC-106、110–112），S4-D4 用户态驱动/受控 I/O（SPEC-113/114），S4-D5 及 X1–X4 的收口证据见 [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md)。


后续硬化与边界见 [SPEC-149](docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md) 和[阶段 1–4 实现复核](docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md)。

### 交付检查

- [x] S4-D1：原生运行时 API 与版本规则；SPEC-084/085 + SPEC-130。
- [x] S4-D2：init / 服务管理器；SPEC-103–105、109 + SPEC-130。
- [x] S4-D3：日志、Namespace 和基本存储服务；SPEC-106、110–112 + SPEC-130。
- [x] S4-D4：第一条用户态驱动或受控 I/O 路径；SPEC-113/114 + SPEC-130。
- [x] S4-D5：使用消息与共享 Segment 的原生示例应用；[SPEC-129](docs/superpowers/reports/2026-10-16-stage4-native-shared-segment-app-report.md) 的一页起点扩为 [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md) 的 8 KiB 跨页路径，QEMU 两次运行。

### 退出条件

- [x] S4-X1：不依赖 POSIX LibOS 即可自举并运行应用；SPEC-130 的独立 ring3 Rust 原生客户端和 BIOS QEMU 双次启动。
- [x] S4-X2：服务按现行 COSIR §8.1 的最小权限拆分；[SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md) 列出初始 Capability、结构性空 Namespace 与运行期额外授予。
- [x] S4-X3：共享 Segment 传 8 KiB 跨页数据；SPEC-130 的执行前取消、越界长度拒绝与恢复重试由客户端/管理器双侧和 QEMU trace 校验。
- [x] S4-X4：驱动从 ring3 向无权映射的客户端数据 VA 写入产生真实 `#PF`，驱动变 `Faulted`，无关客户端随后完成两次校验并到达 `boot_complete`；SPEC-130。

**下游义务登记**：阶段 5 的基座库栈（DOC-002）消费一批内核 ABI 语义，清单与归属见「阶段 5 · 上游前置」表与 BLK-003；这些**不属本阶段交付项**，进入阶段 5 前须逐项评审确认。
## 阶段 5 — POSIX 源码兼容

**状态：已满足退出条件（2026-10-27，SPEC-152）。** 签署范围、验收命令与未覆盖项见[签署报告](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md)；[M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md)/[M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)保留为签署前评审记录，本节不复制完整兼容矩阵。

### 里程碑证据与边界

| 里程碑 | 已取得的技术证据 | 当前限制 / 后继 |
|---|---|---|
| M0–M4 | [M0盘点](docs/superpowers/reports/2026-10-18-stage5-musl-m0-inventory.md)、[M2边界](docs/superpowers/reports/2026-10-18-stage5-musl-m2-boundary.md)、[M3 musl](docs/superpowers/reports/2026-10-18-stage5-musl-m3-base.md)、[M4文件](docs/superpowers/reports/2026-10-18-stage5-musl-m4-file.md) | 首切片不等于完整 libc；固定初始栈/单文件后端由后继扩展 |
| M5–M7 | [sbase echo](docs/superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md)、[时间](docs/superpowers/reports/2026-10-19-stage5-m6-time.md)、[fd/dup](docs/superpowers/reports/2026-10-19-stage5-m7-fd-table.md) | 冻结用例与 partial 边界；S5-X3 待复核 |
| M8–M11 | [VFS](docs/superpowers/reports/2026-10-20-stage5-m8-user-vfs.md)、[metadata/cwd](docs/superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md)、[匿名内存](docs/superpowers/reports/2026-10-21-stage5-m10-anon-memory.md)、[树变更](docs/superpowers/reports/2026-10-22-stage5-m11-file-mutation.md) | 固定容量、匿名 arena 与有限文件语义 |
| M12–M13 | [启动/退出](docs/superpowers/reports/2026-10-22-stage5-m12-c-startup.md)、[链接/rename](docs/superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md)、[权限/stdio](docs/superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md) | A2有界载体和W7b CLOEXEC本轮通过；无硬链接/持久化 |
| M14 | [TTY原始范围](docs/superpowers/reports/2026-10-23-stage5-m14-tty-input.md)、[本轮IRQ/取消/重启](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | I1双轮通过；IRQ4/FIFO，有界行规程，无VTIME/流控/job control |
| M15–M16 | [原始线程证据](docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)、[本轮线程/futex/取消](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | W2–W5双轮通过；有限属性/取消/分离出口；共享线程视图待最终范围确认，不承诺完整pthread |
| M17 | [fork/COW原始范围](docs/superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)、[本轮完整后继](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | W6/W7/W7b双轮和Linux参考通过；有界独立视图/OFD、新crt/连续exec；非多线程fork/动态装载 |
| M18 | [原始signal范围](docs/superpowers/reports/2026-10-23-stage5-m18b-sigaction.md)、[本轮mask/EINTR等](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | W8/W9双轮通过；合并通知/8层现场，无altstack/实时排队，#AC无机器见证 |
| M19 | [loopback UDP N1](docs/superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md) | 仅环回 IPv4/UDP，无 NIC/TCP/DNS |
| VFS / LibOS 基础 | [原始验证](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)、[本轮会话/OFD](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | 认证Process holder/fork关闭隔离已验证；每LibOS根及多personality仍未实现 |
| musl 后继补强 | [原始边界报告](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)、[本轮验证](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | 八补丁锁定构建/归档和host5项通过；信号/取消/exec只覆盖本轮有界见证 |
| M20–M21 | [历史出口评审](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)、[本轮技术验收](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | D6 A已验证；profile v2与D1–D10可审阅，最终范围及组织级签署待确认 |

### 交付检查

以下勾选为 SPEC-152（2026-10-27）的签署结果；"当前子集满足"限于 v2 冻结边界，不外推完整 POSIX 语义。

| ID | 交付物 | 勾选 | 签署处置（SPEC-152；M21 判定保留于评审报告） |
|---|---|---|---|
| S5-D1 | 冻结且版本化 POSIX profile | [x] | [profile v2](docs/superpowers/specs/2026-10-25-stage5-profile-v2.md) 于 2026-10-27 冻结；v0/v1 为历史契约 |
| S5-D2 | libc适配 | [x] | 有界信号/线程/退出/exec 与本轮 host、Linux 参考、机器双轮通过；边界即 v2 |
| S5-D3 | fd、路径、线程和基础网络子集 | [x] | RAM VFS/OFD、线程、环回 UDP；无持久化/硬链接/NIC/TCP/DNS（见签署报告 unsupported） |
| S5-D4 | 逐API语义映射矩阵 | [x] | [逐入口表](docs/superpowers/compat/stage5-working-api.md) 与本轮证据对齐；全部 partial 不外推 |
| S5-D5 | Linux差分矩阵 | [x] | 21 项同源参考及完整 BIOS 双轮通过（含 W7/W7b）；有限见证，不证明全部参数组合 |
| S5-D6 | Rust target/core/std | [x] | [D6 A](docs/superpowers/specs/2026-10-26-rust-d6-scope-decision.md) 的 R1/R2/R3 双轮与 Clippy 通过；B 不在本轮 |
| S5-D7 | persona、符号版本、台账与ABI护栏 | [x] | 静态声明/版本锚点/可见性脚本/拒绝负例通过；无动态版本解析、无内核 persona |

签署范围与最终关闭门见[签署报告](docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md)；[具体评审工件](docs/superpowers/specs/2026-10-26-stage5-release-decisions.md)记录 D1–D10 的范围来源。

### 上游 ABI 与候选规格归属

BLK-003 已按阶段 5 范围关闭（2026-10-27，SPEC-152）。原生号段以 [prima-abi](prima-abi/src/lib.rs)为定义源；1–72的工作映射（52保留、新59–72按本轮host和具体验收程序取证）见[台账](docs/superpowers/compat/compat-state.md)和[接口说明](docs/architecture/interfaces.md)，52未分配。候选 DOC-002 的 `0x0xx`、RUXOS 命名与时延目标不自动进入 Prima 契约。

WaitObject/futex、信号、fork/exec、TTY 输入、匿名内存已经有各自的后继技术证据，不能继续以候选阶段的“未实现”表代替当前状态。直接 IPC ≤250ns、动态链接归属、Rust 范围、persona/符号版本及未收口语义按[路线图 D1–D10](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)与 SPEC-152 的签署范围处置；同步 call/reply 快路径和性能指标不因队列 IPC 通过而完成。

候选规格与风险走查保留在[基座库规格](docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md)和[走查记录](docs/superpowers/specs/2026-10-11-baselib-stack-walkthrough.md)。这些是设计输入，不是本仓库已接受的排期或ABI承诺。

### 退出条件

| ID | 条件 | 勾选 | 当前依据 |
|---|---|---|---|
| S5-X1 | profile每项API均有映射与可重复测试 | [x] | v2 冻结边界内逐行有映射与测试；不外推逐 API 全部参数 |
| S5-X2 | 每项API标记supported/partial/unsupported | [x] | v2 的 30 行全部 partial；supported 仅 v0 的 2 行；unsupported 清单见签署报告 |
| S5-X3 | 选定开源程序通过冻结测试集 | [x] | echo E1–E6、cat C1、ls L1–L3 双轮 + Linux 参考；集限 sbase echo/cat/ls |

## 阶段 6 — Linux x86_64 静态 ELF

**状态：已满足退出条件（2026-11-04，技术复核4/4）。** 按作者完成全部余项的指令实施并统一验收，
[统一验收](docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)记录完整源码、输入身份、逐调用矩阵、失败/整改和最终运行范围。
普通完整BIOS36组72/72、Linux同ELF11项双轮、G1–G4及七审计停止边界双轮、D14原探针双轮均通过；
普通/Linux特性host1118/1119、lint/特性Clippy、Lean及文档工具通过。技术复核不自动构成组织级签署，阶段7仍未开始。

[D12白名单](docs/superpowers/compat/stage6-static-whitelist.md)八成员及[Linux profile v1](docs/superpowers/compat/stage6-linux-profile-v1.md)有界冻结；
每行Linux入口为partial，未实现项明确unsupported。D11动态链接留阶段7；D13公开确定AT_RANDOM无机密/ASLR主张。
D14仅单核双人格桩探索与模型，不冒充7-A完整交付。前序构建切片和历史超时/失败按报告链保留；
本轮明确后继其“未执行”状态，不改写历史结果。普通VFS任意大文件exec、实时信号/备用栈、XSAVE/AVX、
完整pid命名空间及完整POSIX/Linux不在本冻结范围。

2026-11-06[复核整改](docs/superpowers/reports/2026-11-06-stage6-review-remediation.md)补齐既有容量边界与实现硬化，
不扩大profile或改变退出勾选。S6-X3的窄口径守卫已接入lint；阶段5原生`clone_call`仍含既有`CLONE_*`
标志适配，不能把“Gate不解释Linux调用”外推为全仓库没有任何Linux形状的适配。此审查遗漏与门禁限制见整改报告。

### 交付检查

- [x] S6-D1：静态 ELF 加载器与 Linux 初始栈。冻结制品真实PT_LOAD/BSS和8页栈双轮运行，argv/env/auxv与失败回收按契约验证。
- [x] S6-D2：OS 中立的 syscall / exception 递送机制。实验性73–89、代理复制/上下文/生命周期、G1–G4及审计停止双轮通过；TCG分段计量不承诺硬件延迟。
- [x] S6-D3：版本化的 Linux LibOS syscall profile。v1按命名观察和partial/unsupported范围冻结。
- [x] S6-D4：所选 profile 必需的 futex、TLS 和进程环境。P1/P2、静态pthread及fork/exec/wait矩阵双轮通过。
- [x] S6-D5：未修改静态 ELF 白名单与测试镜像。D12八成员原始字节在pinned Linux/Prima各项通过，额外M3/M4不扩白名单。

### 退出条件

- [x] S6-X1：白名单中的静态 ELF 无需修改即可运行。D12八成员双轮通过。
- [x] S6-X2：支持的 syscall 与参考 Linux 通过差分测试。v1命名变体与M3/M4矩阵双轮通过。
- [x] S6-X3：HexArc / COSIR 不包含 Linux 专属对象或 syscall 语义。Gate递送路径与COSIR按有界范围复核；既有原生clone适配偏差及本次源码守卫的限定范围见[RV整改](docs/superpowers/reports/2026-11-06-stage6-review-remediation.md#审查纠偏与门禁边界)，不外推为全内核无Linux形状适配的证明。
- [x] S6-X4：未支持 syscall 得到确定且可审计的失败。ENOSYS原号/参数/cookie/结果与一次性终结核对通过。

> 注：候选规格（DOC-002）把 `ld.so` 最小动态链接闭环列为 P0、随阶段 5 交付，而白皮书把“动态链接”列为阶段 7 候选。**已裁决（D11/D4，SPEC-152）**：动态链接留阶段 7 的 7-E，阶段 6 的输入必须无 `PT_INTERP`；不得两处各写一套。

## 阶段 7 — 扩展与硬化

**状态：未开始；综合交付范围已确认，技术规格与量化门槛待冻结。** 作者在
[2026-10-31 范围讨论](docs/superpowers/specs/2026-10-31-stage7-scope-decisions.md)中逐项确认下列范围，
承接 D16 的九条子轨结构。九轨均为必交付，排序仍是假设，各轨需自身退出签署；本次范围确认不启动阶段7。

| ID | 子轨 | 优先级 | 退出条件（摘要） | 勾选 |
|---|---|---|---|---|
| 7-0 | 工程底座（CI、验收分组、基准、发布工件） | P0 | `test/lint/tcb/docs check` 在 CI 通过；5项基准及制品可复核；基线后冻结的性能门槛通过 | [ ] |
| 7-A | Linux + Prima 原生/POSIX 并存 | P0 | 真实白名单、独立语义状态/根/能力视图、显式共享服务；未授权跨域拒绝；单侧故障和耗尽后另一域持续进展 | [ ] |
| 7-B | x86_64 SMP（先做状态按 CPU 分区） | P0 | 2/4 vCPU；并行 fork/exec 无丢唤醒；TLB shootdown 正确；扩展性曲线 | [ ] |
| 7-C | 核心资源及I/O配额 | P1 | CPU/内存/Task/句柄/队列/审计及磁盘空间/I/O请求/网络带宽逐项计量；超限隔离与生命周期对账 | [ ] |
| 7-D | 完整选择性撤销 | P1 | 能力、会话、映射、DMA及未决请求分别定义完成点；旧授权不能再访问或因重启复活，无关域不受影响 | [ ] |
| 7-E | 双 personality 动态链接 | P2 | Linux未修改动态ELF与Prima原生/POSIX动态应用分别通过冻结程序/库白名单 | [ ] |
| 7-F | IOMMU、用户态驱动与持久化 | P2 | 块设备与网络驱动、DMA隔离、从盘启动、跨重启文件持久化分别验收 | [ ] |
| 7-G | 用户态组件透明恢复 | P2 | 冻结端到端协议内，LibOS/服务/驱动崩溃后应用继续执行，请求不丢失、不重复产生副作用；配额/审计一致 | [ ] |
| 7-H | AArch64 单核机制验证 | P2（后期） | 同一POSIR契约下能力隔离、内存、调度、IPC的对应 `COMPAT-TEST`；不要求完整应用或设备对等 | [ ] |

顺序：`7-0 → 7-A → 7-B → 7-C/D → 7-G → 7-E/F → 7-H`。D14 按启动记录调整为在 S6-M1 正式架构评审前完成一次 7-A 的**有界探索原型**（单核、≤2 人周、不勾选任何条件）；本轮[ring3 人格桩探索](docs/superpowers/reports/2026-10-29-stage6-m1-blocker-closure.md)已双轮通过，未否定该有界假设；仍不构成完整多 LibOS 或 7-A 退出。若后继原型否定，暂停阶段 6 并回到架构假设评审。SMP 不在白皮书成功标准内，但它是可信度门槛（D15）。

上述顺序是起步假设，非新确认的严格串行计划：7-A耗尽隔离需最小配额先到位，7-G完整恢复
验收需对应7-E/F功能。正式验收平台为QEMU，实机不作退出条件；应用范围为命令行与服务白名单。
关键配额、撤销和恢复性质必须建模并证明，并附实现映照与故障注入证据；量化门槛、白名单、
平台配置和故障协议须按[冻结门](docs/superpowers/specs/2026-10-31-stage7-scope-decisions.md#8-实现前和签署前的冻结门)进一步确定。
整机重启后的应用续执行不在透明恢复范围，文件持久化义务独立保留。本次只更新范围，不新增运行或证明结果。

独立候选扩展：[Windows LibOS 计划书](docs/superpowers/plans/2026-10-31-windows-libos-roadmap.md)。
作者已选择控制台与服务优先（PE64、DLL、文件、进程/线程、同步与异常）；技术路线及profile仍为提案，
实现未启动，不替换上述Linux + Prima原生/POSIX组合，也不自动增加第三个阶段7必交付personality。

文档维护记录：[2026-10-24 生命周期治理与清理](docs/superpowers/reports/2026-10-24-documentation-lifecycle-report.md)仅更新文档与检索工具，不新增 OS 运行证据，不改变上述阶段状态、勾选、profile 或阻塞结论。

[2026-10-31 日志附件整理](docs/superpowers/reports/2026-10-31-documentation-log-inline.md)将必要输出内嵌到原证据报告，移除独立日志附件并保留Git恢复路径；不新增运行证据，整理时的退出0/4保持；2026-11-04的技术退出后继见阶段6节，历史失败保留。

## 阶段晋级规则

1. 阶段状态由该阶段的退出条件决定，不由提交数量、代码行数或主观完成度决定；
2. 前一阶段未满足退出条件时，后一阶段可以做探索原型，但不能成为“当前已完成阶段”；
3. 失败测试、反例和实现偏差应写入对应报告；仍影响当前阶段的事项进入上方问题表；
4. 关闭阶段时，在签署报告中记录完整命令、工件版本和结果，并从证据索引链接该报告；
5. 架构或语义变化必须先更新权威文档，再重新评估受影响的已关闭检查项。
