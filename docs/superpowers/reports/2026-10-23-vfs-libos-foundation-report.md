---
id: report-vfs-libos-foundation
kind: report
authority: evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-23
---

# VFS / LibOS 基础重构验证报告

范围与行为契约见[设计](../specs/2026-10-23-vfs-libos-foundation-design.md)，执行顺序见[计划](../plans/2026-10-23-vfs-libos-foundation-plan.md)。基线是文档重构后的 `9a311ed`；本报告是 M11/M13a/M13b 的实现后继证据，不回写或抹去当时的限制与测试结果。

## 变更与证据范围

`prima-runtime::vfs::OpenFiles` 从服务循环提取打开对象；服务只负责编解码。新增原生 AccessMode/SeekFrom/FileInfo/HandleId，移除服务内重复的句柄操作。节点 v3 增加打开引用，区分有名节点、孤立但打开对象、空闲槽。路径、稀疏写、表满与权限检查按设计收紧；原测试迁入 `vfs/tests.rs`。

**结论**：实现、host 回归、lint 与 Linux 同源参考完成；V1 双轮、V2/V3 单轮整机验收有成功记录，QEMU 双轮/全量回归因多处超时未完成，不能视为已通过全部提交检查。

这是为多个 OS personality 提供可复用对象语义的基础，不是已完成多 LibOS、持久化文件系统或 Windows/BSD 兼容。内核原语、syscall 号、线上 tag、节点数、内容区和所有内核容量常量不改。

## 验证记录

| 命令 / 检查 | 当前结果与范围 |
|---|---|
| `cargo test --workspace` | PASS；876 个测试执行项（含不同 target，不是互异性质计数）；runtime 31 项，其中 VFS 30 项，保留原 19 项并新增 11 项 |
| `cargo xtask lint` | PASS；fmt、Clippy -D warnings、裸机内核构建、29 个内核源文件零 unsafe |
| V3 宿主独立重建 | `cc -DPRIMA_HOST -Wall -Wextra -Werror`；一次性目录运行，退出 0，新四项均为 1 |
| `cargo xtask test` | 两次全量尝试分别在 V2 / E3 启动超时；未全量通过，失败记录见下 |
| `cargo xtask test-vfs` | V1 双轮 PASS；V2 首轮 PASS，第二轮探针输出成功但后续整机阶段超时；整个命令失败 |
| `cargo xtask test-vfs V3` | Linux 参考 PASS；QEMU 首轮 PASS，第二轮在 bootloader 映射阶段超时；再次尝试首轮空输出超时。未达到双轮门槛 |
| 文档 self-test/build/check、diff whitespace | PASS；报告与索引更新后重新生成并校验 |

最终 host/lint 在新增测试命令后重跑通过；测试项数不因 CLI 重构改变。

Host 原始日志：`/tmp/prima-vfs-workspace.log`、`/tmp/prima-vfs-lint.log`；启动回归输出：`/tmp/prima-vfs-qemu.log`。这些是本机运行文件，不是仓库内持久工件；本报告保留摘要和失败记录。

新增 host 回归按边界分组：表满与 255 引用上限不产生 truncate/create 副作用；两个表共享孤立节点直至最后关闭；rename/链接删除；压实保留孤立内容；访问模式/独立游标/append/chmod；目录父关系；空写/稀疏写；路径负例；I/O 失败与 close 重试；挂载错误与服务重启回收。

## 新 V3 的已观察证据

首轮完整运行输出（退出 33 且标准验收函数返回成功；不代表双轮 IPC 一致性已经建立）：

```text
VFS3|umask0=1;umask077=1;chmod=1;readonly=1;stdio=1;stdio_ro=1;missing=1;unlink_open=1;rename_open=1;unlink_link=1;sparse=1|ok=1;err=0
ECHO|status=0;len=135;hash=211250902
ipc_ready: queued=425;direct=733;blocks=741
CAPACITY: audit_used=5171;audit_limit=8192;caps_live=132;caps_peak=140
```

服务退出后的打开句柄清理仍通过原验收；未扩大任何容量。原始串口在 `target/prima-os/V3.log`，构建产物 `target/prima-os/stage5-vfs3.elf` 的 SHA-256 为 `b9691237a31c3a335a9b0e252c3189e9bc3f723e556eb2c3c92bb0592ca43e01`。这些 target 工件被 Git 忽略，重新构建可能覆盖；可用 `cargo xtask test-vfs V3` 重放。

## 实际发现与修复

- 原句柄分配在 open 后执行，表满时 create/truncate 的副作用已发生；改为前置容量校验，host 比较全镜像字节不变。
- rename 释放被替换节点，导致旧目标句柄读到清空或复用内容；引入打开引用与孤立节点，unlink 同样按最后关闭回收。
- 原 unlink 的服务预检查跟随末项链接，错误拒绝删除指向已打开对象的链接；移除该预检查，由文件系统操作链接目录项。
- 新压实回归首次返回 NoSpace：空文件的旧 data 偏移被 relocate 当成活跃区间。修复为只把非空存活内容计入尾部，回归成功；不是扩大容量解决。
- 空写原先可能增长文件，稀疏写可能暴露截断前字节；分别提前返回、逐块清零。
- `/file/..`、链接目标尾随 `/`、内嵌 NUL、源目录只读 rename、悬空链接独占创建缺少检查，新增负例与修复。
- 第一轮 lint 指出范围索引循环与测试的无 Drop 类型显式 drop，已调整实现/测试并复跑。
- 第一次 QEMU 命令因 worktree 缺少 git-ignored musl 快照终止，未开始启动验证。核对主 checkout 的干净快照 commit/COPYRIGHT hash 与锁文件一致后复制到本 worktree 的 target/；sbase 同样核对锁定 commit、干净状态后复制，不下载或改动上游源码。

- 首轮全量回归在默认、E1–E6、T1、F1、V1 双轮通过后，于 V2 第一轮 30 秒超时；stdout/stderr 都为空。同一镜像 35 秒诊断重跑有输出但只到内核 cow_ready（文件系统探针尚未开始），也被终止；该诊断不计为验收通过。随后沙箱外全量重跑在默认、E1/E2 双轮通过后，于 E3 第一轮同样空输出超时。未修改超时或放宽断言；确切超时根因未定位。主分支既有镜像的一次对照启动正常结束（退出 33），不能据此证明超时与改动无关。

- `test-vfs` 首次运行：V1 双轮成功，V2 首轮成功、第二轮已输出 `VFS2...ok=1;err=0` 和 `ECHO|status=0`，但在后续内核事件约 seq=36 时超时。V3 单独运行：首轮通过全部原始验收，第二轮在 bootloader 映射阶段超时；重试首轮空输出超时。所有超时都保留为失败，根因尚未定位，不能宣称与代码无关。
- 将已有 V1/V2/V3 三段重复循环提取为全量与 `test-vfs [V1|V2|V3]` 共用的函数；专项不是替换断言或跳过其中某轮，仍各运行两轮、比较 stdout/退出码/IPC/容量等原检查。

## 已知限制

普通文件的打开后删除与替换已纳入验证；打开目录仍拒绝删除/替换；C stat 的 nlink 仍是固定值，未验证完整 unlink 元数据语义。C 预先折叠 `..` 可与符号链接/中间非目录的后端逐分量语义不同；host 路径负例不代表 C 端已完整消除这类偏差。fchdir 路径缓存、fork fd close 隔离和子任务授权限制不由本切片消除。QEMU 超时的原始日志另见 `/tmp/prima-vfs-qemu-retry.log`、`/tmp/prima-vfs-focused.log`、`/tmp/prima-vfs-v3.log`、`/tmp/prima-vfs-v3-retry.log`，诊断与对照见 `/tmp/prima-vfs-v2-retry.log`、`/tmp/prima-vfs-baseline-control.log`。

任意中途 Storage 故障的事务回滚、断电恢复、持久化、多后端与多 personality 均无证据；不扩大 profile supported 声明、不关闭 BLK-003、不签署 M21。
