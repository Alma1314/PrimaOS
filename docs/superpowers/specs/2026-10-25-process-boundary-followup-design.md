---
id: spec-process-boundary-followup
kind: specification
authority: scoped-implementation-contract
status: working-contract
lifecycle: current
updated: 2026-10-27
---

# 阶段 5 进程与栈边界后继

承接 [前批缺口补强](2026-10-25-stage5-gap-closure-design.md)、[M17](2026-10-23-stage5-m17-fork-exec-design.md)
和 [M18a 证据](../reports/2026-10-23-stage5-m18a-signal-kill.md)。按用户要求，先完成全部修复，最后统一测试；
本文是预期契约，不记录 PASS 或阶段签署。

## 每 Task 中断栈

所有 18 个 Task 栈均为页对齐的 4 KiB 下界 guard + 32 KiB 可用栈。
自建内核根省略全部 guard 页映射；Task 根克隆同一 supervisor 子树。
只通过裸地址计算栈顶，不创建覆盖未映射 guard 的 Rust 引用。
`syscall_fast_ready.stack_guard=1` 必须同时证明 syscall 专用栈和全部 Task 栈的下界页均未映射，
不改变字段格式。保护页不增加可用容量，也不声称所有调用路径的峰值证明或任意跳写保护。
更换页大小/栈布局/共享 supervisor 子树时重审。

## wait 的退出原因

Process 保存显式 `Exited(u8)` / `Signaled(u8)`，普通 exit 只取低八位；
原生 exit54 的输入仍是普通退出码。signal56 的默认终止记录 Signaled，
等待立即返回与阻塞写回使用同一四字节编码：普通退出 `code << 8`，信号终止 `signal & 0x7f`。
不能把 shell 的 `128+signal` 约定用于 wait status。W8 去掉接受143普通退出的宽松分支，
只接受 SIGTERM 的 WIFSIGNALED/WTERMSIG；保留四字节/COW写回与 guard 检查。
本项不自动补齐完整 libc signal、mask、EINTR、权限模型或进程组。

## W7 / W7b 的 Linux 参考

两条探针共用 ELF fixture 与执行 helper；Linux 通过 memfd/fexecve，Prima 仍使用内存镜像 exec55。
仅机器码的退出 syscall 号按平台选择（Linux60/Prima54），argc 和截断镜像断言相同。
W7b 参考从临时普通文件读取镜像，目标端仍读取既有 `/tiny.elf`，均保留父预读边界。
参考输出和退出码接入全量/聚焦验收，逐字节比较；不因此声称子进程 VFS 或完整 libc execve 已完成。

## Process 身份与孤儿回收

追加原生59 `process_identity(selector)`，0返回当前Process的leader Task id，1返回父leader；
其它selector返回EINVAL。无Process元数据的原生Task以自身为Process id，父标识取创建者，
Kernel创建者无用户父标识则返回0；不修改原生7的Task/线程身份语义、不复用52。
C getpid/getppid改查59，gettid继续查7；同Process线程的getpid必须一致。
fork父关系来自Process表。父退出/删除时清除子进程对旧Process id及其等待者的引用，
运行孤儿的父标识为init(1)，已退出孤儿立即回收，运行孤儿最终退出后不保留无等待者僵尸。
工作号段/语义不代替BLK-003或阶段签署。

## 可选接收来源元数据

原生18完整保留24字节旧布局、边界及忽略未用寄存器的行为；追加60返回40字节：cap0/cap1/count/sender/msg_id。
sender与msg_id来自内核消息记录，0发送者表示Kernel，用户payload不能覆盖来源。
不通过原生18的未用寄存器猜测新格式，避免旧机器码残留寄存器破坏兼容。新布局允许跨两页，逐页预检/处理COW并按物理字地址写回；
旧布局仍拒绝跨页。Runtime.receive使用新布局并校验ID/数量，经典C/机器码调用仍用旧布局。
阻塞接收记录内部format；用户直投与内核事件恢复使用相同编码，取走pending后不得重复交付。
用户线程不得在同一Process内故意破坏阻塞接收者的缓冲区映射；恢复时仍重新验证，
错误按机器接口返回EFAULT，不能造成内核坏指针访问。本项不构成并发VM/全COSIR细化证明。

## 线程组退出与 Task 代次

普通Task销毁20仍针对单Task；有活动共享线程的leader返回EAGAIN，避免释放其地址空间及能力空间。
54允许任一组员退出整个Process；默认致命信号同样走整组路径。新61使用leader的TaskDestroy凭据执行远端整组退出，
管理器在回收托管C程序的共享Segment前使用它。整组预检审计容量/后继可运行任务，先清非leader再清leader，只产生一次wait结果。
活动根的页表帧在切换CR3后释放；延迟队列按逻辑映射上限保守定容，退出不再保存已清除的旧FPU现场。

Task物理编号可复用，每次创建分配不回绕的代次；Task目标Capability保留创建时代次。
旧token、旧排队消息和旧命名空间引用均保留原位置，旧目标不因编号复用恢复可用。
创建/销毁审计包含代次，60的sender字为`generation << 32 | task_id`，Kernel为0。
同Process线程共享结构根及能力空间；Task退出清空其信号/调度缓存。

## exec 准备与提交

55保留四参数、无envp语义，新增62为六参数`image,length,argv,argc,envp,envc`。
均先逐页验证并复制输入，argv/envp各至多16项、每项含终止符至多128字节；超限失败，不静默截断。
静态ET_EXEC计划拒绝非canonical/保留地址、段页重叠、不可执行段外入口；初始rsp对齐16字节。
新根只继承supervisor子树；新映像和栈放入内核私有Segment，数据帧与表帧全部准备成功后才替换旧映像。
准备失败释放全部新帧并保留旧映像；提交登记全部逻辑映射、重置FS和信号现场，旧表帧延迟到CR3切换后归还。
多线程exec暂返回EAGAIN，须在后继整组exec实现后扩展。
私有exec Segment在最后一个映射消失后回收，fork共享映射在此计数内；Segment销毁按实际帧归属账本回收后续COW帧。
此处仍不是完整libc execve、CLOEXEC及适配层状态跨映像恢复的完成声明。

## 认证服务会话和 OFD 持有者

63查询Task代次、Process及父Process代次、所持TaskSend目标和官方self cap。
clone线程首次查询self时可建立其自身根并记录CapabilityMinted；已有self token被移动后不自动重铸。
同Process共享地址空间的逻辑映射归leader，所有活动组员入口页受卸载/去执行权限保护。
用户VM区间限定canonical低半区且作溢出检查；不得把共享supervisor子树扩展为用户映射。
MAP先预留整个区间所需表帧，失败归还全部预留而不留下部分映射；单页底层映射同样归还未链接的失败批次。
wait/clone输出逐页按对应Segment处理COW；clone预检TLS及TID输出，并按flag决定写入和清除登记。

C首次VFS请求用self派生回复凭据，管理器比对内核来源cookie和回复cap目标，依据已授权根客户、同Process或父关系建立会话。
管理器先发送认证的存储服务路由cookie，再向存储端移交回复和发送凭据；客户同时核对路由来源、回复来源及cap目标。
后续VFS请求直接送存储服务，payload里的client编号不选取权限。路径/双路径/链接分片使用每Task私有暂存，
按稳定wire格式放在存储服务独占Segment的额外页中；文件树仍使用原4KiB区间。
每个OFD保留共享游标及Process/预留持有者位图；fd别名计数仍在各Process适配层，最后一个本地fd关闭才释放该Process holder。
只有最后一个holder关闭才减少后端open_refs。会话及owner同时比较完整代次，槽复用不能取得旧会话权限。

存储状态段从1页扩为6页；普通服务用户栈2页、存储服务4页，C栈保持8页。
逻辑vspace上限1024，容纳共享物理页对应的父/子逻辑条目及exec替换；仍有确定容量错误，不改变动态帧上限。
这些是本版本工作容量，需统一QEMU/TCB/资源验收；不继承前批机器证据。

## fork 的准备、交接、发布

原生51保留立即fork机制；C/musl的57入口改用新增64事务：selector0准备、1发布、2取消。
准备先检查资源并复制快照，创建新Task/Process/独立能力视图，但状态为Prepared，不可调度，也不进入wait选择或普通信号目标。
返回父进程一条仅用于本次事务的Task管理ticket。ticket和官方子self来自各自合法的新Task根，不建立跨目标CDT边。
普通cap_derive仍要求Derive；只有原先选定的fork继承是例外。

C在内核准备前预留全部已有OFD的子holder；准备后以ticket派生子回复凭据，由管理器验证Prepared及父代次并交接服务会话。
存储服务将路由/发送凭据排入Prepared子邮箱，并向父确认；全部交接成功后父调用64发布，子才可调度。
子消耗预排队消息，不再发布后铸造注册凭据。交接失败则取消内核事务并释放预留holder，fork向父返回错误。
已发布的holder/会话回收按内核代次判死；父在发布前死亡时，内核取消Prepared子，服务端回收相应预留。

每个事务预留足以应付全局消息/能力/异常/Timer/私有Segment上限的取消审计预算，其它操作不能消耗它。
丢弃ticket、承载ticket的容器销毁或父退出均可取消；外部持有的其它后代按§8.2保留为stale，不递归撤销。
准备期间阻止父VM显式变更、exec和新增线程；保留父正常COW写入。取消只恢复仍指向快照帧且权限未被另行改变的叶项，
不覆盖已私有化的页，并同步当前CPU的TLB。发布释放取消预算；硬件快照不进入Lean证明。
线程注册失败先摘除Process成员再删除记录，丢弃新self根并记录销毁；musl线程出口清理会话凭据，避免共享Process中的遗留根。

完整C新crt映像见证已写入待验收；std OS PAL、整体抽象模型/形式映射及组织级门仍需后续工作；后继实现均待统一验收。


## 通用 upcall、线程退出与真实保护页（工作扩展）

新增65为语义中立的有界upcall控制：0动作、1掩码、2待处理集、3通知、4导出现场、5恢复现场、
6临时掩码与原子等待、7查询动作、8注册生命周期/硬件故障路由。64槽按位合并，保留首条来源；不是实时信号队列。
动作由Process共享，mask/pending/8层恢复栈与FPU现场按Task隔离；普通fork复制动作/mask/活动恢复栈，不复制pending，
exec复位捕获动作、活动现场及FPU，保留ignore与mask。创建线程复制mask并共享动作；Task复用清空全部缓存。
handler入栈避开128B red zone。720B通用现场包含160B GPR/硬件返回帧、mask/来源/种类/值/token及512B FXSAVE。
只接受栈顶token，返回强制用户CS/SS、清除特权flags并验证canonical PC/SP和MXCSR；FPU、mask与嵌套帧同时恢复。
临时掩码等待在递送时将原mask放入恢复现场，无检查与阻塞之间的窗口，且不受RESTART重放。

Process有不可由用户选择的通知组，独立创建的服务使用不同组；fork继承组，父退出不改变组。
用户通知只能发给同组Task，或调用者确有目标TaskDestroy授权的Task；裸编号不能终止无关服务。
内核的子退出与硬件故障通知使用预登记路由，POSIX信号号/si_code的解释仍在C边界。
默认终止回收整个Process并保留Signaled wait原因；硬件错误码帧与无错误码帧分开解码。
阻塞recv/wait可被upcall摘除并以EINTR恢复，标记RESTART的调用仅在原指令确为syscall/int80时重放。

C/musl边界接rt_sigaction、mask/pending、kill/tkill/tgkill、sigsuspend/pause和自动trampoline，
映射siginfo/ucontext并允许用户修改通用寄存器与FPU恢复现场。默认SIGCHLD/URG/WINCH忽略由C登记；
SIGCHLD、SEGV、ILL、FPE号通过路由绑定，内核不内建这些POSIX号。
公开支持的普通信号为1–31中的非job-control子集；32–34供musl内部使用。
自定义restorer、alternate stack、job control、实时排队和NOCLDWAIT仍无支持契约；不接受后不执行。
锁定musl 1.2.5的取消信号为33（32为SIGTIMER、34为SIGSYNCCALL）；33不自动重启，使取消点得到EINTR；异步取消仍经过musl清理链与共同线程出口。

66原子解除指定用户栈范围并退出当前共享线程，先验证完整范围和销毁审计预算，再在内核栈完成解绑与Task销毁。
C分离栈保持分配占用，直到该Task代次消失后才允许复用；内核官方线程self根在退出时退役，外部后代保留。
67将用户word比较与单对象generation等待登记合并，消除futex值检查/登记窗口。
C每Task使用独立WaitObject并按wake(count)选择，退出清理对象；WaitObject扩为32、每对象等待者与Task上限对齐，
表移至既有分段arena，保持BootstrapState的13KiB生产编译守卫。signal/destroy预留整批唤醒审计，generation不回绕。

68只切换映射的用户可访问性，保留帧、原逻辑权限和COW状态，以独立审计事件记录。
C的PROT_NONE不再返回可读页；重新开放前先完成权限变更。fork保留隐藏页，对只读和隐藏叶也登记共享标记，
mprotect恢复写入不能绕过COW；只读收权移除可写COW标记，内核copy-out同样不能越过只读权限。
普通匿名分配拒绝缺少映射授权的退化成功，分配状态在Process内串行修改；sched_yield无其它Ready时返回0。
W4/W9已加入取消、detach复用、属性错误、临时mask、EINTR/restart、SIGCHLD、故障恢复与guard回归代码，均尚未执行。


## 显式 exec 交接与 libc execve（工作扩展）

新增69接收88B描述符：`image,length,argv,argc,envp,envc,ranges_ptr,range_count,registers_ptr,adopt_cap,redirect_pc`。
最多4个`(base,pages)`区间、合计128页；只保留调用者已有逻辑user/hidden映射，空洞保持空洞。
保留区间不得覆盖新ELF或栈；新根保留原帧、权限及COW，提交后同步登记Segment逻辑引用。
6个入口寄存器依次为RDI/RSI/RDX/RCX/R8/R15；R12为原ELF入口。
非零redirect必须位于保留的可执行用户页；它是通用提交后入口，内核不解释内容、fd或POSIX状态。
可选adopt_cap要求调用者持有其自行创建Segment的SegDestroy授权，成功提交将token移交HeldByKernel并审计，
随后该Segment随最后映射消失回收；失败不移交。描述符、权限、页表与资源准备先于不可失败的提交。
55/62保持既有参数接口；三条路径均使用8页用户栈，auxv增加PHDR/PHENT/PHNUM/BASE及固定单用户字段，
提交更新Task登记入口并清旧clear_tid。仍只接受静态ET_EXEC，单线程Process；不是动态链接或阶段6的声明。

C execve先检查并读取实际打开OFD的执行权限与文件内容，再生成3页交接区：版本化opaque状态、关闭请求、独立RX过渡代码。
旧映像的fd表在准备期间不变。提交成功后，保留的过渡代码先按认证sender完成CLOEXEC最后引用的服务端close，
再进入任意新ELF；小原生ELF也执行关闭，不依赖新crt。不能在该提交后阶段回退为旧映像成功运行，错误以Process终止处理。
新Prima crt恢复剩余fd/OFD引用、cwd、umask、termios、进程等待记录、服务会话和旧mask，重建heap空洞、恢复RW并清零新libc arena。
交接区由内核接管，原生程序直接退出也可回收。FD_CLOEXEC按fd关闭，剩余dup别名继续持有OFD。
失败清临时映射/Segment/镜像buffer并恢复mask，不关闭旧fd。VFS内容容量及16项argv/envp、每串128B限制仍显式保留。

逐帧owner账本在外部arena中从320扩至640，覆盖原320+192页启动映像预算+96页保留heap私有化+3页交接的611条上界，保持32KiB结构体守卫。
不放大栈上的SegmentFrames向量；本批最终QEMU/TCB仍需验收资源预算。
W7b已改为fork子内直接libc execve(path)，使用可写VFS生成的紧凑静态ELF；探针检查envp、CLOEXEC的实际服务端关闭、
剩余OFD可用，以及ENOEXEC时旧CLOEXEC fd仍能读取。Linux参考从同源构造本地ELF；回归未执行。

WaitObject ID改为实例内单调不回绕，避免对象销毁后旧外部token因ID复用重新授权新对象；到u32边界确定失败。


## IRQ、TTY 事务与物理期限（2026-10-26 工作扩展）

原生70 `dev_irq_create(line,out[2])` 返回Device和可独立委托的ExcSource cap；单机8259线路1–15中2保留级联、0保留PIT。
71以DevInterruptSubscribe授权mask(1)/unmask(0)，unmask启用调用者用户IF。源的ExternalInterrupt种类编码为7，
订阅按目标对象与实际持有位置授权，不要求原根cap仍被持有。销毁Device同时销毁其专属源，外部旧token失效。
IRQ入口保存完整GPR、清DF，处理spurious7/15，单次屏蔽线路并EOI；有界mailbox/审计暂满时保留pending位，在普通入口/idle重试。
设备ID单调不回绕，拒绝重叠IRQ注册；这不增加MMIO/DMA/远端设备或I/O端口排他性声明。

管理器订阅COM1 IRQ4、打开UART接收中断和OUT2；只读取已经到达的字节，没有字节立即回事件循环。
待读FIFO最多18×8项，每Task最多8项，来源使用认证cookie，队列满为EAGAIN。UART行缓冲和ready各64B；
满时保留硬件屏蔽，读者腾空后继续排空并unmask。超长canonical行最多63B加换行，保持原有有界行契约。
读请求低16位为非零序号；回复32–47为同序号、24–31为长度、低24位为数据，长度255表示低24位errno。
追加5EEB取消、5EEC确认、5EED读者确认；取消确认低位1表示未消费输入，0表示读回复已经提交。
成功回复保留服务端记录直到读者确认；发送失败不消费输入。
SEND的QueueFull后继映射为EAGAIN，可让确认发送者让出后重试；AuditFull保持ENOSPC，不伪装成可恢复队列背压。死亡代次清除排队请求，已提交回复通过正常接收或取消握手收齐。
C按Task和最多8层请求暂存外层TTY回复，认证Manager cookie；已完成握手后才复用16位序号。

upcall动作追加IPC_RESTART_IN_USER(32)：内核不自动重放18/60，由C在回调后检查缓存并决定重启或取消；
普通generation等待/wait4仍可内核重启。普通fd/VFS的短事务在Process锁内完成回复和本地状态修改，再恢复信号掩码，
避免已提交操作被EINTR截断后留下同tag旧回复。阻塞TTY/网络接收及futex/wait/sleep不持该锁；网络事务按下节独立关联和取消。
带未完成TTY或网络接收调用栈的fork/exec暂返回EAGAIN，保留原读事务；siglongjmp跨接收栈、完整atfork语义没有新增支持声明。

会话注册可附第二条针对本Task的TaskSend|TaskDestroy cap（不含Derive），Manager验证目标cookie后保留作输入通知控制；
fork交接由ticket派生该控制cap，失败或旧会话回收时显式丢弃。Store不接收这条控制cap。
ISIG开启时固定Ctrl-C/Ctrl-\通知当前等待输入的Task的槽2/3，清行输入；没有前台ProcessGroup/job control声明。
原生65仍是中立槽通知；POSIX槽号由用户态Manager/C约定。TERM控制字符自定义、完整termios及pty仍不在本扩展范围。

原生72 `task_wait_deadline(absolute_tick)` 不需WaitObject，只等待自身物理期限；1为到期，负EINTR为中断，永不自动重启。
67的有限deadline改用与40相同的物理时钟；44原有抽象绝对时间契约保持不变。
物理时钟在单CPU下用两个PIT周期校准时间戳计数器，按约100Hz返回单调tick；采样回退钳制，校准失败确定拒绝。
IRQ0只评估物理期限并唤醒，不推进抽象COSIR Timer，也不执行抢占见证以外的任务轮转。
该校准不声明跨CPU、频率变化、迁移或墙上时间正确性。C nanosleep/clock_nanosleep改为72阻塞，
相对睡眠EINTR写回剩余量；futex有界时间转换检查溢出。没有Ready任务时在内核STI/HLT/CLI休眠，IRQ回调返回后选择Ready；
线程/Process退出也可进入该idle，不再以缺少立即Ready后继拒绝退出。当前根保持延迟释放，直到切入其它根。

新增纯队列/物理时间分离与取消回归，以及I1的Ctrl-C、取消后再读、SA_RESTART和handler嵌套stat，
W9的SA_RESTART睡眠仍返回EINTR及rem检查。I1输入脚本等待完整marker换行再注入，避免IRQ提前回显穿插marker。上述均仅写入/编译，尚未执行统一测试。


## Socket 的持有、认证与接收提交

承接[M19设计](2026-10-23-stage5-m19-loopback-udp-design.md)和[历史证据](../reports/2026-10-23-stage5-m19-loopback-udp.md)。
保留IPv4 loopback UDP、4端点×4数据报×64B的边界；后继表移至`prima-user/src/network.rs`。
Manager只接受认证Task cookie对应的会话，以会话Process cookie校验holder，不相信payload身份。
线程共享Process holder；fork在Store交接与发布前复制holder，失败、未发布子进程回收及Process死亡释放holder。
每个Process最后一个本地dup引用关闭才释放该holder，另一个Process的socket继续有效。

后继请求格式为`tag | op(44..47) | handle(42..43) | sequence(34..41) | arg(0..25)`，
应答同格式，另以26–33位保存status。sequence为非零8位请求序号，身份来自60的来源代次；
不兼容M19历史payload client布局，不将实验性内部协议声明为稳定外部ABI。
C按op/handle/sequence及Manager cookie匹配；短RPC掩蔽回调，发送全事务还持Process fd锁。
每Task独立64B发送暂存，最多一个阻塞/已提交接收、一个短RPC；合法嵌套快速回调可缓存外层接收头。
嵌套阻塞recvfrom返回EAGAIN。序号只有在握手完成后才复用，短RPC避开活动接收序号。

接收按请求顺序选择等待者，只有头回复发送成功才从socket队列取出数据报，保存为该Task私有快照。
读者用op6的arg低16位offset、16–23位原接收序号取3B块；op10 FINISH携原序号释放快照。
op9 CANCEL携原序号，data=1表示尚未提交且已取消，data=0表示已经提交、必须读完并FINISH。
非SA_RESTART中断先取消/收齐头再返回EINTR；已提交头返回数据，SA_RESTART继续等待原请求。
短RPC回复优先，空闲接收FIFO；一个目标邮箱满时跳过它继续投递其它Task，之后让出并自发唤醒重试，
保留正常请求处理机会。失败投递不消费数据报。最后一个fd关闭仍保留活动发送/接收对象，完成或Task死亡才回收。

N1增加fork/dup/双侧close、EINTR取消后再读、SA_RESTART和handler嵌套stat；
纯模型增加并发发送暂存、接收私有快照、认证代次、错序chunk及死亡回收回归。均待统一运行。
管理器入口帧由10392B增至14584B（新增完整事务表）；用户栈由4页扩到8页，内核创建与init映射同步。
该测量不是全部调用链峰值证明；整体映像149页旧上限的后继处置见下一节。


## 完整新 C crt 见证与 Segment 生命周期

W7b保留真实`execve(path)`的紧凑ELF/失败原子性验证，再嵌入独立编译的
`stage5_exec_child.c`静态ET_EXEC。新子映像使用相同Prima crt、适配层和musl，
经`prima_exec_image`进入与路径exec共用的完整交接流程；Linux参考用memfd/fexecve。
RAM rootfs内容区只有2920B，不能保存该完整libc映像；本见证分别证明路径读取和完整crt，不合并成大文件系统支持。
静态子映像以`-z separate-loadable-segments`保证PT_LOAD页对齐，保留PT_TLS和PHDR。

新main通过公开libc验证argv/envp、构造函数、volatile TLS及errno、pthread_self、
保留fd的共享OFD游标、CLOEXEC、cwd、umask、信号掩码/忽略动作保留和捕获动作重置，
并在Prima验证termios、分配清零内存。随后新libc再调用`execve("/tiny.elf")`，
Linux使用相同fixture的memfd参考；第二次替换验证旧交接VA可重用。
父进程在wait后检查共享游标，末次argc退出码为2才接受整个链。
这些字段均尚未运行，不单凭成功编译记录PASS。

新crt恢复必须卸载全部3页交接范围；只卸载状态首页会留下控制/桥代码页，阻止第二次交接。
CLOEXEC桥遇SEND的EAGAIN先yield再重试。原生内存映像入口同样预取arena授权和初始化heap。

Segment表的48个槽现在允许复用inactive记录；对象ID保持严格递增，使用保留表的最大ID加1，
耗尽u32确定拒绝。槽复用不复用对象身份，因此旧cap仍指向已死对象。private exec位按实际槽登记/清除，
不再假定新对象总追加；容量`segment_slots`继续表示分配过的表槽高水位。审计与cap历史仍是显式有界预算。
新增纯回归覆盖超过48次槽循环、旧cap不复活、ID耗尽及外置向量的非连续帧顺序，待统一运行。

仅编译阶段已测得W7b嵌入C子映像后的聚合服务映像约150页（含参数载体），超过旧149页；
普通/用户Segment的内联149页上限保持。仅聚合boot服务映像增加独立192页外部arena向量，
启动解析和创建均校验该边界，逐帧分配/归属及失败回滚不变；BootstrapState只新增8B句柄。
不重新扩大所有SegmentFrames内联数组，避免历史192页数组引发的栈峰值问题。
这一编译测量使用已有musl基座作尺寸参考；锁定补丁重建、全部镜像实际峰值与QEMU仍待统一验收。


## 2026-10-26 对齐故障路由

65 selector8增加event4，对应CPU #AC，C初始化为SIGBUS(7)；info.kind8由C翻译BUS_ADRALN。
无CR2的#AC仍以RIP作为可报告地址，不承诺获取真正的数据操作地址；纯路由回归待运行，无新机器见证。


## 2026-10-26 静态 exec 声明护栏

C exec共享路径在交接分配、根替换及CLOEXEC前新增[目标声明校验](2026-10-26-static-program-contract-design.md)。
完整C子映像和两个紧凑ELF见证携带相同persona/adapter声明；缺失、重复、不兼容或截断拒绝ENOEXEC。
W7b新增contract字段：共同解析器版本负例，Prima另验证错误persona的实际exec失败及fd原子性。
Linux不执行Prima装载策略，仅作为同源解析器及原有exec功能参考；本增量全部未运行。


本后继的musl归档裸陷入白名单收紧为clone.lo仅48、restore.lo仅旧机制58。
clone返回后的线程出口及__unmapself转入C适配层清会话/clear_tid，再调用49/66；不再允许clone.lo直接49。
这局部修订2026-10-24边界设计的48/49白名单，旧归档证据保留；新锁定补丁构建和审计均待统一执行。

## 统一验收中的资源布局修订（2026-10-27）

完整路径exec、失败原子性、新crt和连续exec工作负载使用16384条外置审计槽；
槽存有效零初值事件，条数边界以内才可读取，不将占位事件视作已发生记录。
此布局让存储进入BSS，保留8MiB启动映像限制和BootstrapState 13KiB守卫。
普通cap上限192、Frame owner上限640；FrameRegistry保持32KiB守卫。
exec构建临时栈、帧向量和页表批次在提交/旧映像回收前退栈；syscall可用栈仍为32KiB。
这些容量调整不改变失败前预检与审计耗尽语义，也不构成所有调用路径的栈峰值证明。

## 交互IRQ验收的跨轮计数（2026-10-27）

I1输入由宿主在marker后注入UART；到达时接收者可能BlockedRecv，也可能仅Ready，
所以同一用户SEND可走直投或排队。固定脚本的一次完整重跑实测84/339/353与83/340/353，
两轮消息总数423、阻塞数353，输出和清理相同。
交互验收保留全部结构化事件、共享cap排队/直投见证、逐字节输出、EINTR/取消/重启和资源清理检查；
跨轮改为严格检查queued+direct与blocks相等，且每轮queued/direct/blocks均非零、加法无溢出。
新增拒绝丢消息、重复消息、阻塞数改变、缺少路径和计数溢出的回归。
默认启动及除N1外的非交互程序仍逐项固定queued/direct/blocks，不改变超时或运行次数。
这不是所有输入时序的证明，实际通过范围以报告为准。


## N1定时信号验收的跨轮计数（2026-10-27）

N1同时覆盖阻塞UDP、定时信号及EINTR/重启。信号唤醒与服务回复先后关系可让原本的
阻塞直投变为不阻塞的排队接收；一次重跑实测55/461/472与56/460/471。
固定脚本的queued+direct均为516，blocks-direct均为11；这两个量必须跨轮严格相等。
计数加法无溢出，direct不能超过blocks，各路径仍须非零；回归拒绝消息丢失/重复、
阻塞差值改变、非法大小关系、缺少路径和溢出。该规则只应用于N1，不推广到其它程序。
全部网络功能字段、逐字节输出、Process holder/事务清理、协议见证、退出码、双轮及30秒期限保留。


本后继当前实际运行、失败修复及未覆盖项统一查
[分层验收报告](../reports/2026-10-25-stage0-5-gap-followup.md)；
上文按日期保留各次实现/未验收记录，不以契约文字替代最终技术证据或组织级签署。
