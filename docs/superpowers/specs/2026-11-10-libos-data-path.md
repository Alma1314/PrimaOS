---
id: spec-libos-data-path
kind: specification
authority: scoped-implementation-contract
status: implementation-contract-for-bulk-file-slice
lifecycle: current
updated: 2026-11-16
---

# LibOS 数据通路改造与批量文件协议首片

## 范围与决定

作者于2026-11-10要求全部改造此前讨论的数据通路：域内语义前端、共同服务、共享缓冲、
批量/异步、背压及多核扩展。授权实施不等于所有技术契约已冻结或阶段退出。
上位为[白皮书§7/性能成本映射](../../../Prima%20OS.md)、
[阶段7范围](2026-10-31-stage7-scope-decisions.md)与
[VFS契约](2026-10-23-vfs-libos-foundation-design.md)。
[原生协议](2026-10-24-native-runtime-service-design.md)的旧分片协议保留；
本协议在认证会话之上新增独立版本入口，修订其单在途/共享缓冲限制的适用范围。
前序会话/OFD/fork边界及失败见[后继报告](../reports/2026-10-25-stage0-5-gap-followup.md)，
Gate与测量限制见[L1](../reports/2026-11-08-stage7-libos-benchmark.md)。

总体结构为应用域内personality前端直接访问授权资源服务；不同personality不串联转发。
Linux原样ELF继续经过Gate，域内库不自动替换其中的syscall指令。
内核保持服务/ABI中立，不能通过跳过能力校验或审计实现性能目标。

## 首片：版本1批量文件会话

新增用户态tag，不分配syscall：OPEN请求一页会话缓冲，OPEN_REPLY携带有限映射能力，
SUBMIT携带单调序号，COMPLETE携带该序号及协议错误，CLOSE显式释放会话缓冲。
发送者必须来自现有内核认证的Task incarnation及Sessions登记，owner仍由服务选择。
载荷里的fd不是Authority；OpenFiles继续逐项检查当前owner。

Store拥有缓冲根能力，客户端仅得到Derive/SegMapR/SegMapW，无SegDestroy。
每个Task最多一页、一个在途batch；最多8项，一个完成通知。缓冲/映射/句柄预算使用
既有内核额度，建立失败显式返回，不放宽容量。旧协议保留供其它客户端使用；
当前C适配层建立失败显式返回，不自动回退或重放。
OPEN在旧会话仍有缓冲时拒绝重复建立；Task槽复用不继承旧incarnation缓冲。
客户端先卸载映射/丢弃能力，再CLOSE；服务卸载后销毁，仍被其它映射引用时保留根并重试回收。
死亡客户端不再接受请求，清理失败不能丢掉唯一销毁能力。

共享页显式小端编码：32B头（PBIO、version=1、零保留位、u64序号、u32条数），
8个32B描述符，数据从288B开始。描述符包含op/fd/offset/length/data_offset/status/result。
所有描述符及数据区范围、保留位、输出初值、范围互不重叠先整体预检；不合法batch无VFS副作用。
服务先把整页复制到私有快照，此后只解释快照；客户端并发篡改不能改变已验证范围。
READ/WRITE/READ_AT/WRITE_AT使用原生OpenFiles语义。READ允许既有目录字节流。
每项按序执行，独立记录错误/字节数；批次不是VFS事务，较早成功项不因后项失败回滚。
读输出区先清零，短读不泄漏旧输入。原生WRITE_AT保持忽略append的契约。
WRITE优先整块写入；ENOSPC时在服务内按旧3B/2B量子继续处理，保留旧C调用的部分进展。
这些量子不重新跨域发送，批处理不改变既有磁盘容量或文件最大长度。
序号从1开始，成功接纳后严格递增；重复/跳号拒绝，不自动重放副作用。
完成通知丢失不允许客户端重新提交同一批次；恢复/去重由后继恢复协议单独定义。
exec的EXE1交接继续保持2200B，session只交接原4个u64，缓冲状态不进入wire。
新映像清零缓冲本地状态，重新OPEN时从Store获取下一序号；不改变原程序契约版本。

本首片的batch是有界顺序执行，客户端可以一次提交多项；不是异步环形队列或SMP验收。
资源背压为确定性的建立/提交拒绝，不承诺无配额的无限排队。

默认探针的精确通信基线随协议更换为queued/direct/blocks=51/119/128：
write(6)、read(4)、read(8返回7)合计减少4次数据分片请求（8条消息），
增加缓冲OPEN/CLOSE的4条消息，净减少4次直投/阻塞。容量观测为live134、peak148、
segment_slots40、audit1144；上限192/64/16384保持。旧基线146/39/1158及旧计数失败
保留于报告；新断言仍须严格验证缺失消息、泄漏和错计数，不因通信减少放宽为范围断言。

## 启动包与验收时限

仅对正常镜像内由本仓库编译的8个Rust服务复用现有strip_debug，再写入manifest。
Cargo原始调试制品保留；服务的entry、程序头和PT_LOAD载荷保留（ELF节表位置/数量
元数据随剥离重写）。不变更raw Linux ELF、C/Rust应用白名单输入或内核装载语义。
BIOS仍使用原30秒阈值；减少非装载DWARF传输，不以放宽验收时限掩盖失败。

I1的raw输入用例固定两次单字节read；不让UART合并时机改变调用次数和用户消息总数。
信号取消、SA_RESTART、EOF及生产TTY协议保持原义。每轮仍须严格满足阻塞接收归账，
跨轮用户总消息、非UART出口及其它接收终态保持一致；不把丢失/重复放宽为容差。

## 完整改造的后继契约门

- 共同文件服务：Linux fixture迁移到独立Store，冻结根视图、owner注册与启动镜像组合，
  保持两种personality的errno/append/目录布局适配，真实应用和跨域拒绝必须QEMU验证。
- 异步队列：冻结生产/消费所有权、release/acquire、序号回绕、满队列、通知合并、
  丢唤醒、取消及每会话公平配额；不能用单线程数组测试代替共享内存并发证明。
- 多核：先按CPU分区状态，再实现AP/IPI/TLB shootdown和锁顺序；沿用D15与7-B范围。
- 撤销/恢复：明确在途请求完成点、重启epoch、结果查询及副作用去重，与7-D/G对账。
- 性能：同平台/构建/负载分解Gate、传输、复制、服务和排队；未冻结门槛前不宣称性能达标。

## P2a：代理会话与受限根视图（2026-11-16）

本节冻结共同文件服务的第一段接线；它不关闭 P2，也不构成 Linux 数据面迁移或 QEMU 证据。

- **签发**：已登记的 LibOS 客户端向管理器发送 `PROXY_REGISTER`，携带应用 Task 的
  `TaskSend` 能力和代理自身的回复能力。管理器只接受已登记文件客户端、回复目标等于发送者，
  并把应用 Task 经内核 `task_cookie`/`task_parent_cookie` 祖系验证为发送者后代；策略
  `SERVICE_ROOT_SERVICE` 一律拒绝。管理器把 `PROXY_ADD` 转发给 Store，附带该回复能力。
- **Store 绑定**：Store 只接受管理器发送的 `PROXY_ADD`；再次用内核元数据验证代理 cookie、
  应用 Task cookie、Process cookie 和祖系。为应用分配独立 OFD owner，并登记
  `(代理 cookie, 应用 Task cookie, Process, owner, 根策略)`。请求里的 fd、PID、owner、
  Task id 或 NodeId 都不是授权；batch 槽位只有命中该登记表且应用 incarnation 仍存活才可用。
- **根策略**：由 Store 固定，客户端不能传 NodeId。`SERVICE_ROOT_SERVICE` 不可由代理选择；
  `SERVICE_ROOT_LINUX`、`SERVICE_ROOT_NATIVE` 分别以懒创建的 `/linux`、`/native` 为根，
  并把 `/shared` 显式挂到视图根名 `shared`；`SERVICE_ROOT_SHARED` 只暴露 `/shared`。
  Linux 策略在首次创建时写入既有 fixture（`fixture.txt`、`fixturedir/{alpha,beta}`），
  之后不重置内容。未注册代理策略的镜像不创建这些节点，既有服务根语义不变。
- **视图语义**：绝对路径从视图根开始；`..` 在视图根处停留；符号链接的绝对目标重新从
  视图根开始；相对目标的起点必须在视图内；`openat(dirfd)` 的起点、创建/删除/改权限/
  rename/link 的父目录都必须属于视图。`shared` 只匹配视图根的第一段，`..` 从共享子树根
  返回视图根。不是字符串前缀检查，也不接受旧协议里客户端自带的 NodeId 作为边界。
- **数据面**：代理 `PROXY_OPEN`/`PROXY_SUBMIT`/`PROXY_CLOSE` 使用独立槽位与独立缓冲页；
  slot 经登记表验证，SUBMIT 的序列仍严格递增，成功验收后先消费再回写/通知。每个应用
  的 OFD 生命周期由 Store owner 管理；应用或代理死亡时关闭 owner 并回收槽位，I/O 失败
  保留登记以便重试。直接客户端与旧会话路径不变。
- **边界**：Linux personality 仍使用域内 RAM fixture；P2b 才把 open/stat/read/write/
  getdents/lseek/close、fork 预留/发布/回滚/退出关闭迁移到 Store，并把 Store 与授权
  接入启动镜像。本段没有 QEMU 正反例，不能宣称双 personality 共享已完成。

## 验证与重审

首片host覆盖全批次预检、越界/重叠/保留位、owner隔离、重放拒绝、短读清零、
位置读写不改游标、ENOSPC与旧量子路径的部分进展对照及批量partial结果；
QEMU须覆盖真实C读写/fork/exec/线程清理，以及非法批次无先行副作用、重复序号拒绝、
客户端销毁权拒绝、重开序号延续与512B双向数据完整性。
共享映射只是减少逐字节IPC，不称端到端零拷贝。容量、格式、并发、profile、授权、
故障模型变化时重审；计划与报告分别记录待做和实际证据，不勾选7-A/B/D/G退出。
