# 阶段 5 / M9：`statx` 元数据、`chdir`/`getcwd` 与 sbase `cat`/`ls`

**日期**：2026-10-21
**范围**：在 M8 的用户态 VFS/rootfs 之上，补齐路径元数据（`stat` 家族）与工作目录
（`chdir`/`getcwd`），并用 sbase `cat`/`ls` 做端到端验收。
**结论**：M1（`stage5_meta.c`）在 Prima 双轮通过且与 Linux 参考逐字节一致；sbase `cat`
C1 与 `ls` L1–L3 双轮通过。`P5-FS-04`/`P5-FS-05` 新增为 `partial`，`P5-FS-01`/`P5-FS-03`
只更新边界与证据。未勾选任何 S5-D*/S5-X*。设计与边界见
[profile v1 delta](../specs/2026-10-20-stage5-profile-v1.md)。

## 1. 实现

### 1.1 VFS 协议：无句柄的 STAT

- `SERVICE_VFS_FLAG_STAT`（bit 7）复用 OPEN/PATH/COMMIT 三段式，但只解析路径并回
  `node/kind/writable/size`，**不分配服务端句柄、不做读写权限检查**。应答 op 为
  `SERVICE_FILE_OP_STAT`，不会与 open 的句柄分配混淆。
- `pack_vfs_node` 增加 `writable` 位（bit 10），用于把 0444/0555 与 0666/0777
  映射到 `st_mode`。
- 空路径 + STAT 表示“查询 `start` 节点本身”（`len == 0`），供 `fstat(fd)` 使用。
  这避免了 VFS `walk` 的既有语义——相对路径的 `start` 必须是目录，若用
  `start=文件节点, path="."` 会被判为 `ENOTDIR`。该现象在首轮 M1 中实际出现，
  由 `fstat=0` 暴露，按上述节点请求修复。

### 1.2 适配层：x86_64 musl 走 legacy kstat，不是 statx

**实现中发现的边界**：musl 1.2.5 的 `src/stat/fstatat.c` 在 `SYS_fstatat` 可用时优先走
`kstat` 路径；x86_64 的 `SYS_fstatat` 被映射为 `SYS_newfstatat=262`，且
`struct kstat` 的 `st_atime_sec` 宽度等于 `time_t`，所以 `stat`/`lstat`/`fstat`/`fstatat`
实际调用 **4/6/5/262**，`statx(332)` 只有直接调用 `statx` 的程序才会用到。首轮只实现
`statx` 时 M1 全部 `stat` 位为 0，即由该差异暴露。

适配层因此实现两套填充：

| Linux 号 | 调用 | 入口 | 返回布局 |
|---|---|---|---|
| 4 | `stat` | `AT_FDCWD, path, flags=0` | 144 B `struct kstat` |
| 6 | `lstat` | `AT_FDCWD, path, flags=AT_SYMLINK_NOFOLLOW` | 同上 |
| 5 | `fstat` | `fd, "", flags=AT_EMPTY_PATH` | 同上 |
| 262 | `newfstatat` | `dirfd, path, flags, buf` | 同上 |
| 332 | `statx`（直接调用时） | `dirfd, path, flags, mask, buf` | 256 B `struct statx` |

两端都只填本 profile 冻结的字段：类型、权限、`nlink`、`ino`（节点+1）、`size`、
`blksize=4096`、`blocks=(size+511)/512`；`uid`/`gid`/`dev`/`rdev` 与六个时间戳为 0。
`AT_NO_AUTOMOUNT` 被接受并忽略，`AT_SYMLINK_NOFOLLOW` 在无符号链接时等价，其它 flag
返回 `EINVAL`。

### 1.3 工作目录

- 适配层为每个 C 任务保存一个 `prima_cwd[65]`，初值 `/`；服务端不感知 cwd。
- `chdir(path)` 先把 `path` 与 cwd 合成为词法规范化的绝对路径（消除 `.`/`..`/重复
  `/`，根处钳制），再用 STAT 确认目标是目录；失败不改 cwd。阶段 5 无符号链接，词法
  `..` 与 VFS 解析一致。
- `getcwd(buf,size)` 返回含结尾 NUL 的字节数；`size` 不足返回 `ERANGE`（v1 新增
  `ERANGE=34`）。
- `openat(AT_FDCWD, 相对路径)` 与 `statx/kstat(AT_FDCWD, 相对路径)` 都先把相对路径
  合成为绝对路径再送服务端；`dirfd` 形式仍以该目录节点为起点，语义不变。
- 无 `fchdir`、无 cwd 继承、无 `buf=NULL` 的分配形式；`chdir("")` 与 `open("")` 返回
  `ENOENT`（空路径不做 cwd 合成）；`stat("")` 同样 `ENOENT`。

### 1.4 探针与 sbase 验收

- `prima-user/c/stage5_meta.c`：同一份源码在 Linux（`PRIMA_HOST`，cwd=一次性树）与
  Prima（cwd=`/`）编译。覆盖 `stat`/`lstat`/`fstat`/`fstatat`、普通文件/目录/字符设备、
  0444/0555/0666/0777、`ENOENT`/`ENOTDIR`/空路径、`chdir`/`getcwd`/相对路径/`ERANGE`。
  `/dev/null` 是 Prima 侧字符设备；宿主非 root 不能 `mknod`，故 `null` 位在 Linux 侧
  按“不适用”保留 1 并在 §5 登记。
- sbase `cat`：`cat.c` + `concat.c`/`writeall.c`/`eprintf.c`/`fshut.c`，`cat etc/motd`。
- sbase `ls`：`ls.c` + `libutil/{ealloc,eprintf,fshut,reallocarray,human}.c` +
  `libutf/{rune,runetype,utf,isprintrune,iscntrlrune}.c`。`ls` 默认路径经
  `opendir`/`readdir`（musl 重映射到 `getdents64`）、`stat`/`lstat` 与
  `chdir`/`getcwd`，把 M8 的目录枚举、M9 的元数据与 cwd 串成一条真实程序路径。
- `human.c` 的 `double` 转换在 `x86_64-unknown-none` 的 libgcc 中缺少 `__floatdidf`
  等符号，故 `ls` 构建不启用 `-mgeneral-regs-only`（M3 已启用 ring3 SSE 与
  per-task FXSAVE/FXRSTOR）；`echo` 与其它切片保持原 flag。

## 2. 验证

- `cargo test --workspace`：PASS。
- `cargo xtask lint`：fmt / clippy `-D warnings` / 裸机 `deny(unsafe_code)` /
  `kernel/src` 零 `unsafe` 全 PASS。
- `cargo xtask test`：PASS；既有 probe 两轮、echo E1–E6 各两轮、T1/F1/V1 各两轮，
  新增 M1 两轮、C1 两轮、L1–L3 各两轮全部 PASS。
- Linux 参考：`stage5_meta.c` 用宿主 `cc` 重建并在同形树（0444/0555/0666/0777）中运行，
  退出 0、stdout 与 Prima 逐字节一致；`cat`/`ls` 用同一 sbase 源码在宿主重建，输出与
  Prima 逐字节一致。

M1（Prima 串口原始输出）：

```
META|ok=1;motd=1;hostname=1;readme=1;version=1;etc=1;tmp=1;f=1;null=1;lstat=1;fstat=1;fstatat=1;enoent=1;enotdir=1;empty=1;chdir=1;cwd_abs=1;cwd_etc=1;cwd_parent=1;openrel=1;chdir_file=1;erange=1
ECHO|status=0;len=196;hash=2446796539
```

sbase 用例：

| 用例 | argv（含程序名） | Prima stdout | 管理器上报 |
|---|---|---|---|
| C1 | `cat etc/motd` | `welcome\n` | `status=0;len=8;hash=3237564155` |
| L1 | `ls etc` | `hostname\nmotd\n` | `status=0;len=14;hash=367789108` |
| L2 | `ls .` | `README\ndev\netc\nf\ntmp\nusr\n` | `status=0;len=25;hash=617542093` |
| L3 | `ls etc/motd` | `etc/motd\n` | `status=0;len=9;hash=3662320480` |

双轮 IPC 计数（`queued/direct/blocks`，两轮一致）：

| 用例 | 计数 | `audit_used` | `segment`/`table`/`image` 帧 |
|---|---|---|---|
| M1 | 151/587/595 | 4012 | 171/146/1397 |
| C1 | 51/83/91 | 916 | 176/146/1397 |
| L1 | 59/143/151 | 1244 | 187/146/1397 |
| L2 | 53/193/201 | 1504 | 187/146/1397 |
| L3 | 51/65/73 | 828 | 187/146/1397 |

所有用例的 `TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132`、
`namespace_ready ... resolves=4`、`caps_live=132;caps_peak=140;caps_limit=160` 与
`messages_peak=14;messages_limit=32` 均保持 M8 口径；`frames_ready` 的 image 从 M8 的
1396 增至 1397（内核 BSS 增长 1 页），segment 随 C 映像大小变化（表中为实测值）。

## 3. 容量与布局（实现中发现的约束）

| 常量 | 变化 | 依据 |
|---|---|---|
| `C_HEAP_PAGES` / `PRIMA_HEAP_PAGES` | 64 → 96（brk 16→40，mmap 48→56） | sbase `ls` 在 `main` 入口按 `PATH_MAX` 分配 4096×16 B 的 visited 表；64 页布局的 16 页 brk（64 KiB）容纳不下。此约束先由 boot 在 C 创建后陷入无 Ready 任务暴露，再把 heap 扩到 80 页时确认 `create_service` 因归属表/创建前置失败 |
| `MAX_FRAME_OWNERS` | 256 → 320 | C 任务栈+arena 段从 72 页增至 104 页；归属表按 `184 + 104 = 288` 上界预留 32 项。80 页 heap 的试探（88 页段）正是越过旧 256 上界的最小反例 |
| `MAX_VSPACE_ENTRIES` | 320 → 384 | 同一段增加 32 个逻辑映射；VFS 映像 130 页 + 栈/堆 104 页仍在 384 内 |
| `MAX_SEGMENT_FRAMES` | 136 → 144 | boot manifest 的服务映像段从 136 增至 **137 页**（`ls` C 映像 24 页）。首轮 L1 因此 `install_boot_image` 失败、boot 在 `vspace_ready` 前 panic；只扩 8 页，不重复 M8 试过的 192 |
| `MAX_RECYCLED_FRAMES` | 512 → 640 | 回收栈必须 ≥ `MAX_FRAME_OWNERS + MAX_TABLE_FRAMES = 320 + 256 = 576`；`all_live_frames_can_be_returned_without_filling_the_recycle_pool` 在首次 `cargo test --workspace` 中逐帧拒绝（`NotOwned`），扩到 640 后通过 |
| 服务端句柄 / 本地 fd | 8 / 19 | 不变（M8 口径） |
| 文件服务 errno | 新增 `ERANGE=34` | 仅 `getcwd` 缓冲不足 |

`ls` 的 64 KiB visited 表是**上游实现**的固定行为，本次没有给 sbase 打补丁；选择扩
C arena 而不是改 `PATH_MAX` 或探针侧特判。

## 4. 既有事件变化

| 事件/字段 | 旧（M8） | 新（M9） | 原因 |
|---|---|---|---|
| `frames_ready` image | 1396 | 1397 | 内核 BSS：vspace/owner arena 扩容 |
| `frames_ready` segment/table | 随程序 | M1 171/146、C1 176/146、L1–L3 187/146 | 服务映像段随 C 映像页数变化；表帧不变 |
| `frames_ready` universe/free | 随 ramdisk | 29681/27967（M1）、29678/27959（C1）、29664/27934（L1–L3） | ramdisk 大小随 C 映像变化 |
| 默认 probe 的 `ipc_ready` | 53/156/164 | 53/156/164 | 不变 |
| 默认 probe `audit_used` | 1299 | 1299 | 不变 |
| `namespace_ready resolves` | 4 | 4 | 不变 |

以上均为真实计数；xtask 的断言未放宽。M1/C1/L1–L3 的 `audit_used` 均远低于 8192。

## 5. 冻结边界（`P5-FS-04`/`P5-FS-05` partial）

- `statx`/`kstat` 只填类型、权限、`nlink`、`ino`、`size`、`blksize`、`blocks`；
  时间戳、`st_dev`/`st_rdev`、`uid`/`gid` 恒 0；无 `statx` 的 mount/sync/attribute 字段，
  无 `statfs`/`fstatfs`。
- 目录 `nlink` 固定 2（不按子目录数计算）；`ino` 是 VFS 节点+1，不是持久 inode；
  `dev` 恒 0。
- cwd 是**每任务**的适配层状态：无 `fchdir`、无 fork/exec 继承、无 `chdir` 权限检查
  （rootfs 无权限位）；`..` 按词法回退，依赖“无符号链接”这一 M8 边界。
- 路径 ≤64 B、名称 ≤16 B、分量 ≤6；超出返回 `EINVAL`，不新增 `ENAMETOOLONG`。
- sbase `ls` 只验收默认（非 `-l`）输出；`-l` 依赖 `getpwuid`/`localtime`/`time` 与
  时间戳，未验收；`ls -a`/`-R`/`-F` 未冻结。`cat` 只验收文件参数，stdin 读仍是 fd 0
  的 EOF 语义，未验收交互输入。
- `/dev/null` 的字符设备类型只在 Prima 侧断言；Linux 参考树没有该节点。
- 无 `mkdir`/`rmdir`/`unlink`/`rename`/符号链接/持久化/文件映射/并发写锁；这些仍按
  M8 的 `P5-FS-01` 边界执行。

## 6. 变更文件

- `prima-runtime/src/protocol.rs`：STAT flag、STAT op、writable 位、flags 位宽。
- `prima-user/src/service.rs`：`stat_staged`、空路径节点 STAT、`pack_vfs_node` 调用。
- `prima-user/c/prima_abi.h`、`prima_syscall.c`：kstat/statx、cwd、chdir/getcwd、
  `AT_EMPTY_PATH`、`ERANGE`。
- `prima-user/c/stage5_meta.c`（新）。
- `prima-user/src/servicemgr.rs`、`kernel/src/bootstrap.rs`、`kernel/src/frames.rs`：
  arena/容量常量。
- `xtask/src/image.rs`、`main.rs`、`qemu.rs`：M9 程序、Linux 参考、双轮流程与日志落盘。
- `docs/superpowers/specs/2026-10-20-stage5-profile-v1.md`、本报告与索引/跟踪页。

## 7. 证据命令

```text
cargo test --workspace
cargo xtask lint
cargo xtask test        # 默认 probe ×2、echo E1–E6 ×2、T1/F1/V1 ×2、M1 ×2、C1 ×2、L1–L3 ×2
```

每个托管程序的串口原始流由 xtask 写入 `target/prima-os/<name>.log`（M1/C1/L1/L2/L3、
T1、F1、V1、E1–E6），本报告的计数与 `META|`/`ECHO|` 行均可在其中复核。
