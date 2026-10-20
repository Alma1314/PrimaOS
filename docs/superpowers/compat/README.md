# 兼容性台账索引

本目录保存阶段 5 的**兼容性状态台账**：冻结 profile 的 API 行、符号级状态和已知偏差。状态以最新报告与 [`Prima OS 实现阶段.md`](../../../Prima%20OS%20实现阶段.md) 为准。

| 文档 | 标题 |
|---|---|
| [compat-state.md](compat-state.md) | 阶段 5 兼容状态台账（M0） |
| [symbol-trace.md](symbol-trace.md) | 阶段 5 符号 ↔ 系统调用追溯（musl 1.2.5 / echo） |

## 使用规则

- `compat-state.md`：API/符号兼容状态；每项须有状态、已知偏差和测试入口。
- `symbol-trace.md`：公开符号 → 调用路径/服务的追溯表。
- 新增或改变兼容状态时，先更新冻结 profile 或发布新版本，再更新本目录与测试报告。
