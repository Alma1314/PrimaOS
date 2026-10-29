---
id: compatibility-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-27
---

# 兼容性台账索引

本目录保存阶段 5 的**兼容性状态台账**：冻结 profile 的 API 行、符号级状态和已知偏差。状态以最新报告与 [`Prima OS 实现阶段.md`](../../../Prima%20OS%20实现阶段.md) 为准。

| 文档 | 标题 |
|---|---|
| [stage6-static-whitelist.md](stage6-static-whitelist.md) | 阶段 6 静态 ELF 白名单（D12 冻结；契约非实现状态） |
| [compat-state.md](compat-state.md) | 阶段5当前子集状态与历史基线（按章节区分） |
| [stage5-working-api.md](stage5-working-api.md) | 当前工作逐入口表；运行范围见本轮报告，不升级为完整POSIX支持 |
| [symbol-trace.md](symbol-trace.md) | 符号追溯：最初echo闭包 + M20子系统调用链 |

剩余 API/子系统的实施顺序见[阶段 5 收尾与阶段 6 入口路线图](../plans/2026-10-22-stage5-completion-roadmap.md)；
该路线图是计划，不改变本目录的兼容状态。

## 使用规则

- `compat-state.md`：API/符号兼容状态；每项须有状态、已知偏差和测试入口。
- `symbol-trace.md`：公开符号 → 调用路径/服务的追溯表。
- `stage6-static-whitelist.md`：阶段 6 白名单的制品、成员与通过判据；成员/版本/哈希变更即新裁决。
- 新增或改变兼容状态时，先更新冻结 profile 或发布新版本，再更新本目录与测试报告。

## 按范围读取

两表的开头保留早期盘点，不能按首个表格概括整个当前实现。compat-state §5–7 / symbol-trace §5–6记录后继汇总；组织级签署与未满足条件仍查阶段跟踪/M21。源码号段以prima-abi为准，历史候选号不得直接用于调用。

2026-10-24 musl 子集与证据见[边界设计](../specs/2026-10-24-musl-port-boundary-design.md)、[验证报告](../reports/2026-10-24-musl-port-boundary-report.md)；快照重放输入见 [musl-snapshot.lock.toml](musl-snapshot.lock.toml)。


2026-10-27本轮新增入口的分层结果见[统一报告](../reports/2026-10-25-stage0-5-gap-followup.md)：
完整单次BIOS68轮、20项Linux参考及相关host/构建检查通过。上方历史表的planned/UNTEST
及受限Rust旧描述按各自版本解释；profile v2、静态护栏范围及阶段5组织级签署仍待最终确认。
