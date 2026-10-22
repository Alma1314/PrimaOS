# 阶段 5 / M13a：符号链接、rename 与 fchdir 设计

**日期**：2026-10-22
**状态**：已落地（M13a）。VFS 实体现在 `prima-runtime/src/vfs.rs`，协议与服务端在
`prima-runtime/src/protocol.rs`、`prima-user/src/service.rs`，适配层在
`prima-user/c/prima_syscall.c`；证据见
[M13a 报告](../reports/2026-10-22-stage5-m13a-vfs-links.md)。
**上位计划**：[阶段 5 收尾与阶段 6 入口实施路线图](../plans/2026-10-22-stage5-completion-roadmap.md)
M13；[用户态 VFS 设计](2026-10-20-stage5-user-vfs-design.md)。
**范围**：补上 `rename`/`renameat`、`symlink`/`symlinkat`、`readlink`/`readlinkat`、
`fchdir`，并把内容区从“只增不减”改为可压实回收。**不**做硬链接、权限/`umask`、
`chmod`、stdio 文件 API 与跨启动持久化；这些留 M13b/M13c。

## 1. 决策与切片

D8 尚未组织级关闭。本切片按路线图建议的**工作决定**实现：

| 项 | 本切片 | 理由 |
|---|---|---|
| 符号链接 | 实现创建、`readlink`、路径跟随 | 用户态 VFS 可完整表达；不需要新内核对象 |
| 硬链接 | 不实现，适配层返回 `ENOSYS` 的路径由 profile 标 unsupported | 需要引用计数与持久化语义，超出本切片 |
| 权限/`umask`/`chmod` | 不实现 | 需要先冻结 mode 模型；留 M13b |
| 跨启动持久化 | 不实现 | 路线图 R/D8 建议留阶段 7 |

这些状态只写入 profile 的 `partial` 边界，不升级任何 `supported` 行，也不代表 D8 已关闭。

## 2. 节点与路径模型

### 2.1 符号链接

- `NodeKind::Symlink = 3`，线上编码沿用既有 32 B 节点布局；目标字节写入内容区，
  由节点的 `data`/`size` 指向，**不**改节点编码版本。
- 目标长度上限 `MAX_PATH`（64 B），空目标与超长目标在创建时拒绝。
- 允许悬空链接；解析到不存在的目标时返回 `ENOENT`。

### 2.2 路径解析（冻结）

`walk` 按分量逐项解析；遇到符号链接时：

1. 相对目标从**链接所在目录**开始，绝对目标从根开始；
2. 剩余分量按原顺序接在目标分量之后；
3. 展开深度上限 `MAX_SYMLINK_DEPTH = 8`，超过返回 `ELOOP`；
4. 待处理分量上限 16，目标分量过多返回 `EINVAL`（不引入动态分配）。

`resolve_link` 只展开前缀，末项**不跟随**（`lstat`/`readlink` 语义）。`open`/`stat`
跟随末项；`rename`/`unlink`/`mkdir` 的末项按路径本身处理。悬空链接上
`open(O_CREAT)` 的“创建目标”语义未实现（返回 `EACCES`），列入边界。

### 2.3 `rename`

同一 VFS 内移动节点，末项不跟随链接：

- 源不存在 `ENOENT`；前缀不是目录 `ENOTDIR`；父目录不可写 `EACCES`；
- 目标已存在：文件/链接可替换文件/链接；目录只能替换**空**目录；否则
  `ENOTDIR`/`EACCES`（冻结错误集没有 `EISDIR`/`ENOTEMPTY`，沿用 M11 的映射）；
- 目录不能移进自身或子树（`EINVAL`）；`rename(x, x)` 成功且不改动；
- 源带尾随 `/` 且不是目录时 `ENOTDIR`。

### 2.4 容量

| 项 | 旧值 | 新值 | 理由 |
|---|---:|---:|---|
| `MAX_NODES` | 16 | 32 | 符号链接/rename 用例在 16 槽下临时节点耗尽；32 槽仍留 ≥3 KiB 内容区 |
| `CONTENT_OFFSET` | 16+16×32 | 16+32×32 | 节点表扩大 512 B |
| `CONTENT_CAPACITY` | 3560 | 3048 | 按一页镜像重算，仍覆盖既有文件与 1.9 KiB 压实用例 |
| `MAX_NAME`/`MAX_PATH`/`MAX_COMPONENTS` | 16/64/6 | 不变 | 本切片未触顶，不预先扩大 |

## 3. 内容区回收

原实现只在创建时取 `data_used` 作为起点，写入不回收；删除后的空洞永久占用。M13a：

1. 写入增长时先看本文件到“下一个存活区间”的空隙与内容区尾部
   （`growth_limit`）；放得下就原地增长；
2. 放不下先 `compact`：按 `data` 升序把存活文件/链接内容左移、重写节点 `data`，
   再把 `data_used` 收到实际使用长度；节点编号不变，打开句柄不失效；
3. 压实后仍放不下则把该文件搬到尾部（`relocate`），旧区间成为空洞；
4. 头部 `data_used` 随原地增长/搬迁前移，作为下一次创建的起点。

两个先后创建的空文件因此不再共享同一区间。压实是单线程、O(节点数²) 的有界操作，
不引入动态分配；写入失败路径不改变已提交的节点状态。

## 4. 协议

新增双字符串暂存与分段回读，避免 64 B 路径被单条消息的 48 位 payload 截断：

| tag | 方向 | 载荷 |
|---|---|---|
| `SERVICE_VFS_PAIR_TAG` (0x5EDD) | C→管理器→存储 | `slot(47) | op(44..46) | path_len(32..39) | start(24..31) | 前 3 字节` |
| `SERVICE_VFS_PAIR_COMMIT_TAG` (0x5EDE) | 同上 | `slot(47) | op(44..46) | client(32..39)` |
| `SERVICE_VFS_PAIR_REPLY_TAG` (0x5EDF) | 存储→管理器→C | `op(44..46) | status(32..39) | data(0..31)` |
| `SERVICE_VFS_LINK_READ_TAG` (0x5EE0) | C→管理器→存储 | `offset(32..39) | client(40..47)` |
| `SERVICE_VFS_LINK_REPLY_TAG` (0x5EE1) | 存储→管理器→C | `status(32..39) | 3 字节(0..23)` |

- 槽 0 = rename 源 / symlink 链接路径 / readlink 路径；槽 1 = rename 目标 /
  symlink 目标字符串（按原样保存，不按路径解析）。
- PATH 片段复用 `SERVICE_VFS_PATH_TAG`；服务端按当前激活的暂存态路由到单路径或
  双路径缓冲。
- 槽 0 的 COMMIT 只确认暂存；readlink 的槽 0 COMMIT 直接执行并返回目标长度，随后
  用 `LINK_READ` 按 3 字节分段取回。
- 管理器只做路由：新增上述 tag 的转发与同步应答回送，不解释路径或链接语义。

## 5. 适配层

| Linux 号 | 调用 | 实现 |
|---:|---|---|
| 81 | `fchdir` | 目录 fd 打开时记录的规范化绝对路径 |
| 82/264 | `rename`/`renameat` | 两条路径规范化后走双路径暂存 |
| 88/266 | `symlink`/`symlinkat` | 链接路径规范化；目标原样保存 |
| 89/267 | `readlink`/`readlinkat` | 目标长度 + 3 字节分段；缓冲不足按 Linux 截断 |

- 目录 fd 的路径保存在适配层 `prima_fd_paths[19][65]`；`dup*` 复制、`close` 清除。
- `fchdir` 要求 fd 是目录且路径已知，否则 `ENOTDIR`/`EINVAL`。
- 硬链接与 `linkat` 未接线，落到默认 `ENOSYS`。

## 6. 构建容量

M13a 的共享服务代码使 debug（`-O0`）下的 `prima-log`/`prima-ns`/`prima-store`
从 21 页增至 29 页，超过 boot manifest 单段上限（149 页）。因此只对用户态 crate
启用轻量优化：

```toml
[profile.dev.package.prima-user]
opt-level = 1
[profile.dev.package.prima-runtime]
opt-level = 1
```

内核与 `prima-ostd` 的构建参数不变。优化后服务映像段实测约 86 页（含目录页），
默认探针合计约 105 页、`ls` 约 112 页、V2 约 98 页，均低于 149；`MAX_SEGMENT_FRAMES`
保持 149，不再次放宽。该改动只影响用户态 crates 的代码生成，不改变
`prima-abi` 号段、内核对象或 `deny(unsafe_code)` 边界。

## 7. 证据计划

- **host**：`prima-runtime` 的 `vfs::tests` 覆盖创建/相链/环/悬空、rename 的
  成功/替换/子树/非空/只读/缺失、压实回收与 32 槽边界。
- **Linux 同源**：`prima-user/c/stage5_vfs2.c` 在宿主参考树与 Prima 上编译同一源码，
  stdout 逐字节比较；覆盖相对/链式链接、readlink、截断、rename 文件/目录/链接、
  子树与非空拒绝、fchdir、只读父目录、链接环、类型错误与缺失路径。
- **QEMU**：`V2` 双轮，`VFS2_EXPECTED_STDOUT` 逐字节匹配；容量记录
  `[17, 0x5a, 0, 1, 1]`；既有 E1–E6/T1/F1/V1/M1/C1/L1–L3/D1/A1 全部重跑。

## 8. 边界

- 硬链接、`chmod`/`fchmod`/`umask`、`lstat` 的 nofollow 返回、权限模型、跨启动
  持久化、stdio 文件 API 不在本切片。
- 悬空链接上的 `open(O_CREAT)` 未实现为“创建目标”；符号链接目标长度受 64 B 限制。
- 路径/名称/分量上限未变；符号链接展开的待处理分量上限 16，超出返回 `EINVAL`。
- 构建 profile 的 `opt-level=1` 是容量缓解，不是性能目标；不改变证据边界。

## 9. 本设计不代表

- 不表示 D8 已关闭或 profile v1/v2 已发布；
- 不表示 M13 完成（权限、stdio、持久化仍未做）；
- 不表示阶段 5 退出条件已满足。
