# 阶段 5 / M11：文件树变更（`mkdir`/`rmdir`/`unlink`）

**日期**：2026-10-22
**范围**：在 M8/M9 的用户态 VFS/rootfs 上补齐**单路径**目录创建与文件/空目录删除，
并用同源探针做 Linux/Prima 差分。
**结论**：`mkdir`/`mkdirat`/`unlink`/`unlinkat`/`rmdir` 在 Prima 上可用；D1 双轮通过
且与 Linux 参考逐字节一致；`P5-FS-06` 新增为 `partial`。未勾选任何 S5-D*/S5-X*。
设计与边界见 [M11 设计](../specs/2026-10-22-stage5-m11-file-mutation-design.md)。

## 1. 实现

### 1.1 VFS 模型（`prima-runtime/src/vfs.rs`）

- 删除采用**标记**而不是压缩：节点 `parent = 0xFF`（`FREE_PARENT`）表示空闲槽；
  `lookup`、`dir_entry_count`、`dir_entry` 一律跳过。`kind`/`data` 保留，槽位复用前
  未关闭句柄仍能读到旧内容（存储服务实际会先拒绝删除，见 §1.2）。
- 新增 `alloc_node`：先扫描 `[1, count)` 的空闲槽，没有才追加；`mkdir` 与
  `open(O_CREAT)` 共用。D1 首轮暴露了原 `open` 只看高水位 `count` 的问题——删除后
  节点表高水位仍把创建挡在 `ENOSPC`；改为复用后修复，并新增
  `file_creation_reuses_a_freed_slot` 回归测试。
- 新增 `mkdir`/`unlink(remove_dir)`：
  - 父目录必须存在、是目录且可写；末项不能是 `.`/`..`/根；
  - `rmdir` 要求目标为空目录；`unlink` 拒绝目录与 `/dev/null`；
  - 非空目录/目录 `unlink` 在冻结错误集中没有 `ENOTEMPTY`/`EISDIR`，统一
    `EACCES`（显式偏差）。
- 只读根树（`/etc` 等）不可变更；`/tmp` 可写。删除不回收内容区（首版无内容空闲表）。

### 1.2 存储服务（`prima-user/src/service.rs`）

- 新增 `SERVICE_VFS_MUTATE_TAG`：`op(44..47) | path_len(32..39) | start(24..31) |
  前 3 字节`，随后复用 `PATH` 片段与 `COMMIT`。`OpenStaging` 增加 `op`（0 =
  open/stat，1/2/3 = mkdir/unlink/rmdir）；COMMIT 按 op 分派。
- `mutate_staged` 在删除前扫描服务端句柄表：任一 `open && node == id` 返回
  `EACCES`，避免句柄指向随后被复用的槽。Linux 的“删除后已打开句柄继续读”不在
  本片承诺内。
- 错误映射沿用 M8：`ENOENT=2`、`EACCES=13`、`EEXIST=17`、`ENOTDIR=20`、
  `EINVAL=22`、`ENOSPC=28`。

### 1.3 C 适配层

- 新增 Linux 号：`mkdir=83`、`rmdir=84`、`unlink=87`、`mkdirat=258`、
  `unlinkat=263`；`AT_REMOVEDIR=0x200`。
- `mkdirat`/`unlinkat` 复用 `prima_path_target`：`AT_FDCWD` 相对路径与 cwd 合成，
  `dirfd` 以目录节点为起点；空路径 `ENOENT`、超长 `EINVAL`、未知 flags `EINVAL`。
- `mkdir`/`rmdir`/`unlink` 分别落到 `AT_FDCWD` 的 `mkdirat`/`unlinkat`。
- `rename*`、`link*`、`symlink*` 仍未实现（`ENOSYS`）。

### 1.4 测试接线

- `xtask` 新增 `Stage5Program::Mut`、`build_images_for_mut`、
  `check_stage5_linux_mut` 与 D1 双轮；Linux 参考在一次性 cwd（含 `ro/` 0555）中运行。
- D1 插在 M9 的 L1–L3 与 M10 的 A1 之间，共用既有 tty/ECHO/TASKCAP 证据通道。

## 2. 证据

### 2.1 host 单测

```
cargo test -p prima-runtime vfs
test result: ok. 11 passed

cargo test --workspace
kernel: 406 passed; prima-runtime: 12 passed; 其余全部通过
```

新增覆盖：`mkdir` 成功/重复/只读/缺父/根；`unlink` 文件、`rmdir` 空目录、非空目录
拒绝、目录 `unlink` 拒绝、文件 `rmdir` 拒绝、只读父目录；删除后查找/枚举跳过；
槽位复用（含 `O_CREAT` 复用）。

### 2.2 QEMU（`cargo xtask test`，D1 双轮）

```
MUT|ok=1;mkdir=1;exists=1;stat_dir=1;nested=1;nonempty=1;rmdir=1;unlink_file=1;unlink_dir=1;rmdir_file=1;readonly=1;missing=1;enoent=1;at=1
ECHO|status=0;len=140;hash=3125851182
TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132
CAPACITY|caps_live=132;caps_peak=140;caps_limit=160;segment_slots=41;segment_limit=48;messages_queued=1;messages_peak=14;messages_limit=32;audit_used=3256;audit_limit=8192
```

M5–M10 的全部既有 QEMU 轮（echo E1–E6、T1、F1、V1、M1、C1、L1–L3、A1）在本切片后
继续双轮通过，说明 VFS 节点分配/删除改动未破坏既有探针与 sbase `cat`/`ls`。

### 2.3 Linux/Prima 同源差分

`stage5_mut.c` 同一份源码在宿主 glibc 与 Prima 静态 musl + 适配层上编译运行，
stdout 逐字节等于冻结常量。`nonempty`/`unlink_dir` 是平台冻结期望：Linux 返回
`ENOTEMPTY`/`EISDIR`，Prima 返回 `EACCES`；字段仍相同。

## 3. 容量与既有证据变化

| 项 | 旧（M10） | 新（M11） | 原因 |
|---|---:|---:|---|
| D1 `audit_used` | — | 3256 | 多次 MUTATE/OPEN/CLOSE 与路径分片 |
| D1 `caps_live/peak` | — | 132/140 | 未新增 cap 类型；本地 fd 表复用 |
| D1 `segment_slots` | — | 41 | 未新建 Segment |
| VFS 节点高水位 | 12 | 16 | D1 创建多个节点并复用删除槽；`MAX_NODES` 未改 |
| 新增协议 | — | MUTATE + op | mkdir/unlink/rmdir 共用 PATH/COMMIT |
| 冻结错误集 | — | 无新增 | `ENOTEMPTY`/`EISDIR` 映射为 `EACCES` |

`MAX_NODES`、`MAX_SEGMENT_FRAMES`、`MAX_AUDIT_EVENTS`、`MAX_VSPACE_ENTRIES` 均未调整。

## 4. 已知边界（`P5-FS-06 = partial`）

- 只支持单路径 `mkdir`/`mkdirat`/`unlink`/`unlinkat`/`rmdir`；无 `rename`/`mv`、
  硬链接、符号链接、`linkat`；
- `mkdir` 不创建中间目录（`-p` 由 sbase 逐级调用，未做端到端验收）；
- `mode`/`umask`/`chmod` 不建模：新建目录统一 0777/可写，文件权限仍是 0444/0666；
- 删除不回收内容区；节点表 16 槽，删除后槽位可复用；
- 带打开句柄的节点拒绝删除（`EACCES`），与 Linux 的“删除后句柄继续读”不同；
- 非空目录/目录 `unlink` 用 `EACCES` 代替 `ENOTEMPTY`/`EISDIR`；
- 只读根树不可变更；`/tmp` 是唯一可写目录；
- 持久化只在单次启动内，跨启动仍由静态 rootfs 重新格式化。

## 5. 复现

```sh
cargo test --workspace
cargo xtask lint
cargo xtask test                # D1 双轮 + 全部既有 BIOS 轮
```

产物与日志：`target/prima-os/stage5-mut.elf`、`target/prima-os/D1.log`、
`target/prima-os/mut-linux`。

## 6. 文件

- `prima-runtime/src/vfs.rs`：`FREE_PARENT`、`alloc_node`、`mkdir`、`unlink`、
  查找/枚举跳过与 4 个新单测
- `prima-runtime/src/protocol.rs`：`SERVICE_VFS_MUTATE_TAG`、op 与打包函数
- `prima-user/src/service.rs`：`OpenStaging.op`、`begin_mutate`、`mutate_staged`、
  MUTATE 分派
- `prima-user/src/servicemgr.rs`：MUTATE 纳入 VFS 转发
- `prima-user/c/prima_abi.h`、`prima_syscall.c`：5 个 Linux 号、`AT_REMOVEDIR`、
  MUTATE 请求与路径解析
- `prima-user/c/stage5_mut.c`：D1 同源探针
- `xtask/src/image.rs`、`main.rs`：D1 程序、Linux 参考与双轮
