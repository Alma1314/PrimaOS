---
id: report-stage1-5-regression-remediation-20261031
kind: report
authority: scoped-evidence
status: validated-with-limits
lifecycle: historical
updated: 2026-10-31
---

# 阶段 1～5 复核与整改

## 结论与范围

本次在独立工作树复核阶段 1～5 的签署条件与后继整改链，修复用户态 RAM VFS 的跨文件内容覆盖，musl `--check` 未审计归档时仍报告 PASS，以及聚焦测试依赖既有输出目录的缺陷。不修改阶段 6，也不扩大发布的 POSIX profile v2。

## 基线与输入

基线 `298bf55c46e2ba9c52b6730368a5e547d1d4f7ea`，开始时工作树干净、分支为 detached HEAD。
契约及发现来源：

- [阶段跟踪](../../../Prima%20OS%20实现阶段.md)：S1～S5 的交付与退出检查。
- [阶段1～4复核](2026-10-18-stage1-4-implementation-review.md)、[SPEC-151](2026-10-18-stage1-open-issues-spec151.md)、[阶段2 IPC整改](2026-10-18-stage2-ipc-review-remediation.md)、[阶段3整改](2026-10-18-stage3-review-remediation.md)。
- [阶段5签署](2026-10-27-stage5-exit-signoff.md)、[冻结profile v2](../specs/2026-10-25-stage5-profile-v2.md)、[质量审查](2026-10-27-implementation-quality-review.md)。
- VFS [M13a设计](../specs/2026-10-22-stage5-m13a-vfs-links-design.md)、[打开对象契约](../specs/2026-10-23-vfs-libos-foundation-design.md)、[执行计划](../plans/2026-10-23-vfs-libos-foundation-plan.md)、[前序报告](2026-10-23-vfs-libos-foundation-report.md)。

## 复核判断

| 阶段 | 核对内容与结论 |
|---|---|
| 1 | Authority、结构根、追加式历史、单调 CapId 与页帧/启动证据均已有对应实现和后继回归；保留 ISSUE-AUTH-001 的既定 image/vspace 细化分叉。 |
| 2 | 带 Cap 的阻塞直投已由后继整改关闭，容量 BLK-004 也已有 C ELF 见证；不能把历史报告的 Open 当作当前缺失。异常恢复仍有 SPEC-074 登记的内核代调/源授权细化差异，本次不修改这项既定契约。 |
| 3 | Namespace DFS、逐条销毁审计、准确对象类型与长派生链查询已有后继修复；检查当前 Timer set/cancel/destroy 前置与审计预算，未提出新行为修订。 |
| 4 | 服务已使用共享用户态 VFS/OpenFiles；检查原生 runtime 的 IPC、Namespace、Capability 与 Timer API，没有为已实现接口重复添加封装。 |
| 5 | 找到并修复下述三个当前缺陷；动态链接、硬链接、持久化、多线程 fork 等显式排除项按原签署边界保留。 |

以上为有界源码与证据链复核，不是所有可达状态穷举，也不是新的签署。

## 失败反例与修复

### 空文件位置提示导致内容别名

复现：创建 A 并写一个字节 → 创建空 B（保存当时尾部偏移）→ A 再增长 64 字节 → B 写一个字节。
修复前新增 host 测试失败：A 第二字节从 `A` 被改为 `B`。原因是 `growth_limit` 只考虑起点位于候选位置之后的区间，遗漏覆盖候选位置的区间；空节点不占区间，其旧 data 可能被邻居增长或压实覆盖。

修复：若候选起点位于任一存活区间内部，原地容量返回 0，复用现有压实/搬迁逻辑重新分配独占区间；保持节点号、打开对象、游标和协议不变。新增 host 回归及 V3 的 `independent=1` 同源检查，要求 A/B 内容均保持独立。V3 stdout 期望显式追加此断言。

### musl 检查的假 PASS

前序质量审查的 locale 问题在当前基线已强制 C locale；但缓存 miss 分支仍跳过归档审计并打印 PASS。
修复：无有效缓存直接返回错误并提示构建；只有输入、缓存与归档全部验证后才打印 PASS。
新增测试覆盖缓存 miss、不合法归档、成功路径，以及 subprocess 强制 C locale 且不改变父进程环境。

### 干净工作树的聚焦测试失败

首次 `CARGO_NET_OFFLINE=true cargo xtask test-vfs V3` 在 Linux 参考编译时失败：`target/prima-os/vfs3-linux: No such file or directory`。多个参考编译入口都假定输出父目录已由镜像构建创建，但聚焦测试恰好先运行参考程序。

修复：在 xtask 测试命令公共入口创建当前工作树的输出目录，避免逐个探针修补；不修改任何阶段 6 机制或断言。此失败来自首次干净工作树实跑，修复后再跑同一命令。

## 命令与结果

| 命令 | 本次结果与范围 |
|---|---|
| `CARGO_TARGET_DIR=/tmp/prima-stage1-5-runtime cargo test -p prima-runtime empty_file_offset_inside_grown_neighbor -- --nocapture`（修复前） | FAIL：A 内容被 B 覆盖，保留反例。 |
| `CARGO_TARGET_DIR=/tmp/prima-stage1-5-runtime cargo test -p prima-runtime`（修复后） | PASS：41 项 host 测试。 |
| `CARGO_NET_OFFLINE=true cargo test --workspace`（最终源码） | PASS：1044 项测试 + 7 doctest，共 1051；0 failed。 |
| `python3 prima-user/musl/test_port.py` | PASS：7 项 host 测试，包含实际 C/汇编夹具和新增构建护栏。 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy `-D warnings`、裸机内核构建、内核零 unsafe（35 文件）。 |
| `sh prima-user/musl/build.sh --check`（缓存 miss） | 预期失败，exit 1：`musl cache missing, stale or incomplete`，没有打印 PASS。 |
| `sh prima-user/musl/build.sh --check`（重建后） | PASS：快照、缓存与归档均验证，`cache=hit`。 |
| `CARGO_NET_OFFLINE=true cargo xtask test-vfs V3` | 首次因输出目录缺失失败；修复后 PASS：Linux 同源参考 + BIOS QEMU run=1/2，含 `independent=1` 与公共启动/原生服务断言。 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base 298bf55c46e2ba9c52b6730368a5e547d1d4f7ea` | PASS：最终文档自检、生成、链接/目录及基线变更检查。 |

原始日志位于本次运行环境 `/tmp/prima-stage1-5-*.log`，不作为永久仓库工件；本文保留命令、计数和关键反例。未运行全套 BIOS 程序、Lean 或 Rust probe 独立验收，本次 QEMU 范围仅 V3 及其公共启动/原生服务链。

## 限制与阶段影响

- 不改变 S1～S5 的已签署范围，不更新冻结 profile 的状态字，不宣称完整 POSIX 支持。
- 不覆盖完整 COSIR→机器细化、SMP、真机、性能或全部参数组合；本次不补齐上述显式阶段外能力。
- 首次 `cargo test --workspace` 因沙箱无法解析 `rsproxy.cn` 而失败；沙箱外在线重跑仍长时间停在 registry 查询，已取消；离线重跑成功。原始失败保留，不把依赖下载失败当作产品行为失败。
- 本地 gitignored musl/sbase 输入由主仓库 target 只读复制到当前工作树；不修改主仓库，不共享构建输出。

## 后继关系

本报告补强 VFS 独占内容区不变量，并关闭质量审查中 `--check` 缓存未命中假 PASS 项；不覆盖该报告的 CI、性能、结构重构与旧 QEMU 超时问题。
