---
id: spec-vfs-libos-foundation
kind: spec
authority: design
status: implementation-contract
lifecycle: current
updated: 2026-10-23
---

# VFS 打开对象与 LibOS 适配边界

## 问题与范围

本切片遵循[白皮书 §7/§11](../../../Prima%20OS.md)：文件系统在用户态，LibOS 负责 OS personality，内核不识别文件、Linux fd 或 Windows handle。前序为 [M8](2026-10-20-stage5-user-vfs-design.md)、[M11](2026-10-22-stage5-m11-file-mutation-design.md)、[M13a](2026-10-22-stage5-m13a-vfs-links-design.md)、[M13b](2026-10-22-stage5-m13b-permissions-stdio-design.md)，执行与结果见[计划](../plans/2026-10-23-vfs-libos-foundation-plan.md)和[报告](../reports/2026-10-23-vfs-libos-foundation-report.md)。

原实现把打开对象的访问模式、游标、append 和回收放在无 host 单测入口的服务循环中。表满在 create/truncate 之后检查，rename 替换的目标可被提前回收，unlink 通过跟随末项链接的预检查拒绝打开目标。本切片抽出实际使用的共享语义，并修复这些缺陷；不创建尚无第二个实现的后端 trait，不引入依赖。

这是当前实现的增量契约，替代 M11“所有打开节点拒绝删除”与 M13a“替换立即释放目标”在**普通文件**上的行为。profile 发布、阶段签署与 BLK-003 状态不随此文改变。

## 分层与接口

| 层 | 职责与入口 | 不承担的职责 |
|---|---|---|
| OS personality | `prima_syscall.c`：Linux 号、flags、errno、fd/dup、cwd/umask、stat/dirent 布局 | 不直接操作镜像节点/内容区 |
| 服务传输 | `service.rs`：分片暂存、原生 flag 解码、句柄编号偏移、回复编码与错误映射 | 不维护第二份打开对象语义 |
| 原生打开对象 | `vfs/handles.rs`：`OpenFiles<N>`、`HandleId`、`FileInfo`、`SeekFrom`；调用现有 RAM 后端 | 不知道 Linux fd 0/1/2、OS syscall 号或 errno 数值 |
| RAM 文件系统 | `vfs.rs`：路径、节点、目录项、权限检查、内容区与延迟回收；`Storage` 提供字节读写 | 不拥有进程/fork/exec、Capability 授权或挂载策略 |

`AccessMode` 是 ReadOnly/WriteOnly/ReadWrite 的类型，`OpenFlags` 表达规范化意图。这是显式记录的 0.x 源码调整：`OpenFlags.access` 从 u8 改为 AccessMode，Node 的打开引用字段由内部管理；调用方需要重编译并迁移构造方式，不承诺 Rust 二进制 ABI 兼容。原始 OS flags 在 C 中校验并转为现有线上协议；原生协议的 2 位 access 在服务边界解码，3 被拒绝。没有把 Linux 的 flag 数值固化到原生对象 API。

`FileInfo` 只含 id/kind/size/mode，不暴露内容区偏移和镜像引用计数。`HandleId` 是服务内索引，**不是 Capability 或安全句柄**；不同客户端的认证、表选择和授权仍由服务/管理器负责。多个 OpenFiles 表可在同一镜像上串行调用；不宣称当前线上协议已实现多个互不信任的 LibOS。

## 成功、失败与生命周期

- 一个表项对应一个打开对象（独立游标、访问模式、append）。重复 open 分配不同对象；dup 由已有 C fd 表继续引用同一服务句柄，最后一个 fd 别名关闭才发送 close。
- 表容量先于 create/truncate 检查，表满返回 `TooManyOpen`，不创建文件或截断旧内容。单节点引用计数达到 255 时，同样在 truncate 之前拒绝打开。
- open 检查 owner 读/写权限；已打开对象后续按授予的 access 读写，chmod 不撤销该 access。仍是单用户权限子集，不实现 uid/gid、ACL 或共享/删除模式。
- 普通文件 unlink 立即移除名字；仍打开的对象保留节点、内容和元数据。rename 替换打开的普通文件也保留旧对象；新路径指向源节点。符号链接 unlink 操作链接本身，不受其目标是否打开影响。
- 打开目录暂不允许 rmdir 或被 rename 替换（Access），避免丢失 dirfd 的父关系。目录自身移动仍保留 NodeId；C 缓存路径的 fchdir 限制仍在。
- 最后一个打开引用 close 后，已移除名字的节点才成为空闲槽。压实/增长分配必须保留这些孤立对象的内容；不把空文件旧 data 偏移误算成已分配空间。
- read/write 失败不推进游标；close 只有镜像引用更新成功才释放表项。`close_all` 在首个 I/O 错误停止，剩余项可重试。表不实现 Clone/Copy，不能通过复制表复制引用；无隐式 Drop I/O，调用者须显式清理。
- Storage 的单次 write 必须全写成功或保持不变返回 false，不能把部分写入报告为 false；当前实现是已映射、串行访问的 RAM。Storage 不提供多次 write 的事务。多步 create/rename/压实/写入遭遇任意中途 I/O 失败时，**不承诺全镜像回滚或崩溃一致性**；保留原有 RAM 服务故障模型，不将本切片写成持久化事务实现。

原生 `BadHandle`、`TooManyOpen` 不携带 errno。现有协议边界分别保持 EBADF、EAGAIN；其它冻结错误映射不改。未来 personality 可选择自己的状态码，但必须先扩展并协商服务协议，而非把本地索引视作权限证明。

## 镜像、启动与回收

v3 继续使用 16 B 头、32 × 36 B 节点和 2920 B 内容区；节点 byte 3 保存 u8 打开引用，`parent=0xFE` 表示已解除目录链接但仍被引用，`0xFF` 表示可复用。节点编号不因压实变化。不扩大内核或服务容量常量。

`mount` 只对全零头的新段自动格式化；非零旧版本头、无效头和读取错误不会静默覆盖。`format` 是显式破坏性重建入口；旧 v2 镜像没有自动升级。当前服务每次全新启动仍从零段获得 rootfs，不是磁盘格式升级工具。

`recover_open_files` 仅在服务启动、旧打开表已全部失效时调用：保留有目录链接的文件并清引用，回收孤立对象。它不是打开句柄迁移协议，不能与另一活动表同时使用。普通 mount 不重置活动表的引用。

## 路径与数据边界

- 路径字节数 ≤64、名称 ≤16、分量 ≤6；拒绝内嵌 NUL。每个分量（包括 `.`、`..`）之前都要求当前对象为目录；`/file/..` 不能绕过类型检查。
- 符号链接展开保留目标的尾随 `/` 目录要求；展开缓冲只计尚待处理的分量，深度仍为 8。独占创建检查末项目录项，悬空链接也返回 Exist。缺失普通文件带尾随 `/` 的创建被拒绝。
- rename 同时检查源与目标父目录的写权限。只读源目录不能通过移出节点绕过保护。
- 空写返回 0，不扩展文件、不修改游标；稀疏写把旧 EOF 到写入起点之间清零，截断前残留数据不能重现。镜像读取检查内容区边界与父编号编码，不声称完成任意恶意镜像的全面验证。

## 验证与后续

host 回归覆盖两个表共享节点、独立游标/访问模式/append、表满无副作用、最后关闭回收、rename 替换、压实保留孤立内容、服务重启回收、I/O 错误、路径错误和稀疏写。`cargo xtask test-vfs` 复用全量命令的 V1/V2/V3 验收循环。V3 同源探针新增 unlink_open/rename_open/unlink_link/sparse，要求 Linux 与 Prima stdout 逐字节一致、QEMU 双轮成功。执行证据只登记到[报告](../reports/2026-10-23-vfs-libos-foundation-report.md)。

未来顺序：先完成带会话身份和版本协商的服务协议、每 LibOS 的根视图及路径授权，再引入第二个实际 personality 来检验语义边界；第二个实际文件系统后端出现后再提炼后端接口。Windows/BSD 等的路径、删除/共享策略、大小写、锁、元数据扩展由各自 personality/profile 明确定义。本切片不提供这些兼容实现。

C stat 的 nlink 仍是固定值（没有解除链接后变 0 的完整元数据语义）。持久化/日志、硬链接、挂载/跨文件系统 rename、文件映射、异步 I/O、稳定目录 cookie，以及 fork fd close 隔离、子任务 VFS 授权仍是未覆盖项。固定内容区搬迁可能需要额外连续空间，不能把所有空闲字节都承诺为可原地增长容量。
