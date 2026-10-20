# 阶段 5 / M8：用户态 VFS/rootfs、通用 fd 与多级路径（设计）

**日期**：2026-10-20
**状态**：M8 实现契约；profile v0 的补充提案见
[profile v1（delta）](2026-10-20-stage5-profile-v1.md)。本文件说明**已实现**的语义与边界，
不把目录/路径支持扩大成通用 POSIX 文件系统承诺。

## 1. 目标与取舍

M8 在 M4/M7 的“单文件 `f` + 固定 fd 3..10”之上补三件事：

1. **用户态 VFS/rootfs**：存储服务在已移交的 1 页 MemorySegment 上格式化一棵静态根树，
   文件内容、节点元数据、目录项都在该段内；内核不解释文件树。
2. **通用 fd**：适配层拥有 fd 0..18 的统一表项（null / tty / file / dir），`dup*`、读写、
   `lseek`、`close` 按表项类型分派；fd 0 预置 null，1/2 预置 tty。
3. **多级路径**：绝对/相对路径、`.`、`..`、重复 `/`、尾随 `/`、`openat(dirfd, ...)` 与
   目录枚举。路径在存储服务里解析，Namespace 只作**一次**授权入口。

取舍：不引入符号链接、`mkdir`/`unlink`/`rename`、cwd/`chdir`/`getcwd`、文件映射、
跨服务并发写锁或持久化。没有足够冻结错误码的操作（如 `EISDIR`/`ENAMETOOLONG`/`ELOOP`）
不新增错误码，统一落到已冻结的 `ENOENT`/`ENOTDIR`/`EACCES`/`EEXIST`/`EINVAL`/`ENOSPC`。

## 2. 根文件系统

静态根树（节点 0 固定为根）：

| 路径 | 类型 | 内容/权限 |
|---|---|---|
| `/` | dir | 只读；子项 `f`、`README`、`etc`、`usr`、`tmp`、`dev` |
| `/f` | file | 可写；M4–M7 探针的兼容文件，初始为空 |
| `/README` | file | 只读，`prima rootfs\n` |
| `/etc/motd` | file | 只读，`welcome\n` |
| `/etc/hostname` | file | 只读，`prima\n` |
| `/usr/lib/version` | file | 只读，`1.0\n` |
| `/tmp` | dir | 可写；`O_CREAT` 唯一允许新建普通文件的位置 |
| `/dev/null` | null | 读 EOF、写丢弃（任意 access） |

镜像布局（相对 VFS 存储起点；段偏移 0..8 仍留给 SPEC-112 状态字）：

```text
0..16    header: magic "PVFS" | version=1 | node_count | data_used
16..528  16 × 32 B 节点条目
528..    文件内容区（容量 3560 B）
```

节点条目：`name_len u8 | kind u8 (dir=0,file=1,null=2) | parent u8 | writable u8 |
size u16 | data u16 | name[16]`。名称 ≤16 B、路径分量 ≤6、路径 ≤64 B；这些上限同时
把 `resolve`/`open` 的栈帧压进服务端点的 1 页栈预算。

## 3. 路径与 open 语义

- 绝对路径从根开始；相对路径从 `dirfd` 的目录节点开始；`AT_FDCWD` 等价于根（本阶段
  没有 cwd）。
- 跳过重复 `/`；`.` 不改变当前目录；`..` 走父节点，根目录的 `..` 仍是根。
- 尾随 `/` 要求最终节点是目录，否则 `ENOTDIR`。
- 中间分量不是目录 → `ENOTDIR`；缺失 → `ENOENT`；空路径/超长/超分量 → `EINVAL`。
- `O_CREAT` 只创建**末项**；父目录必须存在且可写，否则 `ENOENT`/`ENOTDIR`/`EACCES`。
- `O_EXCL|O_CREAT` 命中已有项 → `EEXIST`；`O_DIRECTORY` 命中非目录 → `ENOTDIR`。
- 只读文件以写 access 打开 → `EACCES`；目录以写 access 打开 → `EACCES`（不新增 `EISDIR`）。
- `O_TRUNC` 只对可写普通文件生效；`O_APPEND` 在每次写前把偏移移到 EOF。
- 未列出的 `O_*`（如 `O_NOFOLLOW`/`O_SYNC`）→ `EINVAL`。

## 4. 线上协议

单条 IPC 只有 64 位 payload，路径上限 64 B，故 `open` 分三段：

| 消息 | 布局（位） | 作用 |
|---|---|---|
| `SERVICE_VFS_OPEN_TAG` | `tag(48..63) \| flags(40..47) \| path_len(32..39) \| start(24..31) \| 前 3 字节(0..23)` | 清空暂存路径并写入首段 |
| `SERVICE_VFS_PATH_TAG` | `tag \| offset(40..47) \| 5 字节(0..39)` | 追加路径片段 |
| `SERVICE_VFS_COMMIT_TAG` | `tag \| client(32..39)` | 解析并回复 |

`OPEN`/`PATH` 只入队、无应答；`COMMIT` 等待一条 `SERVICE_FILE_REPLY_TAG`。服务管理器
逐条同步转发，避免一次灌满 32 条全局消息队列。open 成功应答的 `data` 为
`node(0..7) | kind(8..9) | size(16..31)`；`fd` 为服务端句柄 3..10。

已有 `SERVICE_FILE_REQUEST_TAG` 继续承载 read/write/lseek/close；read/write 每次最多
3 字节，由适配层分片。目录读取复用 `OP_READ`，返回目录字节流（每条记录
`node u16 | kind u8 | name_len u8 | name`），适配层把它翻译成 `linux_dirent64`；
公共 `read(2)` 不把目录当普通文件暴露。

## 5. 授权

管理器在创建 C 任务后，把存储服务的 `TaskSend` 派生令牌绑定到隔离 Namespace 的
`fs` 路径，并**解析一次**；成功后置 `file_authorized`。此后 VFS 消息只检查该授权并
路由，不再逐次解析用户路径。相比 M4 的逐 `open` 解析，Namespace 审计的 `resolves`
从 5 收敛到 4，`binds`/`entries` 不变；这是真实操作数变化，已在报告登记。

## 6. 通用 fd 表

| fd | 初始类型 | 行为 |
|---|---|---|
| 0 | null | `read` → 0（EOF），`write` → 返回长度并丢弃，`lseek` → 0 |
| 1, 2 | tty | 逐字节 `SEND` 到管理器 → tty 服务；`read` 不支持（`EBADF`） |
| 3..18 | 空 | `open` 分配最低空闲项；file/dir 项指向服务端句柄 3..10 |

- 文件/目录项做**服务端句柄引用计数**；最后一个引用关闭时才发 `CLOSE`，所以 `dup*`
  共享 offset、目录游标与打开描述。
- `dup2`/`dup3` 的目标接受 0..18；表满返回 `EAGAIN`（不新增 `EMFILE`）。
- `O_CLOEXEC` 被接受但无 exec 语义；无进程退出自动关闭。
- 目录枚举：`getdents64(217)` 在适配层把目录字节流组装成 `struct linux_dirent64`，
  包含 `.` 与 `..`，`d_type` 为 dir/file/chr；缓冲不足时返回 `EINVAL`，非目录 fd 返回
  `ENOTDIR`。`lseek` 目录 fd 会清空本地缓存并按服务端目录偏移重定位。

## 7. 容量与栈

| 项 | 变化 | 依据 |
|---|---|---|
| `MAX_SEGMENT_FRAMES` | 128 → 136 | 服务映像段实测 130 页（含目录页）；只留 6 页余量。先扩到 192，在驱动故障切换/退出收尾处触发三重故障（具体根因未定位；`SegmentFrames` 按值出现在内核栈帧中，扩容会增加栈占用）；回退到 136 后通过 |
| `MAX_AUDIT_EVENTS` | 2048 → 8192 | V1 探针实测 `audit_used=4120`；M7 已用 1969，多级路径三段式与目录分片必然越界 |
| 服务端点栈 | 保持 1 页 | 通过名称 ≤16、分量 ≤6 把 VFS 解析栈帧压回预算，不改变既有栈布局；未采用 2 页 |
| 服务端句柄 | 4 → 8（fd 3..10） | 多级打开/目录 fd/`dup` 的并发需求 |
| 适配层本地 fd | 8（3..10）→ 19（0..18） | 通用 fd 表含 fd 0/1/2 |

## 8. 边界（不得据此宣称通用文件系统）

- 无符号链接、硬链接、`mkdir`/`rmdir`/`unlink`/`rename`/`chmod`/`stat`/`fsync`；
  无 cwd/`chdir`/`getcwd`；无文件映射、`O_NONBLOCK`、异步 I/O。
- 根树静态；只有 `/tmp` 可新建普通文件；节点/内容上限固定，服务重启即丢失。
- 无进程退出自动关闭 fd；无跨 Task 共享 fd 表；无权限模型（只有节点级 writable 位）。
- `getdents64` 只保证本实现的 `linux_dirent64` 子集；`telldir`/`seekdir` 依赖 `lseek`，
  不承诺稳定 cookie。
- `readdir`/`ls` 等 libc 层路径未做端到端验收；本片只验收原始 `getdents64` 与同源探针。
