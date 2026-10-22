# Prima OS 实现阶段

> 更新：2026-10-18。架构见 [[Prima OS]]，精确语义见 [[COSIR 原语规范]]。本文记录当前进度、退出条件和证据范围；历史逐切片记录可在 [2026-10-18 压缩前版本](https://github.com/Alma1314/PrimaOS/blob/a434a2f0c515ac3421e999be3c436b7d7c6c5313/Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md) 查阅。

## 维护规则

- 状态为未开始、进行中、阻塞或已满足退出条件；设计完成不等于实现完成。
- 勾选检查项前，应给出可访问的工件和相应范围的证据。host 测试、QEMU 执行、Lean 模型和机器证明不能互相替代。
- 失败与反例须留在报告中。阶段定义先改 [[Prima OS]]，语义先改 [[COSIR 原语规范]]，再更新本文。

## 当前快照

| 项目 | 状态与依据 |
|---|---|
| 阶段 0–4 | 已满足退出条件；签署报告依次为 [SPEC-145](docs/superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md)、[146](docs/superpowers/reports/2026-10-17-stage1-exit-review-spec146.md)、[147](docs/superpowers/reports/2026-10-17-stage2-exit-review-spec147.md)、[148](docs/superpowers/reports/2026-10-17-stage3-exit-review-spec148.md)、[149](docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md)。阶段 0 的结论限规范文本；实现、模型和 QEMU 各按报告范围解释。 |
| 当前阶段 | 阶段 5 已启动：无 libc 静态 PIE C ELF 在全量服务后运行、自毁并回收；[原生 ABI 1.0 握手](docs/superpowers/reports/2026-10-18-stage5-native-abi-check.md)、[固定 C 初始栈](docs/superpowers/reports/2026-10-18-stage5-c-start-stack.md)与[同源 `main` 返回码](docs/superpowers/reports/2026-10-18-stage5-main-return-status.md)、[同源 `write(1/2)`/`_exit` 的 tty 字节流](docs/superpowers/reports/2026-10-18-stage5-c-tty-write.md)、[`PT_LOAD` 写段/BSS 私有装载与 W^X](docs/superpowers/reports/2026-10-18-stage5-ptload-wx.md)获见证；[M2 C 系统边界与 per-task FS base](docs/superpowers/reports/2026-10-18-stage5-musl-m2-boundary.md)已完成（`SYS_TASK_SET_FS_BASE=38`、适配层调用集含 `exit(60)`、`__syscall4..6` 全参数透传、静态 PIE 构建期校验）；[M3 静态 musl 基座](docs/superpowers/reports/2026-10-18-stage5-musl-m3-base.md)已完成（SSE/FPU 契约、可重放补丁、Prima crt 接 `__libc_start_main`、探针链接 `libc.a` 并通过 `snprintf`/`getenv`/errno 断言），M3b 匿名内存亦已完成（服务管理器预映射 arena，M9 起为 96 页；`malloc/free/realloc` 通过），[M4 文件首切片](docs/superpowers/reports/2026-10-18-stage5-musl-m4-file.md)已完成（单文件 `f` 的 fd 3..6 生命周期、`openat/read/write/lseek/close` 冻结子集、Namespace `f` 授权与 `printf`+`fflush` 输出）；[M5 sbase `echo` 首切片](docs/superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md)已完成（E1–E6 在 Prima 双轮通过，Linux/Prima stdout 逐字节与退出码一致，S5-X3 证据已具备）；[M6 POSIX 时间首切片](docs/superpowers/reports/2026-10-19-stage5-m6-time.md)已完成（`CLOCK_MONOTONIC` + 10 ms PIT 轮询睡眠，`P5-TIME-01` partial，双轮输出与 Linux 参考逐字节一致，未勾选任何 S5-X）；[M7 fd 表切片](docs/superpowers/reports/2026-10-19-stage5-m7-fd-table.md)已完成（本地 fd 3..10、`dup`/`dup2`/`dup3` 共享 offset、`getpid`，双轮 QEMU 与 Linux 同源输出一致，`P5-FD-01`/`P5-PROC-02` 仍为 partial）；[M8 用户态 VFS/rootfs 切片](docs/superpowers/reports/2026-10-20-stage5-m8-user-vfs.md)已完成（静态 rootfs、多级/`.`/`..`/dirfd 路径、目录枚举、通用 fd 0..18、`/dev/null`、`getdents64`，V1 在 Prima 双轮通过且与 Linux 参考逐字节一致；`P5-FS-01`/`P5-FS-02`/`P5-FS-03`/`P5-FD-01` 仍为 partial）；[M9 元数据/工作目录切片](docs/superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md)已完成（`stat`/`lstat`/`fstat`/`fstatat` 经 x86_64 musl 的 kstat 号 4/5/6/262，`statx(332)` 亦实现；每任务 `chdir`/`getcwd` 与 `AT_FDCWD` 相对路径；M1 在 Prima 双轮通过且与 Linux 参考逐字节一致；sbase `cat` C1 与 `ls` L1–L3 双轮通过，`P5-FS-04`/`P5-FS-05` 新增为 partial，`P5-FS-01`/`P5-FS-03` 边界更新）；[M10 匿名内存/保护切片](docs/superpowers/reports/2026-10-21-stage5-m10-anon-memory.md)已完成（内核 `memseg_protect` 追加号 41 + `VspaceProtChanged` 审计；C 适配层 56 页 mmap 区按页分配、真正 `memseg_unmap` 与复映射，`mprotect` 经内核改权；A1 在 Prima 双轮通过且与 Linux 参考逐字节一致；`P5-MM-01` 新增为 partial）；[M11 文件树变更切片](docs/superpowers/reports/2026-10-22-stage5-m11-file-mutation.md)已完成（用户态 VFS 节点删除标记与槽位复用，`mkdir`/`mkdirat`/`unlink`/`unlinkat`/`rmdir` 经 MUTATE+PATH+COMMIT 到达存储服务；D1 在 Prima 双轮通过且与 Linux 参考逐字节一致；`P5-FS-06` 新增为 partial）。BLK-004 已关闭、BLK-003 仍 Open；POSIX profile v0 已冻结（S5-D1 待组织级确认后勾选），M8 另提交[profile v1 delta](docs/superpowers/specs/2026-10-20-stage5-profile-v1.md) 待评审，S5-D2–D7/X1–X2 未勾选，S5-X3 待组织级复核后勾选。见[C ELF 容量见证](docs/superpowers/reports/2026-10-18-stage5-c-elf-qemu.md)、[profile v0（冻结）](docs/superpowers/specs/2026-10-18-stage5-profile-v0.md)、[M8 设计](docs/superpowers/specs/2026-10-20-stage5-user-vfs-design.md)、[M10 设计](docs/superpowers/specs/2026-10-21-stage5-m10-anon-memory-design.md)与[修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md)。 |
| 下一阶段 | 阶段 6 未开始；阶段 7 范围未冻结。阶段 5 余项按[收尾路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)的 M12–M21 执行，阶段 6 入口为 S6-A–S6-E。 |
| 近期复核 | [阶段 1–4 实现复核](docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md)、[阶段 2 IPC 整改](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)、[阶段 3 整改](docs/superpowers/reports/2026-10-18-stage3-review-remediation.md)；后两者已通过 workspace 测试、lint 和 BIOS QEMU 双跑。 |
| TCB | 基线与历次统计见压缩前版本；最新数值应以 `cargo xtask tcb` 现跑结果为准，`kernel/src` 仍要求零 `unsafe`。 |

## 证据索引

| 范围 | 主要工件 | 证据边界 |
|---|---|---|
| 架构与语义 | [[Prima OS]]、[[COSIR 原语规范]] | 定义目标和操作语义；不代表实现或证明完成。 |
| 阶段 0–4 签署 | [SPEC-145](docs/superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md)、[146](docs/superpowers/reports/2026-10-17-stage1-exit-review-spec146.md)、[147](docs/superpowers/reports/2026-10-17-stage2-exit-review-spec147.md)、[148](docs/superpowers/reports/2026-10-17-stage3-exit-review-spec148.md)、[149](docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md) | 各报告记录检查项、命令、版本、结果及限制。 |
| 签署后修订 | [SPEC-150](docs/superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md)、[151](docs/superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)、[阶段 2](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)、[阶段 3](docs/superpowers/reports/2026-10-18-stage3-review-remediation.md) | 按整改报告读取最新实现；旧签署报告是当时基线。 |
| 阶段 4 技术证据 | [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md)及[后续复核 SPEC-149](docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md) | 原生运行时与服务；不等于 POSIX fd/VFS/tty。 |
| 阶段 5 输入 | [准入分析](docs/superpowers/reports/2026-10-18-stage5-entry-analysis.md)、[修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md)、[基座库候选规格](docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md) | 候选规格未经内核侧评审，不冻结 ABI 或排期。 |
| 阶段 5 余项与阶段 6 入口规划 | [阶段 5 收尾与阶段 6 入口实施路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md) | 计划，不代表实现；M12–M21、S6-A–E 按里程碑证据逐项推进，不改变本页勾选状态。 |

早期逐切片 SPEC/TEST、反例和具体命令保留在[文档导航](docs/README.md)所指的设计、计划、报告中；压缩前的逐行证据索引可从上方历史版本读取。

## 已知问题与纠偏计划

| ID | 当前口径 | 下一步 |
|---|---|---|
| ISSUE-CAP-001 | host 模型缺少完整 Capability 派生/位置溯源；非当前内核缺陷。见[差距分析](docs/superpowers/reports/2026-08-17-capability-derivation-model-gap.md)。 | 模型独立切片按不可伪造、权限单调和位置唯一验证。 |
| ISSUE-COW-001/002 | [SPEC-015](docs/superpowers/specs/2026-08-14-cow-refinement-design.md) 的 COW 边界澄清；用户态显式换映射及并发协调责任，非阻断缺陷。 | 若扩大 COW 语义，再做真实路径验证。 |
| ISSUE-AUTH-001 | §14 image/vspace 细化分叉已由 [SPEC-146](docs/superpowers/reports/2026-10-17-stage1-exit-review-spec146.md) 接受，不阻断 S1-X2；不能据此宣称 §14 映射已完整细化。 | 新用户程序段权限和 W^X 在阶段 5 单独验证。 |
| BLK-003 | 阶段 5 候选基座库对内核 ABI 的假设未评审，且号段/命名与 `prima-abi` 不一致。 | 原生版本握手已按追加号 `37` 落地；POSIX profile、C 入口栈、退出状态及错误映射仍按[阶段 5 修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md)逐项裁决，阻塞项保持 Open。 |
| BLK-004（已关闭） | 全量服务后 C ELF Task 16 创建、执行、自毁、栈回收、复建 `ENOSPC` 和四类容量水位获 QEMU 双轮见证，见[C ELF 报告](docs/superpowers/reports/2026-10-18-stage5-c-elf-qemu.md)。 | POSIX 启动/退出状态及 `write` 仍属片 C，不由此门槛代签。 |
| ISSUE-PTHREAD-CLONE-001（已关闭） | clone.s 裸 syscall 56 曾绕过适配层，`pthread_create` EAGAIN。 | 已打 musl clone.s 补丁（56→48、60→49）+ 内核 parent_tid 写 + 适配层 PROT_NONE mmap；create 已成功。musl 内部标记显示 `__pthread_exit` 确实执行（state=2、store DT_EXITED、wake）且早于父线程 create 返回，但 join 仍阻塞，怀疑 detach_state 地址/映射不一致；下一步打印父子 detach_state 地址与 join 读值。 |
| ISSUE-THREAD-CAPACITY-001（已关闭） | 线程创建曾因 18 槽位占满而 `ENOSPC`；扩容会破坏容量负例见证。 | M15b-2b：`thread_create` 惰性回收 Dead 记录（优先无 cap 指向，否则撤销 cap），`task_create` 保持“满 → ENOSPC”；W2 双轮通过，见证 12/132 不变。 |
| ISSUE-PTHREAD-CAPS-001（工作决定） | 线程共享地址空间/适配层 fd 表后，内核 Capability 位置空间仍是每 Task；新线程无法使用既有 cap（`holder_of(cap)` 不匹配），M16 pthread/futex 被阻塞。 | 工作决定：同 Process 的线程共享 leader 的 Capability 位置空间（`space_of`），单线程 Task 退化为自身 id；实现见 M15b-2a。该决定仍需 COSIR 组织级确认。 |
| ISSUE-TTYIN-001（部分关闭） | M14 的串口输入需要“数据到达 → 唤醒读者”。M14a 用“有读者时管理器轮询 COM1、无读者时阻塞 RECV”落地冻结子集；有读者期间的轮询仍是临时偏差，不是 WaitObject。 | M15 的 WaitObject 或内核串口 IRQ 递送落地后替换有读者轮询；在此之前 M14 只按 I1 证据标 `partial`，不宣称通用 console。 |
| ISSUE-START-ARGS-001 | M12 的独立 C 启动探针与 `ECHO_ARGS` 参数载体组合时，QEMU 在 `user_cow_ready` 之前出现 `SYS_RECV` 无 Ready 任务或用户页故障；去掉参数载体即通过。根因未定位，见[M12 报告](docs/superpowers/reports/2026-10-22-stage5-m12-c-startup.md)。 | 先以 host/单测与固定探针验证 envp/auxv；不得放宽内核调度断言。定位后再恢复任意 argv 端到端轮次。 |

其余已关闭问题的发现、修复与复核记录见相应报告和压缩前版本。`ISSUE-IPC-001` 已由阶段 2 整改关闭。

## 当前阻塞项

阶段 5 的 BLK-004 容量门槛已关闭；BLK-003 ABI 裁决仍是后续 libc/profile 工作的前置。BLK-001 已关闭；BLK-002 已形成细化草案，不能外推为机器证明。M12 的参数载体组合另登记 ISSUE-START-ARGS-001。

## 决策日志

- **2026-10-22：**M11 之后的阶段 5 余项与阶段 6 入口采用[收尾路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)（M12–M21、S6-A–E）；BLK-003 仍 Open，路线图不预设 42+ 的具体号或状态升级。M13a 按 D8 的建议作工作决定：符号链接/`rename`/`fchdir` 落地，硬链接/权限/持久化不做；D8 仍未组织级关闭。M19 按 D5 的建议作工作决定：阶段 5 只做用户态 loopback IPv4/UDP，NIC/DNS/TCP 留阶段 7；D5 仍未组织级关闭。M15 按 D2/D3 的建议作工作决定：42 起追加 Prima 命名 syscall，先在 COSIR/设计层确立 Process/Thread/ShareGroup 语义，再以 WaitObject（42–46）和多线程 Process 分片实现；D2/D3 仍未组织级关闭，设计见 [M15 设计](docs/superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)。
- **2026-10-18：**阶段 5 先冻结 profile 和上游 ABI 归属，再扩展 libc；采用[修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md)。
- **2026-10-17：**阶段 0–4 按 SPEC-145–149 的各自证据范围签署；阶段 0 仅为规范文本复核。
- **历史决策：**实现细节、取舍和曾被推翻的结论保留在逐切片报告及压缩前版本，不作为当前状态的替代来源。

## 阶段 0 — 语义基线

**状态：已完成（2026-10-17，SPEC-145 文本层面）**

### 交付检查

| ID | 交付物 | 状态 | 证据 |
|---|---|---|---|
| S0-D1 | 一致的对象能力状态模型 | [x] 已定义，未证明 | SPEC-001 |
| S0-D2 | 操作、错误、全局不变量和非目标 | [x] 已定义，未证明 | SPEC-001 |
| S0-D3 | 关键研究假设与反例标准 | [x] 已定义 | DOC-001 |
| S0-D4 | x86_64 机器状态到 POSIR 状态的细化草案 | [x] 已定义，未证明 | SPEC-013 |

### 退出条件

| ID | 条件 | 满足 | 证据 |
|---|---|---|---|
| S0-X1 | 规范没有已知且未记录的对象生命周期空洞 | [x] | **SPEC-142 第三方组织级复核（2026-10-17）未通过 → SPEC-143 逐项整改 29 项 → SPEC-144 残修 3 项 → **SPEC-145 冻结基线复核（`16ff1fc`）通过（文本层面）**（跨节点 target/终态/Node 身份/出站代理/代理根 rights、Faulted 恢复出口、W6 保留帧、GPU 帧池、加速器 CPU Segment 销毁、task_create kind 出口、W1 外来 ID、creator/Timer 等）** → SPEC-055 登记 9 项 → SPEC-056 修订（SPEC-057 复核推翻，注销其“全部 Fixed”）→ **SPEC-058 统一处置全部 19 项 Open** → **SPEC-059 多轮 harness 内独立复核** → **SPEC-137（2026-10-17）范畴外复核新发现的 5 项未登记问题（DEV-008/WAIT-001/LIFE-008/S0X2-001/AUDIT-001）已登记并修订** → **SPEC-139（2026-10-17）组织级阶段 0 审查又发现 4 项未登记问题（CAP-003/LIFE-009/EXC-013/TASK-004）并已修订** → **SPEC-140（2026-10-17）对抗式复核再发现 12 项未登记问题（W16.1/DMA、非 CPU 运行期、跨节点、订阅撤销、创建初始化、S0-X2 错误出口、设备权限类型、GPU Segment 销毁、审计事件、文档口径）并已修订** → **SPEC-141（2026-10-17）第二轮组织级复核又发现 11 项未登记问题（`task_create` 代码 Segment 存活性、`schedule` SMP、Node/RemoteProxy 状态、`attach_remote_task` 前置、跨节点非 CPU 目标、W15 订阅授权、W16.1 `frame_paddr`、`dev_irq_mask/unmask` 分类、`memseg_map`/`MountTargetDead`/Bootstrap 错误语义）并已修订**。字面条件为“已知空洞均已记录且关闭”，但**组织级独立性门槛未跨过**（本轮审查与整改仍为 harness 内会话），故不勾选，待下一轮范畴外复核 |
| S0-X2 | 每项操作具有明确成功与失败语义 | [x] | **SPEC-142 逐项复核（2026-10-17）未通过 → SPEC-143/144 整改 → SPEC-145 冻结基线复核通过（文本层面）**：`task_create(kind=RemoteProxy)`、10 项 Capability 前置/错误清单、`AlreadyBooted`/`RightsExceeded`/`OutOfBounds`、`gpu_memseg_create` 可达性均已修；§19 现行 **47 项**（新增 `node_attach`） → SPEC-054：旧口径 26 项；[SPEC-134](docs/superpowers/reports/2026-10-17-s0x2-coverage-increment-doc007.md)：DOC-007 新增 8 项；SPEC-137 补入 `gpu_memseg_create` 后旧口径 35 项。**SPEC-139 更正：以 §19 为权威清单，现行常规操作面为 46 项（不含 Bootstrap），旧 26/34/35 口径作废**；并修复 `wait_objects` 长度错误、`ns_resolve` 错误清单缺口、显式清单标准错误包含与 MMIO 零长度/溢出。**SPEC-140：修复本轮发现的错误出口缺口（`memseg_map` 空访问/缺权、`dev_dma_create` 零页、`ns_resolve` 缺权、`dev_mmio_unmap` 幻影错误）并补齐错误定义。** **SPEC-141：`task_create` 代码 Segment 存活性、`memseg_map` `InvalidAccess` 范围、`MountTargetDead` 定义、Bootstrap 失败语义、`dev_irq_mask/unmask` 细化层分类已修；46 项计数与分类见规范 §19/§15.7。SPEC-143：29 项已逐项整改；SPEC-144 残修 `node_attach` 3 项；新增 `node_attach` 后 §19 为 **47 项**，计数与分类见规范 §19/§15.7。** 文本缺口已修，但尚未完成逐项复核，故暂不勾选；均属规范文本证据，不代表实现或证明 |
| S0-X3 | 形式化工具能够表达并执行状态与迁移 | [x] | SPEC-019 |


阶段 0 由 SPEC-142 复核、SPEC-143/144 整改与 SPEC-145 冻结复核签署；文本结论不覆盖实现或形式化证明。

## 阶段 1 — 启动与最小平台层

**状态：已完成（2026-10-17，SPEC-146）**

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

**状态：已完成（2026-10-17，SPEC-147）**


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

**状态：已完成（2026-10-17，SPEC-148）**

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

**状态：已完成（2026-10-17，SPEC-149）** S4-D1 Runtime API/版本协商（SPEC-084/085），S4-D2 ring3 服务管理器（SPEC-103–105、109），S4-D3 三个有状态基础服务（SPEC-106、110–112），S4-D4 用户态驱动/受控 I/O（SPEC-113/114），S4-D5 及 X1–X4 的收口证据见 [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md)。


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

**状态：进行中（BLK-004 C Task 容量门槛已关闭；片 C 的最小 `write(1/2)`+`_exit` 纵向闭环已获 QEMU 见证，见[tty 字节流报告](docs/superpowers/reports/2026-10-18-stage5-c-tty-write.md)；`PT_LOAD` 写段/BSS 私有装载与 W^X 已完成，见[装载报告](docs/superpowers/reports/2026-10-18-stage5-ptload-wx.md)；musl M0 依赖盘点已完成，见[盘点报告](docs/superpowers/reports/2026-10-18-stage5-musl-m0-inventory.md)，发现 P5-LIB-01 符号清单缺口待 profile v1；M2 系统边界已完成，见[M2 报告](docs/superpowers/reports/2026-10-18-stage5-musl-m2-boundary.md)，per-task FS base、C 适配层调用集（含 `exit(60)` 与 `__syscall4..6` 全参数透传）已见证；M3 静态 musl 基座与 M3b 匿名内存已完成，见[M3 报告](docs/superpowers/reports/2026-10-18-stage5-musl-m3-base.md)，SSE/FPU 契约、补丁重放、`__libc_start_main` 与 `malloc/free/realloc` 已见证；M4 文件首切片已完成，见[M4 报告](docs/superpowers/reports/2026-10-18-stage5-musl-m4-file.md)，单文件 `f` 的 fd 生命周期、`openat/read/write/lseek/close` 冻结子集、Namespace `f` 授权与 `printf`+`fflush` 输出已获 Linux/Prima 同源差分与双轮 QEMU 见证；M3b 的匿名 `mmap` 是用户态 arena 的 partial 适配（M9 起 96 页）；M4 的单文件后端已由 M8 推进为用户态 VFS/rootfs，但仍非通用文件系统；M5 首切片已完成，见[M5 sbase echo 报告](docs/superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md)，sbase `echo` E1–E6 在 Prima 双轮通过且与 Linux 参考逐字节一致，S5-X3 证据已具备、待组织级复核；M6 时间首切片已完成，见[M6 报告](docs/superpowers/reports/2026-10-19-stage5-m6-time.md)，`CLOCK_MONOTONIC` 与轮询睡眠在 Linux/Prima 双轮逐字节一致，`P5-TIME-01` 为 partial；M7 fd 表切片已完成，见[M7 报告](docs/superpowers/reports/2026-10-19-stage5-m7-fd-table.md)，本地 fd 3..10、`dup`/`dup2`/`dup3` 共享 offset 与 `getpid` 双轮 QEMU 通过，`P5-FD-01`/`P5-PROC-02` 仍为 partial；M8 用户态 VFS/rootfs 切片已完成，见[M8 报告](docs/superpowers/reports/2026-10-20-stage5-m8-user-vfs.md)与[M8 设计](docs/superpowers/specs/2026-10-20-stage5-user-vfs-design.md)，静态 rootfs、多级路径、目录枚举、通用 fd 0..18 与 `getdents64` 已获 Linux/Prima 同源差分与 V1 双轮 QEMU 见证，`P5-FS-01`/`P5-FS-02`/`P5-FS-03`/`P5-FD-01` 仍为 partial，profile v1 delta 待评审）；M9 元数据/工作目录切片已完成，见[M9 报告](docs/superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md)，`statx`/`chdir`/`getcwd` 与 sbase `cat`/`ls` 已获 Linux/Prima 同源差分与双轮 QEMU 见证，`P5-FS-04`/`P5-FS-05` 为 partial；M10 匿名内存/保护切片已完成，见[M10 报告](docs/superpowers/reports/2026-10-21-stage5-m10-anon-memory.md)与[M10 设计](docs/superpowers/specs/2026-10-21-stage5-m10-anon-memory-design.md)，内核 `memseg_protect`（号 41）与 C 侧 `mmap/munmap/mprotect` 已获 host 单测、A1 同源差分与双轮 QEMU 见证，`P5-MM-01` 为 partial；M11 文件树变更切片已完成，见[M11 报告](docs/superpowers/reports/2026-10-22-stage5-m11-file-mutation.md)与[M11 设计](docs/superpowers/specs/2026-10-22-stage5-m11-file-mutation-design.md)，`mkdir`/`rmdir`/`unlink`（含 `*at`）与删除槽位复用已获 host 单测、D1 同源差分与双轮 QEMU 见证，`P5-FS-06` 为 partial；[阶段 1–4 实现复核](docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md)与[阶段 5 修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md)已登记；BLK-003 仍 Open；ISSUE-IPC-001 已由阶段 2 签署后整改关闭。DOC-002 是外部候选、未经内核侧评审、非权威约束。**

**后续执行（2026-10-22）**：M11 之后的阶段 5 余项按[收尾路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)的 M12–M21 执行；阶段 6 入口为 S6-A–S6-E。该路线图是计划，不改变本页勾选状态；BLK-003 仍 Open。M12a 已落地参数块/auxv 与固定探针 envp 证据，M12b 已落地退出自动关闭、存储句柄计数、`getppid` 与 `O_CLOEXEC` 记录（F1）；独立参数载体的启动回归登记为 ISSUE-START-ARGS-001。M13a 已落地符号链接/`readlink`、`rename`/`renameat`、`fchdir` 与内容区压实（V2 双轮与同源差分，见[M13a 报告](docs/superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md)）；M13b 已落地节点 mode 位、`chmod`/`fchmod`/`fchmodat`、进程级 `umask`、`readv` 与 stdio 文件 API（V3 双轮与同源差分，见[M13b 报告](docs/superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md)）。M19 已落地用户态 loopback IPv4/UDP 子集（socket/bind/sendto/recvfrom/getsockname/close，N1 双轮与同源差分，见[M19 报告](docs/superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md)）；NIC/TCP/DNS 仍属阶段 7。M14 已落地 fd 0 的 tty 输入、canonical/raw、`Ctrl-D` EOF、termios 子集与 QEMU 串口注入（I1 双轮，见[M14 报告](docs/superpowers/reports/2026-10-23-stage5-m14-tty-input.md)）；有读者时的 COM1 轮询待 M15 WaitObject/IRQ 替换。M15 的 Process/Thread/ShareGroup 与 WaitObject 设计已提出（D3 提案，[设计](docs/superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)）。M15a-1 已落地 WaitObject 的 create/observe/signal/destroy 与 `wait_objects` 立即路径（42–46，host 3 用例 + W1 双轮，见[M15a-1 报告](docs/superpowers/reports/2026-10-23-stage5-m15a-waitobject.md)）；M15a-2 已接线 BlockedWait/waiters/signal·destroy·超时唤醒与结果注入（host 12 用例，W1 保留立即路径，阻塞的 QEMU 端到端待 M15b，见[M15a-2 报告](docs/superpowers/reports/2026-10-23-stage5-m15a2-waitobject-blocking.md)）。M15b-1 已落地 Process/Thread 元数据（`task_create` 建单线程 Process、销毁摘除、位图线程表示，host 用例，见[M15b-1 报告](docs/superpowers/reports/2026-10-23-stage5-m15b1-process-metadata.md)）；M15b-2a 以“同 Process 共享 leader 能力空间”的工作决定接线 `thread_create`(47)、自毁 cap 与共享 root/独立内核栈（host 用例，见[M15b-2a 报告](docs/superpowers/reports/2026-10-23-stage5-m15b2a-thread-create.md)）；M15b-2b 以**线程专用惰性 Dead 槽位回收**（优先无 cap 指向的记录，否则撤销 cap 后复用）关闭 ISSUE-THREAD-CAPACITY-001：W2 双轮通过（主线程 BlockedWait 被共享线程 signal 唤醒、线程自毁、容量见证仍 12/132），见[M15b-2b 报告](docs/superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md)。M16a 已落地 futex **WAIT/WAKE** 子集（适配层地址表 + WaitObject，Linux 202 翻译，W3 双轮，见[M16a 报告](docs/superpowers/reports/2026-10-23-stage5-m16a-futex.md)）；M16b-1 已加入内核 `SYS_CLONE(48)`/`SYS_THREAD_EXIT(49)`、clone 帧复制与“线程退出不拆共享 root”修复（见[M16b-1 报告](docs/superpowers/reports/2026-10-23-stage5-m16b1-clone-basis.md)）。M16b-2 已给 musl 快照打 `clone.s` 补丁（裸 syscall 56→Prima 48、子线程退出 60→49），内核 `SYS_CLONE` 按 Linux 参数序接收并写 `parent_tid`，适配层 PROT_NONE mmap 改为 RW（见[M16b-2 报告](docs/superpowers/reports/2026-10-23-stage5-m16b2-musl-clone.md)）。M16b-3 实现 `CLONE_CHILD_CLEARTID` 语义（clone 写 child_tid、退出清零并 futex 唤醒）后，`pthread_create`/`pthread_join` 在 QEMU 与 Linux 参考逐字节一致，共享内存可见性最小探针也通过（W4/W5 双轮，见[M16b-3 报告](docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)）；pthread 取消/分离/属性未验证。M13 的冻结 VFS 子集除硬链接与跨启动持久化外收口；硬链接/持久化仍标未实现。

### 交付检查

- [ ] S5-D1：冻结且版本化的 POSIX profile。**v0 已冻结（2026-10-18），等待组织级确认后勾选。**
- [ ] S5-D2：libc 适配（候选分解 M01–M12：`abi`+IDL 生成器、`ipc`、桩层双模板、crt/`__libc_start_main`、musl 五个边界替换点、`ld.so`、`fd↔capability`、线程/TLS、信号、`fork`/`execve`/`wait4`、tty/console）。**M12–M20 的余项拆解见[路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)。**
- [ ] S5-D3：文件描述符、路径、线程和基础网络子集。**M4/M7/M8/M9/M10/M11 已覆盖 fd、多级路径、目录枚举、`stat`/cwd、匿名内存保护与文件树变更子集；线程与网络仍未实现，故不勾选。** 线程/网络/tty 输入/VFS 完成分别见路线图 M15–M19。
- [ ] S5-D4：逐 API 语义映射矩阵。**M20 汇总，逐里程碑提供差分证据。**
- [ ] S5-D5：与参考 Linux 的差分测试矩阵。**每个里程碑新增同源探针，M20 汇总。**
- [ ] S5-D6：Rust 目标与 `core`/`std` 移植（候选规格 §5.10 / 里程碑 M7）。**目标/范围由路线图 M20 与决策门 D6 冻结。**
- [ ] S5-D7：兼容性护栏——persona 字段、符号版本脚本、`compat-state.md`、符号↔syscall 追溯表、ABI 版本握手（候选 `NT_RUXOS_ABI` 名称待评审）。**M12/M20 承接，仍待裁决。**

### 上游前置（内核侧 ABI 承诺，须先评审确认 —— 见 BLK-003）

下表把候选规格消费的内核语义映射到本仓库现状。**这些不是阶段 4 的交付项**，但进入阶段 5 前必须逐项确认归属；其中号段/命名对齐构成决策，不得静默改写（本仓库号段“只追加、空洞不复用”）。

| 前置 | 候选规格要求 | 本仓库现状 | 归属 |
|---|---|---|---|
| 直接 IPC 快路径 | 小消息往返 ≤250 ns 依赖直接 IPC（决策 D-09） | 同步 `call/reply` 快路径及 ≤250 ns 指标未实现；排队和带 Cap 的 `BlockedRecv` 直投已通过有限见证（ISSUE-IPC-001） | 阶段 4/5 |
| futex / wait_multi 内核化 | 决策 D-02 | 未实现 | 阶段 4/5 |
| 基础信号 | 规格 §4.5.1 结构 + 号段 `0x03A–0x03D`（`sigaction`/`kill`/`sigreturn`/`abort`） | 抽象层已有 `exc_subscribe`/`exc_unsubscribe`（COSIR §12）与 `exc_resume`；用户可调仅 `SYS_TRAP`/`SYS_EXC_WAIT`（SPEC-052） | 阶段 5 |
| `fork`/`execve`/`wait4` | 规格 §5.11（V1 仅单线程 fork，多线程返 `EAGAIN`，决策 D-13） | 未实现 | 阶段 5 |
| tty / console 通路 | 规格 §5.12 | 片 C 已实现 fd 1/2 到 COM1 的 ring3 字节流服务（`prima-tty`）与受控端口自检；无行规程、无通用 fd/console API | 阶段 4/5 |
| 匿名页池与 `mmap` flag、W^X | 规格 §4.14（W^X，决策 D-18） | 未实现；在册“image 段 `{R,X}` 与可写 `.data`/`.bss` 冲突”仍待链接脚本级段切分 | 阶段 4/5 |
| 号段 / 命名对齐 | `0x0xx` 号段、`NT_RUXOS_ABI`、`x86_64-ruxos-none` | `prima-abi` 的 `SYS_*` 编号（1–41 已占用：37 为原生 ABI 握手，38–41 为 M2/M6/M10 追加；后续只从 42 起追加） | 余项待决策 |

**候选里程碑（规格 §19，提案，未纳入本仓库排期）**：M1 ABI 基座 → M2 裸通路 → M3 IPC 通路 → M4 libc 主体 → M5 动态链接 → M6 线程/TLS/基础信号 → M7 Rust 基座 → M8 韧性 → M9 出口；M1/M3/M4 可三线并行、M5/M6 须串行；规格侧估算约 22 周 / 26 人周。

### 候选规格的待确认风险（来自其走查/扫雷记录，随本阶段一并跟踪）

- RED（规格内已回写闭环，落地仍需本仓库评审）：符号↔syscall 追溯表、匿名页池/W^X、ABI 握手、信号结构冻结、`fork`/`exec`、tty 通路、libm 归类。
- YELLOW（规格待确认）：时延目标按 `(qemu, qemu-cpu, hostcpu)` 三元组记 `PROVISIONAL`、`getrandom` 非阻塞语义、`poll/select` 退化实现标 PARTIAL、`sys_features` 位图、`dtable.cookie`、CI 时长、静态链接的 ABI 校验、错误码有损映射（`RUX_KERR_VERBOSE`）。
- GREEN（规格明确推迟 V2）：`dlmopen`、`DT_RELR`/IFUNC、静态 PIE、locale/宽字符、aarch64/riscv、多 itss 热备、LTP/PISA、形式化验证。

### 退出条件

- [ ] S5-X1：profile 内每项 API 都有语义映射和可重复测试。
- [ ] S5-X2：每项 API 明确标记 supported / partial / unsupported。
- [ ] S5-X3：选定开源程序可重新编译并通过冻结的功能测试集。**证据已具备（2026-10-19）：sbase `echo` E1–E6 双轮 QEMU PASS，见[M5 报告](docs/superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md)；待组织级复核后勾选。**

## 阶段 6 — Linux x86_64 静态 ELF

**状态：未开始**

### 交付检查

- [ ] S6-D1：静态 ELF 加载器与 Linux 初始栈。
- [ ] S6-D2：OS 中立的 syscall / exception 递送机制。
- [ ] S6-D3：版本化的 Linux LibOS syscall profile。
- [ ] S6-D4：所选 profile 必需的 futex、TLS 和进程环境。
- [ ] S6-D5：未修改静态 ELF 白名单与测试镜像。

### 退出条件

- [ ] S6-X1：白名单中的静态 ELF 无需修改即可运行。
- [ ] S6-X2：支持的 syscall 与参考 Linux 通过差分测试。
- [ ] S6-X3：HexArc / COSIR 不包含 Linux 专属对象或 syscall 语义。
- [ ] S6-X4：未支持 syscall 得到确定且可审计的失败。

> 注：候选规格（DOC-002）把 `ld.so` 最小动态链接闭环列为 P0、随阶段 5 交付，而白皮书把“动态链接”列为阶段 7 候选。两者的阶段归属须一并裁决，不得两处各写一套。

## 阶段 7 — 扩展与硬化

**状态：未开始；范围未冻结**

候选工作不预先视为承诺：

- [ ] SMP；
- [ ] IOMMU 与更完整的用户态驱动；
- [ ] 动态链接；
- [ ] 资源计量与配额；
- [ ] 选择性撤销；
- [ ] 更强的故障恢复；
- [ ] 第二硬件架构。

只有在阶段 0–6 的证据揭示实际优先级后，才把候选项拆成带退出条件的独立里程碑。

## 阶段晋级规则

1. 阶段状态由该阶段的退出条件决定，不由提交数量、代码行数或主观完成度决定；
2. 前一阶段未满足退出条件时，后一阶段可以做探索原型，但不能成为“当前已完成阶段”；
3. 失败测试、反例和实现偏差应写入对应报告；仍影响当前阶段的事项进入上方问题表；
4. 关闭阶段时，在签署报告中记录完整命令、工件版本和结果，并从证据索引链接该报告；
5. 架构或语义变化必须先更新权威文档，再重新评估受影响的已关闭检查项。
