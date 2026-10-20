# 阶段 5 / M8：用户态 VFS/rootfs、通用 fd 与多级路径

**日期**：2026-10-20
**范围**：把 M4/M7 的单文件 fd 切片推进到用户态 rootfs、统一 fd 表、多级路径与目录枚举。
**结论**：Linux 与 Prima 同源 VFS 探针逐字节一致，V1 双轮 QEMU PASS；`P5-FS-01`/`P5-FD-01`
仍为 `partial`（边界见 §5），新增 `P5-FS-03 getdents64` 为 `partial`；未勾选任何
S5-D*/S5-X*。设计与线协议见 [M8 设计](../specs/2026-10-20-stage5-user-vfs-design.md)。

## 1. 实现

### 1.1 纯逻辑 VFS（`prima-runtime/src/vfs.rs`）

- 1 页段上的镜像：16 B header、16 × 32 B 节点、3560 B 内容区；节点 0 固定为根。
- 静态根树：`/f`、`/README`、`/etc/{motd,hostname}`、`/usr/lib/version`、`/tmp`、
  `/dev/null`。`/tmp` 可写，其余文件/目录只读。
- `Storage` trait 把字节读写抽出来：host 单测用内存数组，ring3 存储服务用已映射的
  MemorySegment。7 个 host 单测覆盖镜像幂等、多级/`.`/`..`/尾随 `/`、错误码、创建/
  截断/追加、目录记录与容量上限。

### 1.2 存储服务（`prima-user/src/service.rs`）

- 收到 SPEC-112 持久化段后，在偏移 8 起 `vfs::mount`；`0..8` 状态字保持 SPEC-110/112 语义。
- 服务端句柄扩为 8 个（fd 3..10），每项保存 `node/offset/access/append`；offset 与目录
  游标归服务端，`dup` 共享。
- open 用 `OPEN`（前 3 字节）+ `PATH`（每段 5 字节）+ `COMMIT` 三段式；OPEN/PATH 只入队，
  COMMIT 同步回复 `node/kind/size`。
- read/write 每次 3 字节；目录 `OP_READ` 返回 `node:u16|kind:u8|len:u8|name` 字节流；
  `O_CREAT`/`O_EXCL`/`O_TRUNC`/`O_APPEND`/`O_DIRECTORY` 在 VFS 内实现。
- 错误映射只用冻结码：`ENOENT`/`ENOTDIR`/`EACCES`/`EEXIST`/`EINVAL`/`ENOSPC`/`EBADF`/
  `EAGAIN`/`EFAULT`/`ENOSYS`。

### 1.3 服务管理器（`prima-user/src/servicemgr.rs`）

- 绑定路径从 `f` 改为 rootfs 授权入口 `fs`；C 创建后解析**一次**并置 `file_authorized`。
  Namespace 审计 `resolves` 5→4、`binds`/`entries` 不变；内核 `NsWitness::evidence_ok`
  与 xtask 的 `namespace_ready` 校验同步更新。
- 管理器只转发 VFS 消息；`OPEN`/`PATH` 不设应答槽，`COMMIT` 设置 `file_client` 并按
  client id 回送。不再解析用户路径，不是 VFS。

### 1.4 适配层（`prima-user/c/prima_syscall.c`、`prima_abi.h`）

- 通用本地 fd 表 0..18：fd 0 null、1/2 tty、文件/目录项指向服务端句柄；`dup2`/`dup3`
  目标接受 0..18，最后引用才关闭服务端句柄。
- `openat`/`open` 支持绝对/相对路径、`dirfd` 起始目录、多级路径与
  `O_CREAT/EXCL/TRUNC/APPEND/DIRECTORY`；路径三段式发送。
- `getdents64(217)` 把服务端目录字节流组装为 `linux_dirent64`，含 `.`/`..`、`d_type`
  与按 8 字节对齐的 `d_reclen`；目录缓存按服务端句柄索引，`dup` 共享。
- 公共 `read` 对目录返回 `EBADF`，目录只经 `getdents64` 暴露；fd 0 读 EOF、`/dev/null`
  读 EOF/写丢弃。

### 1.5 探针（`prima-user/c/stage5_vfs.c`）

同一源码在 Linux（`PRIMA_HOST`，`ROOT=""`，cwd 为一次性目录）与 Prima（`ROOT="/"`）编译：

| 位 | 检查 |
|---|---|
| `motd` | `/etc/motd` 多级绝对路径、8 字节内容、EOF |
| `multi` | `usr/./lib/../lib/version` 相对多级路径 |
| `parent` | `etc/../etc/hostname` |
| `dirfd` | `openat(dirfd,"motd")` 与 `openat(dirfd,"../README")` |
| `dupdir` | `dup` 目录 fd、关闭原 fd 后仍可 `openat` |
| `enoent`/`enotdir` | 缺失路径、文件下钻、`O_DIRECTORY` 命中文件 |
| `eacces` | 只读文件 `O_WRONLY`（root 用户跳过该位并登记） |
| `eexist` | `O_CREAT\|O_EXCL` 命中已有项 |
| `create` | `/tmp/scratch` 创建、写入、关闭后重开读回 |
| `append` | `/f` 截断写 `abcdef`，`O_APPEND` 写 `Z`，读回 `abcdefZ` |
| `null` | `/dev/null` 读 EOF、写返回 3 |
| `getdents` | `/etc` 恰有 `motd`、`hostname` |
| `fd0` | fd 0 读 EOF |

Prima stdout：

```
VFS|ok=1;motd=1;multi=1;parent=1;dirfd=1;dupdir=1;enoent=1;enotdir=1;eacces=1;eexist=1;create=1;append=1;null=1;getdents=1;fd0=1
```

管理器上报 `ECHO|status=0;len=129;hash=2189770559`。

## 2. 验证

- `cargo test --workspace`：PASS（含 `prima-runtime` 的 7 个 VFS host 单测）。
- `cargo xtask lint`：fmt / clippy `-D warnings` / x86_64 裸机 `deny(unsafe_code)` /
  `kernel/src` 零 `unsafe` 全 PASS。
- `cargo xtask test`：PASS；既有 probe 两轮、echo E1–E6 各两轮、T1 两轮、F1 两轮、
  新增 V1 两轮全部 PASS。
- `check_stage5_linux_vfs` 用宿主 `cc` 重建同一源码，在一次性 cwd 中布置同形树并
  `stdin=/dev/null` 运行；stdout 与 Prima 逐字节一致、退出码 0。
- V1 双轮 IPC 计数一致：`queued=157;direct=633;blocks=641`。
- V1 内核侧：`TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132`；
  `namespace_ready ... entries=3;binds=4;resolves=4;evidence_ok=1`；
  `CAPACITY|caps_live=132;caps_peak=140;caps_limit=160;segment_slots=41;
  segment_limit=48;messages_queued=1;messages_peak=14;messages_limit=32;
  audit_used=4120;audit_limit=8192`；`state_audit_ready used=4120;cap=8192`。
- 默认 probe 的同步文件 IPC 计数变为 `queued=53;direct=156;blocks=164`，
  `audit_used=1299`；`namespace_ready resolves=4`。
- 产物：`target/prima-os/stage5-vfs.elf` SHA-256
  `85d91f2b1f5a27e95a3dc42325e0bb5e88ac0b38879c66111a88617a06438762`（最终构建后重算）。

## 3. 容量与栈（实现中发现的布局约束）

| 常量 | 变化 | 依据 |
|---|---|---|
| `MAX_SEGMENT_FRAMES` | 128 → 136 | 服务映像段实测 130 页（含目录页）。先扩到 192，在驱动故障切换/退出收尾处触发三重故障；回退到 136（6 页余量）后通过。具体根因未定位；`SegmentFrames` 按值出现在内核栈帧中，扩容会增加栈占用，故不把 192 当作可用配置 |
| `MAX_AUDIT_EVENTS` | 2048 → 8192 | V1 实测 `used=4120`；M7 已用 1969，三段式 open 与目录分片必然越界 |
| 服务端点栈 | 维持 1 页 | 把 VFS 名称/分量上限压到 16/6，解析栈帧回到 1 页预算；未采用 2 页，也未单独验证 2 页 + 136 的组合 |
| 服务端句柄 | 4 → 8 | 目录 fd、并发 open 与 `dup` 引用 |
| 本地 fd | 8（3..10）→ 19（0..18） | 通用 fd 表含 0/1/2 |

## 4. 既有事件变化（逐条登记）

| 事件/字段 | 旧（M7） | 新（M8） | 原因 |
|---|---|---|---|
| `ipc_ready` 默认 probe | 45/152/160 | 53/156/164 | open 的 OPEN/PATH/COMMIT 与目录 IPC |
| `ipc_ready` V1 | — | 157/633/641 | VFS 探针的多次 open/read/getdents |
| `namespace_ready` resolves | 5 | 4 | 授权由逐 open 改为创建后一次 |
| `CAPACITY` caps_peak | 141 | 140 | VFS 探针的实际句柄峰值 |
| `CAPACITY` audit | 1268/2048 | 1299/8192（默认）、4120/8192（V1） | 真实 IPC/迁移事件增长 |
| `state_audit_ready` used/cap | 1268/2048 | 1299/8192（默认） | 同上 |
| `frames_ready` image/free | 1201/28225 | 1403/28273 | 内核镜像/BSS 增长 |
| `self_root_ready` pages | 423 | 423 | 不变 |

以上均为真实计数变化，不是断言放宽；xtask 的默认 probe 与 V1 校验同步更新。

## 5. 冻结边界（P5-FS-01 / P5-FD-01 / P5-FS-03 partial）

- 根树静态；只有 `/tmp` 可新建普通文件；无 `mkdir`/`rmdir`/`unlink`/`rename`/`stat`/
  `fsync`/`chmod`；无符号链接/硬链接；无 cwd/`chdir`/`getcwd`。
- 名称 ≤16 B、路径 ≤64 B、分量 ≤6；超限返回 `EINVAL`，不新增 `ENAMETOOLONG`。
- 目录以写 access 打开返回 `EACCES`；公共 `read`/`write` 不把目录当普通文件（`EBADF`）。
- 服务端 fd 3..10（8 个并发）；本地 fd 0..18；表满 `EAGAIN`；无 exec，`O_CLOEXEC`
  无语义；无进程退出自动关闭；无跨 Task fd 共享。
- `getdents64` 只承诺本实现的 `linux_dirent64` 子集；`d_off` 不作为稳定 cookie；
  `telldir`/`seekdir` 未验收；`readdir`/`ls` 的 libc 路径未做端到端验收。
- 无文件映射、`O_NONBLOCK`、异步 I/O、并发写锁；服务重启/任务退出后 rootfs 丢失。
- 管理器只做一次授权与路由；Namespace 不是文件树。

## 6. 变更文件

- `prima-runtime/src/vfs.rs`（新）、`protocol.rs`、`lib.rs`。
- `prima-user/src/service.rs`、`servicemgr.rs`。
- `prima-user/c/prima_abi.h`、`prima_syscall.c`、`stage5_vfs.c`（新）。
- `xtask/src/image.rs`、`main.rs`、`protocol.rs`。
- `kernel/src/bootstrap.rs`（容量与审计）、`kernel/src/syscall.rs`（Namespace 证据）。
- `docs/superpowers/specs/2026-10-20-stage5-user-vfs-design.md`、
  `docs/superpowers/specs/2026-10-20-stage5-profile-v1.md`、本报告与索引/跟踪页。

## 7. 证据命令

```text
cargo test --workspace
cargo xtask lint
cargo xtask test        # 默认 probe ×2、echo E1–E6 ×2、T1 ×2、F1 ×2、V1 ×2
```

QEMU 串口原始证据（V1，`target/prima-os/vfs.log`）：

```text
VFS|ok=1;motd=1;multi=1;parent=1;dirfd=1;dupdir=1;enoent=1;enotdir=1;eacces=1;eexist=1;create=1;append=1;null=1;getdents=1;fd0=1
ECHO|status=0;len=129;hash=2189770559
TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132
PRIMA|v=1|seq=30|event=namespace_ready|status=ok|data=namespaces=4;entries=3;binds=4;unbinds=2;mounts=2;unmounts=1;resolves=4;cycles_rejected=1;overlaps_rejected=1;invalid_path_rejected=1;acyclic_ok=2;create_ok=1;resolve_ok=1;overlap_rejected=1;cycle_rejected=1;evidence_ok=1
CAPACITY|caps_live=132;caps_peak=140;caps_limit=160;segment_slots=41;segment_limit=48;messages_queued=1;messages_peak=14;messages_limit=32;audit_used=4120;audit_limit=8192
FAULT|task=15;addr=0xa1000000000;err=0x6;faulted=1
```
