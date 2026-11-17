---
id: spec-libos-data-path
kind: specification
authority: scoped-implementation-contract
status: implementation-contract-for-bulk-file-slice
lifecycle: current
updated: 2026-11-17
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

## P2b-0：Linux 数据面 op 矩阵与冻结规则（2026-11-17）

本节冻结 P2b 的线协议与适配边界。它是 P2b-1 的实现输入，不新增 syscall，
不改变 P2a 的授权模型，不关闭 P2。Store 保持 OS 中立：只返回中性记录，
Linux 的 stat/dirent 布局由域内前端生成。

### 范围与 op 矩阵

只迁移 D12 白名单实际触达的 Linux VFS 面；写盘、网络、终端和文件锁仍不在范围。
`open`（2）与 `openat`（257）共用同一实现。

| Linux 入口 | 线 op | 必需性 | Store 动作 | 结果 |
|---|---|---|---|---|
| `open`/`openat` | `OPEN=3` | `cat`/`ls` 必需 | 在登记策略视图内 `OpenFiles::open_in` | wire fd |
| `read` | `READ=1` | `cat`/`sort` 必需 | `OpenFiles::read`；目录返回 `EBADF` | 字节数 |
| `write` | `WRITE=2` | `cat`/`sort` 的 stdout 在域内，文件写保留 | `OpenFiles::write` | 字节数 |
| `close` | `CLOSE=5` | 必需 | `OpenFiles::close` | 0 |
| `fstat` | `STAT=4` | `newfstatat(AT_EMPTY_PATH)` 必需 | `OpenFiles::info` | 16 B 中性 stat |
| `newfstatat` | `STAT_PATH=10` | `ls` 必需 | 视图内解析后 `metadata` | 16 B 中性 stat |
| `lseek` | `LSEEK=9` | 目录回退与通用语义 | `OpenFiles::seek` | 新偏移 |
| `getdents64` | `GETDENTS=6` | `ls` 必需 | 整记录目录字节流 | 字节数 |

不在矩阵内的入口保持当前确定失败或 `ENOSYS`，不得静默模拟：`dup/dup2/dup3`、
文件 `readv`/`writev`、`open` 的 create/truncate/append、`rename`/`unlink`/`mkdir`/
`chmod`/`symlink`/`link`/`ftruncate`/`fsync`、文件 `mmap`、`statfs`、文件 `ioctl`。
`newfstatat` 只接受 `AT_SYMLINK_NOFOLLOW=0x100` 与 `AT_EMPTY_PATH=0x1000`；
`lseek` 的 `whence` 只接受 0/1/2；其它 flags/whence 返回 `EINVAL`。

### 线布局

沿用 PBIO v1 头（32 B）、8×32 B 描述符和 288 B 起始的数据区；新增 op 编码
3、4、5、6、9、10。描述符字段按 op 冻结：

| op | fd | offset | length | data |
|---|---|---|---|---|
| `OPEN` | dirfd（`0xff`=视图根） | 中性 open flags | 路径长度 1..64 | 路径起始 |
| `STAT` | wire fd | 0 | 16 | 输出起始 |
| `CLOSE` | wire fd | 0 | 0 | 288 |
| `SEEK` | wire fd | i64 位移的 u64 位型 | whence 0/1/2 | 288 |
| `GETDENTS` | wire fd | 0 | 请求上限 1..3808 | 输出起始 |
| `STAT_PATH` | dirfd | 中性 stat flags（bit0=nofollow） | 路径长度 1..64 | 路径起始；输出固定在其后 64 B |

- wire fd = Store handle 索引 + 3；应用 fd 号不进入线协议。`OPEN` 返回 wire fd
  （handle 索引 + 3），前端再分配应用 fd。Store handle 上限为 `SERVICE_FILE_HANDLES=8`。
  `OPEN` 的结果不能被同一 batch 的后继描述符使用；客户端必须单独提交 `OPEN` 批次，
  拿到 wire fd 后再提交读写/stat/getdents/close 批次。
- 中性 open flags：bit0..1 access（0 读、1 写、2 读写、3 非法），bit2 create，
  bit3 exclusive，bit4 truncate，bit5 append，bit6 directory；其余位必须为 0。
  P2b 前端只发送 read + directory；`O_CLOEXEC`/`O_NONBLOCK`/`O_LARGEFILE` 是域内
  标志，不上线。未知 Linux flag 在域内确定返回 `ENOSYS`，不得原样透传。
- 中性 stat 16 B：`node:u32 | kind:u8 | mode:u16 | pad:u8 | size:u64`，全部小端；
  `kind` 使用 VFS 原始编码（0 目录、1 文件、2 null、3 符号链接）。Linux 144 B stat
  布局由前端生成。
- `GETDENTS` 返回 VFS 原始整记录：`node:u16 | kind:u8 | name_len:u8 | name`。
  Store 不得返回半条记录：若请求上限放不下下一条完整记录，返回 0 且不前进；
  否则只前进并返回完整记录前缀。前端把能放进应用缓冲的整记录格式化为 Linux
  dirent；未能输出的整记录必须用 `SEEK`（`Current(-n)`）退回，保证下一次不跳过。
- `SEEK` 结果放描述符 `result:u32`；VFS 内容与目录总量均小于 4096，足够。
- 状态为**正 errno**，`0` 表示成功；`result` 为字节数、handle 或新偏移。
  部分读写不回滚同一 batch 中较早的成功项；批次不是 VFS 事务。

### 错误映射

Store 在 proxy 路径**一次**映射，前端只负责取负：

| VFS/protocol | 正 errno |
|---|---|
| `NoEnt` | 2 |
| `NotDir` | 20 |
| `Access` | 13 |
| `Exist` | 17 |
| `Invalid` | 22 |
| `NoSpace` | 28 |
| `Loop` | 40 |
| `BadHandle` | 9 |
| `TooManyOpen`（proxy） | 24 `EMFILE` |
| `Unformatted`/`Io` | 5 |
| 未支持 op/flag | 38 `ENOSYS` |
| 缓冲/槽位忙、序号错 | 11/22 |

`TooManyOpen` 的直接客户端路径保持既有 `EAGAIN`；只有 proxy 数据面使用 24。
禁止把 `EAGAIN` 复用为“句柄表满”，否则前端无法区分缓冲忙与 fd 耗尽。

### 容量与并发

- Store OFD 表全局 8 项，所有 owner 共享；满时 `OPEN` 返回 `EMFILE`，不做每应用预留。
  前端 fd 表 16 项（0..2 为标准 fd，3..15 为文件 fd），wire fd 只映射 3..10。
- 代理表 `MAX_PROXIES=8`，每代理一页缓冲、一个在途 batch；batch 最多 8 项，
  数据区 3808 B；路径 ≤64 B；`STAT` 输出 16 B；`GETDENTS` 上限 3808 B。
- P2b 数据面**不使用 `PROXY_SELECT`**：batch 的 `OPEN`/`STAT_PATH` 视图直接来自
  登记表的 `entry.policy`，按 `(发送者 cookie, slot)` 解析。`PROXY_SELECT` 保留给
  既有路径原语，不参与本数据面。
- 一个代理服务若同时服务多个应用，必须串行提交数据面 batch；P3 再把选择与暂存
  绑定到具体 batch。跨 personality 的两个代理是不同发送者，不共享选择状态。
- 序号、重放、槽位复用与 incarnation 规则沿用 P2a/P1：成功接纳后先消费再回写；
  重复/跳号拒绝；旧应用或旧代理死亡后关闭 owner，不得授权旧 cookie。

### fixture 与根

`prepare_policy` 的冻结内容（逐字节，SHA-256 为内容哈希）：

| 路径 | 内容 | SHA-256 |
|---|---|---|
| `/linux/fixture.txt` | `hello from prima\n` | `505231fb994a337c00dc563c684280c208f99617fdbf2f5cb284c6d4e6bb2cb5` |
| `/linux/fixturedir/alpha` | `a\n` | `87428fc522803d31065e7bce3cf03fe475096631e5e07bbd7a0fde60c4cf25c7` |
| `/linux/fixturedir/beta` | `b\n` | `0263829989b6fd954f72baaf2fc64bc2e2f01d692d4de72986ea808f6e99813f` |
| `/shared/note` | `shared\n` | `cf99975aa7995fad86fae7f3b0905143f30a52501944dff26002afc99c3b8419` |
| `/native/README` | `prima rootfs\n` | `f93300ea7f55bf939cf1791b8540524529df110305eac73c40dac61a9b06fe2d` |
| `/shared/escape` | 符号链接 → `/linux/fixture.txt` | 目标串 `55b5852658be1c0320ebb2f0cd26115f0d262a1ae873fd69badbb3387397197f` |
| `/shared/up` | 符号链接 → `../linux/fixture.txt` | 目标串 `39df9c3132bd7e670c67aeab1340e58a0bca181789c2ae0a82c5064e22b67de4` |

P2b-5 修订（显式登记，非静默改写）：`/native` 不再为空，新增
`/native/README` 作为原生策略视图的根资源；`/shared` 新增两条根外逃逸探针
符号链接，视图解析必须夹在根内并返回 `ENOENT`。已有四个 fixture 的字节与哈希不变。

`/linux` 根模式 0555；`/shared` 只由 `SERVICE_ROOT_SHARED` 暴露。
视图根名 `shared` 是保留挂载点：fixture 中不得存在同名真实节点，`prepare_policy`
发现冲突必须失败（fail closed）；运行时创建/rename 到该首段名必须拒绝。
`shared` 只匹配视图根第一段；`..` 从共享子树根返回视图根；绝对符号链接目标从视图根
重新解析。上述内容与哈希任一变化都算新契约裁决，不得在 P2b 内静默改写。

### 验收与边界

P2b-0 交付本节文本、op 矩阵与描述符形态校验；Store 执行、错误映射与端到端测试随 P2b-1 添加。
P2b-1 至少覆盖：每个 op 的合法/非法描述符、路径/输出区重叠、wire fd 越界、
`GETDENTS` 半记录截断与 `SEEK` 回退、`EMFILE` 与 `EAGAIN` 区分、保留名冲突。
P2b-5 才在 QEMU 上观察真实 ABI 程序；host 测试不能替代机器证据。本节不关闭 P2，
也不构成 7-A 证据。

## P2b-3：代理 fork/exit 生命周期（2026-11-17）

Linux personality 的 fork/exec/exit 不再操作域内 OFD 表，而是经已登记代理会话
提交 owner 生命周期消息。位域沿用 `PROXY_*`：slot 在 bits 32..39，RESERVE/RELEASE
低 32 位为预留 key，PUBLISH 另用 bits 40..47 携带子 Task id。

| 消息 | 载荷 | 语义 |
|---|---|---|
| `PROXY_RESERVE` | `tag \| slot<<32` | Store 为 `entry.process` 预留子 owner，复制 `entry.owner` 的 OFD 持有位；回复 `FORK_ACK \| key`，0 表示失败 |
| `PROXY_PUBLISH` | `tag \| slot<<32 \| child_task<<40 \| key` + 子 Task 的 TaskSend 能力 | 校验能力目标等于子 Task cookie（P2b-5 修订：受管 gate fork 在内核中无 Process 父系），把 key 绑定到子 Process cookie；回复 0 成功、1 失败 |
| `PROXY_RELEASE` | `tag \| slot<<32 \| key` | 关闭并释放尚未发布的预留；已回收视为成功 |

规则：

- 预留先于 `gate_fork`；fork 失败必须释放，失败窗口不得留下 OFD 持有位。
- 子 owner 槽位复用前必须关闭其所有 OFD；旧 key 与已发布预留不得再次发布或释放。
- 子进程注册自己的代理项时复用已发布的 owner；`reserve_external` 不再拒绝已发布子。
- 退出关闭 owner。fork 后父子共享 OFD 与游标；任一 owner 关闭只移除该持有位。
- owner 0、legacy 槽与 payload 中的 fd/NodeId/PID 均不构成授权。
- 真实代理会话接线属 P2b-4；P2b-3 的 host 证据使用同一语义的进程内 transport，
  直接客户端既有的 RESERVE/PUBLISH/RELEASE 语义不变。

## P2b-4：镜像接线与两 ABI 场景（2026-11-18）

本节冻结 P2b-4 的启动镜像、会话签发与客户端传输。历史镜像的行为、事件与回执不变；
本节不关闭 P2，也不构成 7-A 证据。

### 场景镜像与模式推断

- manifest 新增两个 opaque kind：`SERVICE_IMAGE_KIND_LINUX=0x105`（Linux
  personality）与 `SERVICE_IMAGE_KIND_NATIVE_APP=0x106`（原生/POSIX 应用）。
  它们只在 `cargo xtask test-linux-proxy` 打包的镜像中出现。
- 服务管理器从映像目录看到 Linux 条目即进入**场景模式**；历史镜像不含该条目，
  所有新增路径保持惰性。场景模式下，管理器在 Store 收到持久化段并回复三条探针
  之后创建两个客户端（避免落在 Store 尚未挂载的窗口），并按管理器自身创建时
  记录的任务 id 授权它们的文件会话注册。
- `PROXY_REGISTER` 的既有规则不变：发送者必须是已登记文件客户端（场景客户端按
  创建记录授权），应用 Task 经内核 cookie/祖系验证为发送者后代，回复目标必须等于
  发送者，`SERVICE_ROOT_SERVICE` 一律拒绝。Store 端再次用内核元数据校验代理、
  应用、Process 与祖系。

### 客户端传输

- Linux personality 的 `Remote`：先注册直接文件会话，拿到 Store 发送凭据；再以
  应用 TaskSend 注册根代理条目并映射 Store 提供的共享页。批量提交使用
  `PROXY_SUBMIT | slot<<32 | sequence`，完成前先写共享页、完成后读回。
  owner 生命周期沿用 P2b-3 的 `PROXY_RESERVE/PUBLISH/RELEASE`；退出用
  `PROXY_CLOSE | slot`。
- `PROXY_PUBLISH` 的成功 ACK 在 bits 8..15 返回已发布的 owner；客户端据此为子
  Process 建立会话。`PROXY_CLOSE` 对缺失条目返回 0（死亡回收已清理），对他人
  条目返回 `EACCES`。
- 原生/POSIX 应用使用直接会话（既有 `OPEN/SUBMIT/CLOSE`），不经代理；它读取
  Store 服务根的静态资源。**P2b-5 修订**：该选择让原生 ABI 看到服务根而非
  `/native` 私有视图，私有资源并不隔离；P2b-5 改为原生策略视图代理会话，
  `Direct` 保留为未选用的遗留传输。
- 场景端点由管理器创建，栈预算与基址是显式容量契约：64 页、
  `0x0B00_0060_0000`，避免与 3.2 MiB 的 personality 映像、1 MiB 的
  `ENDPOINT_STACK_VA` 及 4 MiB 的 `SERVER_VA` 重叠。Linux personality 的结构性根
  同时具备 `Derive` 与 `NsResolve`，以便按同一入口 ABI 创建 guest Task。
- 完成报告使用 `SERVICE_PROXY_DONE_TAG`（低 8 位状态）；管理器收齐两侧后写
  `S7ABI` 见证并退出 QEMU（0x10 成功 / 0x11 失败）。

### 验收与边界

`cargo xtask test-linux-proxy` 必须同时观察到：Linux guest 经 Store 读到
`/linux/fixture.txt` 的字节、原生应用读到 `/README`、代理注册阶段以及
`S7ABI|linux=1;native=1;proxy=1;status=0`。代理 fork 子注册、同一 `/shared`
资源的显式授权、伪造/旧 incarnation/根逃逸与单侧退出属 P2b-5；本节的场景不替代
正反例矩阵，也不关闭 P2。

## P2b-5：代理路径正反例矩阵（2026-11-18）

本节冻结 P2b-5 的机器证据口径与两处能力绑定修订。历史镜像、既有 72 轮 BIOS 套件
与 Linux 11 项差分的行为、事件和回执不变；本节不关闭 P2，也不构成 7-A 证据。

### 能力绑定（取代内核父系）

受管 gate fork（`SYS_GATE_FORK`）创建的 Task 在内核中 `Process.parent = None`、
`forked = false`，`task_parent_cookie` 返回 0。因此 P2b-3 的“子 Task 父 Process
等于 `entry.process`”校验在真实代理路径上不可满足，P2b-4 也从未运行过 fork。
P2b-5 改为能力绑定：

- `PROXY_PUBLISH` 携带子 Task 的 `TaskSend` 能力。Store 校验能力目标等于子 Task
  的完整 cookie，再按预留 key 绑定 owner。能力只能由其持有者派生，payload 中的
  Task id 单独不构成授权。
- `PROXY_REGISTER`/`PROXY_ADD` 不再沿内核父系遍历；管理器要求发送者是已授权文件
  客户端、出示主体 TaskSend 能力且回复目标等于发送者，并把主体能力转交 Store。
  Store 校验该能力目标等于命名的活 Task（cookie 与代际）后登记代理项。
- 旧 incarnation 仍由完整 cookie（含 generation）匹配拒绝；host 测试
  `reused_task_ids_cannot_select_old_sessions` 覆盖 id 复用，QEMU 无法强制任务
  重生，故该负例只在 host 层出声。

### 视图隔离与负例

- 原生/POSIX 场景应用改用 `SERVICE_ROOT_NATIVE` 代理视图（`/native` + `/shared`），
  不再经直接会话读取服务根。因此两侧私有根互不可见，`/shared` 是唯一显式共享
  资源。`/native/README` 与 `/shared/note` 是可读正例。
- `..` 在视图根处停留；绝对符号链接从视图根重新解析；相对链接从链接父目录解析后
  仍过 `in_view` 检查。`/shared/escape`、`/shared/up` 在任一策略视图中都必须
  `ENOENT`，而同一链接在服务根上确实指向目标文件（host 控制例）。
- 负例矩阵（原生应用经真实直接文件会话 + 代理会话）：猜 fd → `EBADF`；重放已完成
  `PROXY_SUBMIT` 序号 → `EINVAL`；猜未登记 proxy 槽位提交 → `EACCES`；私有根、
  `..`、绝对/相对符号链接 → `ENOENT`。

### 启动预算与两次启动

内核 Task 表 `MAX_TASKS=18` 且 `task_create_held` 只扫描无记录的 id（已销毁 Task
保留 Dead 记录、不复用），服务与场景客户端占满后每次启动只剩两个 `create_held`
空闲 id。故矩阵分两次启动：kind `SERVICE_IMAGE_KIND_LINUX`（`0x105`，名 `lxsc`）
跑 `cat` 与 `M4-fork`；kind `SERVICE_IMAGE_KIND_LINUX_MATRIX`（`0x107`，名
`lxmx`）跑 `cat /shared/note` 与 fork 限额探针。`gate_fork` 走
`ensure_thread_slot`，可复用无能力引用的 Dead 记录，故 fork 子任务不受此限。
每次启动仍由管理器签发 Linux 与原生两个代理会话，并各自完成报告。

### 验收

`cargo xtask test-linux-proxy` 必须跑上述两次启动并观察到：

- Linux：`S6EXIT|case=cat|M4-fork|cat-shared|fork-limit;status=0`、`M4 fork ok`/
  `M4 exec child`/`shared`/`M4 limit ok` 的字节流，以及
  `S7LX|read=1;proxy=1;status=0`。
- 原生：`S7NA|...;read=13;status=0`、`S7NPOS|case=shared;...;status=0`、
  七条 `S7NEG|...;status=0` 与 `S7NEGDONE|fail=0`。
- 单侧退出：首个完成报告触发 `S7HALF|first=linux|native`，管理器在两侧都报告后
  才写 `S7ABI` 并退出（0x10/0x11）。

一次启动同时覆盖两项后若再创建第三个 guest 根会得到 `ENOSPC`；这是已登记的
Task id 容量限制，不是可忽略的重试错误。反例失败必须保留在报告中，不得以重跑
覆盖。

## 验证与重审

首片host覆盖全批次预检、越界/重叠/保留位、owner隔离、重放拒绝、短读清零、
位置读写不改游标、ENOSPC与旧量子路径的部分进展对照及批量partial结果；
QEMU须覆盖真实C读写/fork/exec/线程清理，以及非法批次无先行副作用、重复序号拒绝、
客户端销毁权拒绝、重开序号延续与512B双向数据完整性。
共享映射只是减少逐字节IPC，不称端到端零拷贝。容量、格式、并发、profile、授权、
故障模型变化时重审；计划与报告分别记录待做和实际证据，不勾选7-A/B/D/G退出。
