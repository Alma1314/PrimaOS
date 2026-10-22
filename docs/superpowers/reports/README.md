# Reports 索引

本目录保存**实现报告与审查/复核证据**：记录实际命令、结果、计数、反例和证据边界。历史报告保留其写作时的结论，后续复核可能改变适用范围。

> 本索引按阶段/主题分组，组内按日期倒序。新增文档请放到本目录，并同步更新本页与 [`docs/README.md`](../../README.md)。

## 阶段 5

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-22 | [2026-10-22-stage5-m11-file-mutation.md](2026-10-22-stage5-m11-file-mutation.md) | 阶段 5 M11：文件树变更（`mkdir`/`rmdir`/`unlink`） |
| 2026-10-22 | [2026-10-22-stage5-m12-c-startup.md](2026-10-22-stage5-m12-c-startup.md) | 阶段 5 M12：C 启动栈与退出清理（envp/auxv、自动关闭 fd） |
| 2026-10-22 | [2026-10-22-stage5-m13a-vfs-links.md](2026-10-22-stage5-m13a-vfs-links.md) | 阶段 5 M13a：符号链接/readlink/rename/fchdir、内容区压实 |
| 2026-10-22 | [2026-10-22-stage5-m13b-permissions-stdio.md](2026-10-22-stage5-m13b-permissions-stdio.md) | 阶段 5 M13b：节点 mode、chmod/fchmod/umask、readv 与 stdio 文件 API |
| 2026-10-23 | [2026-10-23-stage5-m19-loopback-udp.md](2026-10-23-stage5-m19-loopback-udp.md) | 阶段 5 M19：用户态 loopback IPv4/UDP、socket/端口/数据报 |
| 2026-10-23 | [2026-10-23-stage5-m14-tty-input.md](2026-10-23-stage5-m14-tty-input.md) | 阶段 5 M14：fd 0 tty 输入、canonical/raw、termios 子集 |
| 2026-10-23 | [2026-10-23-stage5-m15a-waitobject.md](2026-10-23-stage5-m15a-waitobject.md) | 阶段 5 M15a-1：WaitObject 创建/观察/信号/立即等待/销毁 |
| 2026-10-23 | [2026-10-23-stage5-m15a2-waitobject-blocking.md](2026-10-23-stage5-m15a2-waitobject-blocking.md) | 阶段 5 M15a-2：BlockedWait/唤醒/超时/结果注入 |
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

## 阶段 4

| 日期 | 文档 | 标题 |
|---|---|---|
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
| 2026-10-14 | [2026-10-14-dual-personality-handoff.md](2026-10-14-dual-personality-handoff.md) | 双人格工作流交接与上下文压缩 |

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
