---
id: report-stage0-5-gap-followup
kind: report
authority: scoped-work-record
status: validated-pending-scope-review
lifecycle: historical
updated: 2026-10-27
---

# 阶段 0–5 缺口后继工作记录与统一验收

## 范围和当前结论

基线 `107fd2ce263dbc17e899832d0ae14b367fc6e8bc`，开始时工作树干净。
用户要求逐项解决阶段 5 及以前缺口，并明确要求**全部缺口解决后再统一测试**。
前15节保留先实现、未测试时的工作记录；第16节起记录用户选定D6 A后的统一验收与失败修复。
下表为当前工作树的实际检验范围，统一技术验收已完成；有界profile和阶段出口仍待最终评审。
前批运行结果仍只适用于[前批报告](2026-10-25-stage5-gap-closure-report.md)的版本。
当前状态仍查[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

## 最新检验快照（2026-10-27）

| 检查 | 本轮实际结果 | 范围 |
|---|---|---|
| workspace | 1016项PASS | kernel487、xtask211；审计隔离及I1/N1时序不变量反例 |
| lint | PASS | fmt、Clippy、裸机构建、kernel/src零unsafe（32文件） |
| Rust两种Clippy | PASS | 覆盖层指纹生效后重新构建；core/alloc及单线程std |
| Lean lake build | PASS，10 jobs | 位置保持性及既有CDT定理；未扩大为完整机器细化 |
| Miri | authority模型21项PASS | 全包在旧COW大页表夹具耗时后手动停止，不记全包通过；固定nightly、独立产物目录 |
| musl | 锁定补丁重建/归档审计PASS，host 5项PASS | 不复用其它工作树的旧缓存 |
| 聚焦机器验收 | W4/W6/W7b/W9/R2干净双轮PASS；W7/R1已有本轮双轮PASS | 具体失败与修复链见第16–17节 |
| 默认启动 | 完整命令双轮PASS，caps_live=134 | IPC 51/123/132、caps_peak=146/192、audit=1158/16384，逐字段固定 |
| 完整BIOS | 最后单次命令68/68轮PASS，20项Linux参考PASS | 两次67/68失败及I1/N1验收修订见第17–19节 |
| compat | PASS cow-write-fault | 原基线不变；不是完整Linux兼容性证明 |
| 最终文档 | self-test/build/check/check --base及git diff --check全部PASS | 316篇文档；基线107fd2ce；阶段5保持未签署 |

TCB为trusted_lines=2072、mechanism_lines=30158、ratio=6.43%，unsafe_tokens为78/0，
内核服务耦合include_bytes=0、prima_user_dep=false、service_name_lines=0、protocol_lines=49。
这些是固定统计口径的测量，不是内存安全或性能证明。

## 本次按顺序实现的修改（各节保留当时状态）

### 1. Capability 位置、载体与溯源

[工作契约](../specs/2026-10-25-capability-location-model-design.md)对应 `cosir::authority`。
新增独立 `locations`、消息/FIFO/阻塞接收结果、Namespace Bound 载体和相关审计事件。
用户派生校验持有、存活、Derive、权限子集及类型；drop 保留记录与后代。
消息排队/直投/接收、bind/unbind/resolve、容器销毁同时更新位置与审计。
失去最后一个 ExcSubscribe token 时终止订阅。只读溯源沿保留父链返回相关事件。

异常恢复与 COW 原有授权入口改为查询实际位置；创建路径登记 HeldBy，
`owner` 保留为创建时投影，不作为当前位置或授权回退。CDT 显式 ID helper 仍是内部对拍原语。
新增位置/载体不变量和按事件重放的位置溯源检查，补充失败原子性、stale token、
重复携带、结构根、ID 耗尽、FIFO、直投、容器回收及旧授权入口的回归代码。

现覆盖本地Task、消息、Bound与Mount；Mount包含循环预检和完整解析链审计。Timer、跨节点与完整异常通知仍没有整体host模型。
新 Rust 状态面没有同步扩充 Lean；原 CDT 证明范围保持不变。ISSUE-CAP-001 不在未测试时关闭。

### 2. 每 Task 下界保护页

[进程/栈契约](../specs/2026-10-25-process-boundary-followup-design.md)对应 `kernel::user`。
为 18 个 Task 中断栈各保留页对齐的下界 guard，可用栈仍32 KiB。
根构建跳过全部 guard，栈地址使用裸地址计算；现有 `stack_guard` 检查扩大到 syscall 栈和全部 Task 栈。
新增布局回归代码。尚未取得本版本 QEMU 运行页表证据，也未证明全部路径峰值。

### 3. 信号终止的 wait 编码

Process/等待结果改用显式 `Exited(u8)` / `Signaled(u8)`，立即及延迟写回共用四字节编码。
普通 exit 保留低八位，信号终止写低七位；不再把 shell 的 `128+signal` 编成普通退出。
W8 改为严格检查 WIFSIGNALED/WTERMSIG，并拒绝 WIFEXITED。
这没有补齐 libc sigaction/mask/EINTR/SA_RESTART/SIGCHLD 等其它信号义务。

### 4. W7/W7b Linux 参考路径

共用 `stage5_exec_fixture.h`，Linux 参考通过 memfd/fexecve 执行，目标仍走 exec55。
ELF 仅退出 syscall 编号按平台选择，argc/截断镜像断言相同。
W7b 的 Linux 参考从临时普通文件读取后执行；目标端继续使用父预读 `/tiny.elf`。
两个参考均接入全量与聚焦驱动，stdout/退出码标准不放松。
仅作语法编译、尚未运行，不能据此关闭 S5-D5；完整 libc execve/子 VFS/CLOEXEC 仍开放。

### 5. 用户选定A后的fork与来源隔离

用户已选择[受控继承例外A](../specs/2026-10-25-fork-authority-decision.md)。kernel新增独立进程局部句柄视图，
完整复制槽代次/墓碑、克隆held token、仅重绑定官方self cap；父子drop互不删除对方token，普通derive仍要求Derive。
cosir有对应受控继承操作；两侧均保留CDT和溯源。Task销毁只删除入站消息及自持token，外部后代和出站消息保留。
新增59区分Process id和Task id；父退出清理孤儿引用。新增60提供内核认证的来源Task/代次/msg_id，旧18格式保留。
Task目标cap固定代次，编号复用不使旧目标复活。整组退出、远端61、延迟释放队列与信号槽清理已写入，尚未统一测试。

### 6. exec失败原子性与帧回收

新映像独立准备后提交，失败释放新根/表/Segment而保留旧映像，逐页copy-in不直接解引用用户指针。
55无envp兼容保留，新增62允许显式envp；补静态ELF地址、段重叠、入口、栈容量及16字节对齐校验。
逻辑映射登记新映像，私有Segment按最后映射回收，Segment销毁补回收后续COW帧。
仍需接完整libc execve、适配层持久状态、子VFS/OFD及CLOEXEC，不声称S5-D5关闭。

### 7. VFS/OFD会话与fork事务

新增认证会话、直接存储IPC、每Task分片暂存、共享OFD的Process holder位图和fork预留。
客户先核对管理器认证路由，再核对存储回复来源和发送cap目标；服务端不信任payload里的客户编号。
W6增加普通文件共享游标、子自行open和父先close的回归断言，Linux参考使用临时文件；尚未运行。

静态检查发现“内核发布后再创建服务回复凭据”仍可能失败，因此C/musl入口改为64准备/交接/发布；
Prepared子不可调度或wait，服务端确认后才发布，失败以预留审计预算取消。
父退出、ticket丢弃/容器回收及外部委托ticket退役都进入共同取消路径；保留外部后代。
正常COW写入不被取消回滚覆盖。原生51继续保留立即机制，不能把工作事务当作已有机器证据。

补充线程注册失败的成员摘除/新self根回滚，以及musl出口清理；
MAP改为整区间表帧预留，底层失败批次归还未链接帧；共享VM逻辑归leader，输入区间和用户写回按页处理。
存储状态6页、服务栈2/4页和vspace1024的容量调整均待机器资源验收。
Rust工作区编译检查和C适配层/探针的语法编译已执行；测试代码只写入、尚未运行。
C语法编译一度因命令重复定义PRIMA_ABI_NO_IOVEC触发-Werror，去除重复命令定义后两平台语法编译通过；不是功能测试。

### 8. libc信号与线程生命周期后继

新增通用upcall65，Process共享动作、Task私有掩码/来源/pending/嵌套GPR与FPU恢复栈。
C边界接sigaction、mask/pending、sigsuspend/pause和siginfo/ucontext，内核提供EINTR与受控重启、子退出及硬件故障路由。
#DE/#UD/#SS/#GP/x87/对齐/SIMD入口按是否有error code解码；#PF在COW失败后进入配置的故障路由。
返回现场校验token、用户PC/SP/CS/SS、flags与MXCSR。通知限制在同fork家族或显式TaskDestroy授权，防止裸PID攻击无关服务。

66接分离线程内核栈上的unmap/exit，C分配器在代次消失后复用范围；官方线程self根回收不撤销外部后代。
67原子比较用户word并登记等待；每Task独立WaitObject实现有数量上限的futex wake，退出清理对象。
扩容曾触发BootstrapState编译期13KiB守卫，现将WaitObject表移到既有arena而保留守卫，生产编译检查随后通过。
68提供保留帧/COW的用户访问禁用，替代PROT_NONE近似；fork保存隐藏及只读共享叶，mprotect恢复W继续走COW。
W4/W9的同源C回归已扩展，只有语法编译，尚未运行。完整IPC取消/关联、TTY等待、exec跨映像和std OS后端仍需继续。

### 9. execve/path/envp/CLOEXEC后继

已写入69通用交接、保留范围/帧/COW与显式Segment接管；新栈8页，提交同步Task入口与clear_tid。
C适配层在子内从VFS读取有执行权限的静态ELF，交接后由独立RX代码关闭CLOEXEC，再进入任意新ELF。
新Prima crt另负责恢复fd/cwd/umask/termios/会话并清新libc heap。失败不改旧fd；交接Segment由内核管理最后映射回收。
W7b改为实际libc execve(path)，紧凑ELF检查envp、实际关闭的OFD及保留OFD，ENOEXEC负例检查旧fd继续可读。
Linux与Prima的探针和过渡汇编均只编译，没有执行；完整C新映像crt恢复仍需增加/验收机器见证。
owner账本外置容量扩至1024，保留13KiB BootstrapState守卫；149页SegmentFrames上限未放开。

### 10. TTY IRQ、取消事务与物理期限（未验收）

已写入70/71、IRQ Device/可委托源、PIC递送和pending重试；源订阅从根句柄依赖改为实际持有位置授权。
通用硬件/页故障/syscall入口清DF；timer入口改为完整保存RSI后传递硬件栈地址，避免覆盖用户RSI。
管理器改为COM1 IRQ4加有界待读FIFO，支持来源cookie/序号、取消确认、已提交回复确认、输入背压及死亡清理。
C接收层暂存嵌套handler收到的外层TTY回复，显式处理重启；短fd/VFS事务屏蔽回调并在Process锁内完成。
注册第二条受限控制cap后，Manager以明确授权向待读Task发送Ctrl-C/Ctrl-\通知；无进程组/job control扩展。
72提供物理期限阻塞；67对齐40的物理时间，44仍用抽象时间。单CPU时钟改为PIT校准时间戳，IRQ0唤醒物理期限。
普通睡眠不再忙轮询，EINTR写回相对rem；最后Ready任务阻塞/退出后进入可中断idle，释放根仍等待切换CR3。
I1/W9及纯状态回归已增加，未运行。SEND的QueueFull映射改为EAGAIN，与AuditFull的ENOSPC分离。
管理器单独裸机编译生成汇编：入口帧为10392B，既有栈为16KiB；这是静态局部检查，不是全调用链峰值证明。网络事务、完整exec新crt、Rust OS后端/范围与整体模型仍待继续。
fork/exec遇活动TTY调用栈暂返回EAGAIN；siglongjmp、可变控制字符与完整前台会话仍无新增支持承诺。

## 后续依赖顺序

继续以阶段跟踪的活跃表为输入，不将本表当作第二份状态源：

1. 完成能力继承与 OFD holder 的根因设计/实现：独立进程 cap 视图、失败回滚、退出回收；
2. 以该隔离为基础接子进程 VFS 会话、execve/envp、CLOEXEC 与描述继承；
3. 补信号/线程生命周期与 libc 表面，处理掩码、通知、等待中断和重启；
4. 接 TTY 的硬件中断等待，取代读请求期间的 COM1 忙轮询；
5. 扩展 Rust std OS 后端并明确 D6 支持边界；
6. 整理 profile/ABI/persona/符号版本及开源程序的具体评审工件；
7. 全部代码修改完成后统一验收，并在失败时修复后重新执行受影响的整套验收。

组织级决定依旧由[路线图决策门](../plans/2026-10-22-stage5-completion-roadmap.md#3-决策门)
和 M21 限定；新增工作契约不代替冻结/批准。已接受的 AUTH/COW 细化边界不当成新缺陷抹除。

## 验证状态

本次执行了 `cargo fmt --all` 和仅编译的 `CARGO_NET_OFFLINE=true cargo check --workspace`。
过程中发现并修复语法/类型问题；一次扩大替换误删相邻方法，已从基线精确恢复并重新应用预期修改，后续workspace编译检查通过。
这些仅为开发期编译结果；没有运行Rust/host测试、Clippy/lint、QEMU、Linux参考、Lean、Miri、compat或文档校验。
遵照用户安排，在全部缺口代码补齐后再统一验收。
目录生成仅用于维护导航，不构成校验通过。
最终验收仍需仓库要求的 workspace tests、lint、全量 BIOS 双轮、musl host、
Rust 双模式、compat、TCB 与文档 self-test/build/check/check --base。
历史 QEMU 超时根因尚未定位；不能用这些尚未运行的改动解释旧失败。


### 11. Socket fork holder、事务隔离和取消

承接[M19历史报告](2026-10-23-stage5-m19-loopback-udp.md)，工作契约见[进程边界后继](../specs/2026-10-25-process-boundary-followup-design.md)。
新增纯`Network`表：Process holder保留fork对象，认证Task会话决定请求权限，逐Task发送暂存、等待reply cap和接收快照；
成功投递才提交队列，关闭保留活动引用，Task死亡/取消/FINISH释放。fork交接前增加子holder，失败与死亡回收。
C增加8位请求序号、来源认证、取消与完成握手、外层回复缓存及短事务掩码/Process fd锁。
N1新增fork关闭隔离、信号EINTR/取消后再次接收、SA_RESTART与嵌套stat；纯回归覆盖并发暂存/独立快照及代次。
管理器入口帧编译测得14584B，4页用户栈余量不足，内核和init同步扩为8页；未改变内核每Task栈或149页内联上限。
仅进行语法/编译检查，所有新回归和原M19/N1均未执行；不继承历史PASS，不关闭P5-NET-01或D5。


### 12. 新 C crt、连续exec和可回收 Segment 表槽

新增独立`stage5_exec_child.c`静态ET_EXEC（约70KiB）：完整Prima crt/musl启动，
检查TLS/errno、构造、argv/envp、fd/OFD/CLOEXEC、cwd/umask、信号保留/重置及分配；
再从新libc执行rootfs tiny ELF，检验第二次交接。Linux用同源C和memfd参照。
W7b路径紧凑ELF与完整C内存映像的证据范围分开，rootfs仍只有2920B内容区。
适配层共用`prima_exec_common`，恢复卸载全部3页交接，桥处理EAGAIN，预备arena授权。
Segment死亡槽可复用，ID严格递增，private exec位按实际槽；新增旧cap/ID耗尽回归代码。
聚合服务映像静态尺寸参考约150页，采用192页外置boot向量，普通149页内联预算不变；
启动解析同步校验。仅编译/语法检查及元数据尺寸测量，不构成新crt运行、资源峰值或回归PASS。


### 13. Rust 原生 std PAL 与 D6 裁决工件

新stdio/time/thread PAL链接原生Runtime hooks，stdout/stderr发送真实TTY字节，
Instant读100Hz物理单调时钟，sleep向上取整并在信号中断后续等绝对期限；yield让出调度。
保留显式GlobalAlloc、单线程TLS、无lang_start；System=NULL，stdin/spawn=Unsupported，wall-clock不支持。
构建stamp包含三个PAL文件hash，上游输入锁扩为九项；R2新增真正std输出、时钟/睡眠及负例。
仅执行离线build-std编译，尚未运行探针或Clippy。[D6裁决](../specs/2026-10-26-rust-d6-scope-decision.md)已提交用户选择，
未选择不当作冻结，不因编译成功关闭D6或阶段出口。


### 14. 事件/位置模型组合、Lean位置草稿与剩余ID

[组合契约](../specs/2026-10-26-event-authority-model-design.md)纳入Timer一次性通知、时间推进、异常fan-out与源生命周期、
WaitObject/upcall和远端纯数据handoff/代理/丢弃；MessageOrigin与共同入队/直投保留来源及位置一致性。
新增回归源码覆盖FIFO、取消/销毁/stale、直投、耗尽失败原子性、等待/alert/timeout及远端不携cap；没有运行。
内核TimerId改单调、MsgId末代后拒绝、通知批次预检与到期Timer审计/排队预算修正；#AC独立路由到SIGBUS。
Lean加入locations/独立carriers保持性草稿，UInt32和对象引用对齐；仍不证明完整对象/消息/事件状态机或Rust机器细化。
workspace与内核裸机仅编译检查；全部Rust回归、Lean构建及QEMU待统一执行。
源集合支持已写入；GPU/设备/页表细化及完整W1–W17不在本组合谓词的证明范围，不能因本片存在改写历史发现。


### 15. 逐入口台账、静态 persona/版本护栏与具体评审工件

新增[逐入口工作表](../compat/stage5-working-api.md)，覆盖实际C Linux分派与非分派libc/原生入口；
profile v2更新为当前partial边界，旧报告和旧版本PASS保留、不外推到本轮。
[静态护栏](../specs/2026-10-26-static-program-contract-design.md)给C/Rust产物加入PT_NOTE程序声明，
构建按persona/adapter检查；C exec提交前拒绝不匹配；crt强链接版本化锚点，版本脚本约束静态输出符号。
C/Rust解析器拒绝源码、旧锚点链接失败用例、W7b错误persona与fd保留、紧凑ELF声明和完整新crt均已接入，未运行。
只完成C语法/链接、Rust std编译和workspace编译检查，readelf观察到C/Rust各自声明；
C尺寸链接仍使用已有缓存musl，最终必须重建锁定补丁。没有动态GNU版本、内核persona或第二LibOS根视图。
[出口评审工件](../specs/2026-10-26-stage5-release-decisions.md)明确D1–D10选择、候选快路径/性能与动态链接边界。
D6用户选择仍未收到；不默选、不签署阶段、不启动统一测试或阶段6。

复核同步修正musl构建审计：clone.s不再裸发49，归档白名单从48/49+58收紧为48+58；
49/66在适配层清理后发出。新增旧裸49拒绝用例，未运行；不能继续用旧缓存的归档通过证据。


### 16. D6 选A后的统一验收（2026-10-26，进行中）

用户明确选择D6 A；此前“未统一测试”均为本节之前的实现版本记录，不覆盖下列实际执行。
2026-10-26本轮workspace全套host测试第7次命令通过，Lean lake build通过（10 jobs）；
musl host 5项及Linux F1通过，锁定musl八项补丁从独立本地快照重建、归档审计通过。
Rust core/alloc和restricted std两种Clippy均通过，workspace lint/裸机编译/内核零unsafe通过。
文档self-test/build/check/base通过一次；后续变更及最终报告仍须重新生成和检查。

初次host测试发现并修正测试导入、当前holder夹具、尚未创建/未运行Task夹具、
权限文本逗号和root目录只读夹具；扩大帧账本也暴露回收池仍640及夹具image区相交。
回收池改随owner/table预算计算，owner预算按611帧上界选640，保持FrameRegistry原32KiB守卫；
启动生成码的4→8页管理器栈改变了指纹，按实际代码更新，保留完整指纹断言。
Lean三处解析问题已修正，位置保持性模块与旧CDT定理均成功构建；这不扩展其证明范围。

首次完整BIOS命令在沙箱内执行：缺少sbase快照，已从既有锁定本地Git快照恢复；
Linux socket=EPERM、memfd exec=EACCES，沙箱外W7 Linux参考已通过。
首次BIOS默认轮因新增IRQ源/订阅导致旧计数2/3不符实际3/4；断言按真实新增机制同步。
T1在CPU异常入口panic；定位到IRQ0正常返回错误跳过硬件帧，已移除多余rsp推进，机器重跑待完成。
首次全量在失败收集期间中断，不是完整PASS；一次W7并发碰到测试镜像锁，之后串行运行。
W7仍失败：首次会话注册成功，但fork交接时cap派生返回ENOENT；原映射也用此码报告容量耗尽。
160槽构建实测caps_peak=153（另有内部预留槽），正在验证192槽配置及原13KiB结构体守卫。
临时DBG诊断仍在源码，最终必须移除并重跑，不能将诊断构建记为最终验收。

当前未完成：完整BIOS双轮（包含全部Linux参考）、fork/exec运行修复、compat、最终TCB与文档重跑。
Miri组件未安装，尚无Miri证据；不把其它检查等同Miri。所有历史超时/版本边界继续保留。


统一验收后继：192槽通过原BootstrapState 13KiB守卫并使fork会话交接成功；
此前两个Manager派生失败确为容量问题（旧derive_errno将CapacityExceeded也映射ENOENT）。
QEMU CPU异常日志定位到共享32KiB syscall栈保护页：exec输入缓冲与按值Segment复制叠加。
Arena增加只读投影以查询id/active/frame，exec的ELF头、参数与页复制临时缓冲分入不内联短生命周期函数，
分配Segment前退栈；保留32KiB及保护页。移除所有临时DBG后W7 Linux参考及BIOS双轮完整PASS。
本版本固定BIOS原始trace中client/协商token/service句柄分别为1d/38/3d，validator同步这些值并保留活对象及映射检查；
COM1 IRQ设备/source解释长期cap基线132→134，保持严格退出清理检查。
当前第二次完整命令的所有Linux参考已PASS，echo E1–E6、A2、T1、F1已双轮PASS；
T1通过确认IRQ0正常返回修复。完整命令尚未结束，不记整体PASS。
发现默认轮仍有两条stale会话cap留到最后接收循环之外，已补同一回收helper在Process销毁后立即调用，
其机器验收待后续重跑；W7b Prima夹具改在可写/tmp创建，避免与只读root契约冲突。

第二次完整命令已结束：所有20项Linux参考通过，默认启动两轮及W6/W9/W7b/W4/R2共7个组或检查失败，
其余程序通过两轮；不记完整套件PASS。W6仍向只读根写文件的探针夹具改到/tmp，待重跑。
W9的CPU日志显示非法指令处理后首次用户缺页转upcall时，iretq因SS=0与内核rsp触发内核GP；
页故障裸入口的CPL3分支现同步捕获真实用户rsp/ss，聚焦重跑已到fault=1，但后继匿名内存步骤超时。
W4取消等待和W7b第二次失败exec仍超时，诊断中；R2溢出探针还需处理时钟初值为0的合法边界。
本段为失败定位记录，不扩展已通过证据；诊断输出将在最终验收前移除。

### 17. 2026-10-27 机器失败修复与干净 W7b

页故障的CPL3现场现在捕获真实用户rsp/ss，恢复全部GPR后用RIP相对寻址重建iret帧，
避免COW重试覆盖r11。W9诊断构建已完成全部字段和正常退出；干净构建复测进行中。
锁定musl内部SIGCANCEL实际为33（32为SIGTIMER），原适配层误对32禁用RESTART；
修正为33，并在真实适配层host测试加入回归，5项host测试通过。W4诊断构建取消/异步取消/
detach/属性全部通过；临时输出已经移除，干净复测进行中。

W7b经3字节VFS片段创建/读取实际ELF并重复exec，原8192审计记录耗尽。
错误persona见证先缩为仍可正常执行的232B ELF，保留“不检查契约就会成功执行”的反例；
完整负例、路径exec和新crt链仍超过8192。直接扩至16384曾使非零None初值撑大内核文件，
触及原8MiB image边界，此次失败保留，不放宽image或结构体守卫。
审计槽改存有效的零初值AuditEvent，已有count是唯一可见边界，未追加槽不会形成审计证据；
存储进入BSS，16384条预算下内核文件约7.1MiB。新增回归检查未追加槽不可见和实例隔离，
等待最终workspace执行；审计耗尽的失败原子性回归仍按实际容量测试。

新C运行时已经启动后，第二次exec进一步触发32KiB syscall栈下界。
元数据查询改用arena投影，映射按需取单帧；映像构建的栈/帧/表临时缓冲放入不内联阶段，
在提交与回收旧映像前退栈。保持全部验证、回滚、32KiB栈和guard。
干净W7b命令Linux参考与BIOS两轮均PASS，输出为：
EXECVE|read=1;exec=1;wait=1;env=1;cloexec=1;atomic=1;contract=1;crt=1;reexec=1;ofd=1|ok=1;err=0
实际资源为caps_live=134、caps_peak=160/192、segment_slots=43/48、messages_peak=14/32、
audit_used=11295/16384。此证据只覆盖W7b；其余干净复测、全套68轮与最终文档检查仍待完成。

W4、W6、W9干净Linux参考与BIOS两轮现均PASS。R2又发现两个独立问题：
时钟初值0加Duration::MAX仍合法，溢出见证改为检查到达最大值后再加1ns必溢出；
上游公共spawn入口在unsupported PAL之前用System分配内部句柄，而本子集System按契约返回NULL。
隔离覆盖层现于共同spawn入口分配前返回Unsupported，输入锁增加第十项thread/lifecycle.rs。
初次补丁仍OOM，核对发现Cargo复用了旧std rlib（产物时间早于覆盖层），并未执行新拒绝逻辑。
构建脚本把覆盖层摘要加入每次编译的cfg指纹，保留外部编译参数，从而使build-std缓存随覆盖层失效。
新std已实际重编译，移除全部诊断后R2两轮完整PASS；保留旧失败日志，不将旧产物的检查算作本版证据。
musl取消点补丁的过时限制注释同步修正，八项补丁已重建并通过归档审计；指令实现未因注释改变。

完整单次命令随后取得67/68轮PASS，唯一失败为I1第二轮的排队/直投比例：
84/339/353与83/340/353，消息总数423、阻塞数353不变，串口/功能字段及退出清理相同。
宿主IRQ到达并不固定接收者的BlockedRecv/Ready时序，原逐类别相等不适用于交互注入。
交互runner改为严格的总消息数与阻塞数守恒，两条路径均须执行，保留全部协议/输出/清理断言；
新增丢失、重复、阻塞变化、缺少路径与溢出反例。普通程序的逐类别基线、30秒超时及双轮不变。
当前正进行I1聚焦与全量重新验收；67/68的失败命令不会记作整体PASS。

交互修订后workspace为1015项PASS、lint再次PASS；I1聚焦双轮PASS。compat cow-write-fault通过且未修改原基线。第二次最终完整68轮命令已开始，仍待结果。


### 18. N1定时信号跨轮计数回归（2026-10-27）

第二次最终完整命令在N1 run2遇到55/461/472与56/460/471的逐类别基线差异；
I1与此前机器失败项在本轮均通过。信号使接收者提前Ready时，回复由阻塞直投转为排队接收，
直投与阻塞一同减少，消息总数不变。N1单独检查消息总数与blocks-direct差值严格守恒，
不再假定与定时器竞争的排队/直投类别固定。增加丢失/重复、差值变化、direct超过blocks、
缺少路径与溢出反例，保留全部功能输出/资源/协议断言、30秒期限及双轮。
新增test-net只复用原有N1 Linux参考、构建及双轮完整validator，便于聚焦检查；完整命令仍需重跑。
这属于验收假设的显式修订，不能将旧失败改记为PASS。


### 19. 最终统一验收与评审边界（2026-10-27）

N1修订后workspace共1016项通过（xtask211、kernel487），根lint再次通过；
聚焦test-net的Linux同源参考与BIOS双轮通过。最后单次cargo xtask test退出0，
全部20项Linux参考和34组/68轮BIOS通过。I1和N1仅采用第17–18节明确说明的
时序不变量；其余程序仍逐项固定IPC计数。协议、功能、逐字节输出、退出、资源清理、
30秒期限与双轮要求均保留。前两次67/68的失败分别保存在最终日志final、final-2中，
不会将分段或失败命令算作整体通过。

最终命令和原始日志（当前工作环境的/tmp，非版本化工件）：

| 命令 | 结果与日志 |
|---|---|
| cargo test --workspace | 1016 PASS；/tmp/prima-stage7-final-workspace-3.log |
| cargo xtask lint | PASS；/tmp/prima-stage7-final-lint-3.log |
| cargo xtask test-net | Linux参考及BIOS双轮PASS；/tmp/prima-stage7-net-conservation.log |
| cargo xtask test | 单次68/68，20项Linux参考PASS；/tmp/prima-stage7-qemu-full-final-3.log |
| cargo xtask compat | cow-write-fault PASS；/tmp/prima-stage7-final-compat.log |
| Rust core/alloc与std Clippy | PASS；/tmp/prima-stage7-final-rust-core-clippy.log、/tmp/prima-stage7-final-rust-std-clippy.log |
| musl host/锁定构建 | 5项PASS；/tmp/prima-stage7-musl-host-final.log；八项补丁重建和归档审计见第17节 |
| lake build | 10 jobs PASS；/tmp/prima-stage7-final-lean.log |
| cargo miri test -p cosir --lib authority:: | 21 PASS；CARGO_TARGET_DIR=/tmp/prima-stage7-miri-target；/tmp/prima-stage7-miri-authority.log |
| cargo xtask tcb | PASS，统计口径见最新快照；/tmp/prima-stage7-final-tcb.log |
| 文档self-test/build/check/base | 全部PASS；/tmp/prima-stage7-final-docs-self-test-2.log及/tmp/prima-stage7-final-docs-{build,check,base}-3.log；基线107fd2ce263dbc17e899832d0ae14b367fc6e8bc |

实跑环境为固定nightly-2026-07-30（rustc 1.99.0-nightly）、QEMU 8.2.2，
pc-i440fx-8.2/qemu64/TCG、单CPU、128MiB。没有取得SMP或真机证据。
补装固定nightly Miri组件后，以独立产物目录检查authority模型21项全部通过。
全包尝试在旧COW大页表夹具耗时后手动停止（SIGTERM，cargo退出101），日志为
/tmp/prima-stage7-miri-cosir.log；没有将中止当作Miri缺陷，也不宣称全包通过。
容量：cap 192、Frame owner 640、审计16384；可用syscall栈仍32KiB，
FrameRegistry 32KiB、BootstrapState 13KiB和启动映像8MiB守卫保留。
W7b峰值为cap160/192、Segment43/48、消息14/32、审计11295/16384。
这些是给定负载的见证，不是所有调用路径的栈峰值或容量证明。

| 缺口包 | 已取得的本轮证据 | 保留边界 |
|---|---|---|
| 阶段0–4能力位置/载体/事件与受控继承 | cosir/kernel host回归、Lean位置保持性与CDT构建 | Lean未覆盖完整Rust状态机或机器；既有image/vspace细化分叉保留 |
| 进程隔离/会话/来源与退出 | W6/W8/N1双轮、host失败原子性/陈旧代次回归 | 单线程fork；同Process线程共享视图；不支持多LibOS根/personality隔离 |
| exec/新crt/连续exec与静态护栏 | W7/W7b Linux参考+双轮、构建检查和解析器负例 | 静态ET_EXEC；完整C内存映像不等于可存入2920B rootfs；无动态版本解析/内核persona |
| 信号/线程/IRQ与物理期限 | W2–W5/W8/W9/I1/T1/N1双轮 | 有界通知/现场；无altstack/实时排队/job control；#AC仅host路由，无机器见证 |
| D6用户选定A | R1/R2双轮与两种Clippy，隔离std重新构建 | 原生单线程std表内API；fs/net/process等仍不纳入 |
| 兼容映射/开源程序 | 逐入口台账、冻结echo/cat/ls双轮和参考 | 每行partial仅按登记边界；不宣称完整POSIX或任意程序兼容 |

可审阅的D1–D10、profile v2和S5-D7静态护栏选择已集中于
[最终范围评审工件](../specs/2026-10-26-stage5-release-decisions.md)。
本轮技术工作与统一测试完成；BLK-003/profile冻结、组织级S5-X3复核和阶段5签署仍待明确确认。
阶段6继续未开始；无250ns快路径、NIC/TCP/DNS、持久化/硬链接或完整std声明。
