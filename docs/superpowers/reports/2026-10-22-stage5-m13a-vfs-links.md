# 阶段 5 / M13a：符号链接、rename 与 fchdir 实现报告

**日期**：2026-10-22
**范围**：路线图 M13a（VFS 路径与链接扩展）。设计见
[M13a 设计](../specs/2026-10-22-stage5-m13a-vfs-links-design.md)；M13 的权限、
stdio 文件 API、`chmod`/`umask` 与跨启动持久化**不在**本报告范围。
**结论**：M13a 的代码、host 测试、Linux 同源差分与 QEMU `V2` 双轮均已通过；
M13 整体仍未完成，profile 只按证据更新 `partial` 边界。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-runtime/src/vfs.rs` | `NodeKind::Symlink`；符号链接创建/`readlink`/路径展开（深度 8、`ELOOP`）；`resolve_link`；`rename`（替换/子树/非空/只读/缺失）；内容区压实回收与尾部搬迁；`MAX_NODES` 16→32 |
| `prima-runtime/src/protocol.rs` | 双路径暂存/提交/应答（0x5EDD–0x5EDF）、`readlink` 分段请求/应答（0x5EE0/0x5EE1）、`ELOOP`；`SERVICE_VFS_NODES` 32 |
| `prima-user/src/service.rs` | `PairStaging`/`LinkTarget`；PAIR/PATH/COMMIT/LINK_READ 分派；`readlink` 目标分段回读；`lseek` 对链接句柄 `EINVAL` |
| `prima-user/src/servicemgr.rs` | 管理器原样转发 PAIR/LINK_READ tag，并回送 PAIR/LINK 同步应答（不解释路径） |
| `prima-user/c/prima_syscall.c` | `rename`/`renameat`、`symlink`/`symlinkat`、`readlink`/`readlinkat`、`fchdir`；目录 fd 路径表与 `dup*`/`close` 维护；`d_type` 增 `DT_LNK=10` |
| `prima-user/c/prima_abi.h` | 新 syscall 号、`ELOOP=40`、PAIR/LINK tag 与位移常量 |
| `prima-user/c/stage5_vfs2.c` | 新同源探针（13 个字段 + `ok`/`err`） |
| `xtask/src/image.rs`、`main.rs` | `Stage5Program::Vfs2`、`VFS2_EXPECTED_STDOUT`、Linux 参考运行与 `V2` 双轮接线 |
| `Cargo.toml` | `prima-user`/`prima-runtime` dev profile `opt-level=1`（见 §4） |

未新增内核 syscall、未改 `prima-abi` 号段、未改 COSIR；所有新语义在 ring3
VFS/适配层。`MAX_NODES` 与内容区容量由一页镜像实测决定：节点表 32×32 B，
`CONTENT_CAPACITY` 3048 B。

## 2. 语义边界

- `rename`：文件/链接可替换文件/链接；目录只能替换空目录；目录不能移进自身或
  子树；非空目录/文件覆盖目录沿用冻结错误集（`EACCES`/`ENOTDIR`），不新增
  `ENOTEMPTY`/`EISDIR`。
- 符号链接目标存内容区（≤64 B）；相对目标以链接父目录为基准，绝对目标从根；
  展开深度 8；悬空链接解析 `ENOENT`；未实现 `open(O_CREAT)` 在悬空链接上创建目标。
- `lstat`/`AT_SYMLINK_NOFOLLOW` 仍跟随末项：本切片没有为该 flag 增加独立查询，
  profile 明确登记为 partial 边界。
- 硬链接/`linkat` 未接线，落到 `ENOSYS`；`chmod`/`umask`/权限模型、stdio 文件
  API、跨启动持久化留 M13b/M13c。
- 内容区回收只在写入压力下压实；单文件仍须连续区间，压实后搬尾可能保守地
  要求两倍空间，满盘时返回 `ENOSPC`，不破坏已提交内容。

## 3. 证据

### 3.1 host

```
cargo test -p prima-runtime --lib vfs::
test result: ok. 17 passed; 0 failed

cargo test --workspace
（全部 test result: ok）
```

新增覆盖：符号链接创建/相链/中间分量/环（深度上限）/悬空、`readlink` 原文、
`resolve_link` 不跟随末项、`rename` 文件/目录/链接/替换/子树/非空/只读/缺失/
尾随斜杠、内容压实回收、32 槽边界。

### 3.2 Linux / Prima 同源差分

`stage5_vfs2.c` 在宿主参考树（`target/prima-os/vfs2-linux-run`）与 Prima 上
逐字节比较 stdout：

```text
VFS2|sym=1;chain=1;ren=1;renlink=1;dir=1;subtree=1;nonempty=1;fchdir=1;ro=1;loop=1;type=1;trunc=1;missing=1|ok=1;err=0
```

Linux 参考运行 `PASS linux stage5 symlink/rename/fchdir references`，退出码 0；
QEMU `V2` 双轮输出与 `VFS2_EXPECTED_STDOUT` 完全一致。

### 3.3 QEMU 全量回归

```text
cargo xtask test
```

（结果见本节末；M13a 新增 V2 双轮，既有 E1–E6/T1/F1/V1/M1/C1/L1–L3/D1/A1 全部重跑。）

容量与计数变化：

- V2 的容量记录为 `[17, 0x5a, 0, 1, 1]`，`ECHO|status=0;len=<行长度>;hash=<...>`；
- 用户态 crate 启用 `opt-level=1` 后服务映像段约 86 页（含目录页），默认探针约 105 页、
  `ls` 约 112 页、V2 约 98 页，均低于 `MAX_SEGMENT_FRAMES=149`；
- 未改 `MAX_CAPABILITY_SLOTS`/`MAX_AUDIT_EVENTS`/`MAX_VSPACE_ENTRIES`/`SHARED_RECORDS`；
- `SERVICE_VFS_NODES` 与 `vfs::MAX_NODES` 同步为 32。

全量结果（68 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 run=1 / run=2
PASS qemu bios T1 / F1 / V1 / V2 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 C main / echo / time / fd / vfs / meta / cat-ls / mut / mem
PASS linux stage5 symlink/rename/fchdir references
PASS stage1 boot slice
```

其中 `V2` 两轮的 stdout、退出码与容量记录 `[17, 0x5a, 0, 1, 1]` 均通过；
既有轮次无回归。

## 4. 构建容量说明

M13a 的服务端代码使 debug（`-O0`）的 `prima-log`/`prima-ns`/`prima-store` 从 21 页
增至 29 页，超出 boot manifest 单段上限（149 页），启动在
`install_boot_image` 阶段被确定拒绝。处理方式不是继续放宽内核上限（M8 记录过
192 页在驱动故障切换处触发三重故障），而是只对 `prima-user`/`prima-runtime`
启用 `opt-level=1`：内核与 `prima-ostd` 的编译参数、`deny(unsafe_code)`、
`SegmentFrames` 逐帧归属检查都不变。这是一次**容量缓解**，不是性能目标，也不
改变证据范围。

另记一条构建工具事实（不影响运行语义）：本仓库用 Cargo `bindeps` 把用户态
bin 作为 `xtask` 的 artifact dependency。`cargo build -p prima-user` 单独重建后，
`target/.../deps/artifact/prima-user-*/bin/*` 的副本可能仍是旧的，导致 QEMU 装载
旧服务而出现“改了代码却没生效”。重建前删除
`target/x86_64-unknown-none/debug/deps/artifact` 与对应
`.fingerprint/prima-user-*` 可确保一致。

## 5. 已知限制与后续

- `lstat` 对符号链接返回目标元数据（跟随），不是链接自身；
- 悬空链接上的 `open(O_CREAT)` 未实现为创建目标；`O_NOFOLLOW` 未支持；
- 硬链接/`linkat`、`chmod`/`fchmod`/`umask`、权限模型、stdio 文件 API、
  跨启动持久化未做（M13b/M13c）；
- 符号链接目标 ≤64 B、展开待处理分量 ≤16、深度 8；`chdir("../..")` 仍是词法回退；
- D8 未组织级关闭；本报告不把它视为已关闭，也不升级任何 `supported` 行。

## 6. 复现命令

```sh
cargo test --workspace
cargo xtask test
# 仅 M13a 同源探针的 Linux 参考：
cargo test -p xtask --no-run   # 由 xtask test 内部调用 check_stage5_linux_vfs2
```

## 7. 本报告不代表

- 不表示 M13 完成（权限/stdio/持久化仍未做）；
- 不表示阶段 5 退出条件或 S5-D* 已满足；
- 不表示 D8 已关闭或 profile v1/v2 已发布。
