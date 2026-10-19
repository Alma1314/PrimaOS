# 文档导航

当前状态与退出条件以[实现阶段跟踪](../Prima%20OS%20实现阶段.md)为准。历史计划、设计和报告保留编写时的结论；后续复核可能改变其适用范围。

| 需要了解 | 入口 |
|---|---|
| 项目架构与研究目标 | [Prima OS 白皮书](../Prima%20OS.md) |
| 平台无关操作语义 | [COSIR 原语规范](../COSIR%20原语规范.md) |
| 当前阶段状态、阻塞与证据 | [实现阶段跟踪](../Prima%20OS%20实现阶段.md) |
| 阶段 5 的计划与证据 | [修订计划](superpowers/plans/2026-10-18-stage5-revised-plan.md)、[musl 移植计划](superpowers/plans/2026-10-18-musl-port-plan.md)、[容量水位](superpowers/reports/2026-10-18-stage5-capacity-watermarks.md)、[M5 sbase echo](superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md) |
| 阶段 0–4 退出复核 | [阶段 0](superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md) · [1](superpowers/reports/2026-10-17-stage1-exit-review-spec146.md) · [2](superpowers/reports/2026-10-17-stage2-exit-review-spec147.md) · [3](superpowers/reports/2026-10-17-stage3-exit-review-spec148.md) · [4](superpowers/reports/2026-10-17-stage4-exit-review-spec149.md) |
| 最新实现整改 | [阶段 1](superpowers/reports/2026-10-18-stage1-open-issues-spec151.md) · [2](superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md) · [3](superpowers/reports/2026-10-18-stage3-review-remediation.md) |

按切片查找历史工件：设计在 [`specs/`](superpowers/specs/)，执行步骤在 [`plans/`](superpowers/plans/)，结果和反例在 [`reports/`](superpowers/reports/)。文件名含日期与主题；在仓库根目录运行 `rg --files docs/superpowers | rg 'stage3|ipc|SPEC'` 可按关键词定位。

证据应按实际范围解读：设计与计划是预期，报告记录实测；host、QEMU、Lean 与兼容性测试的结论不能互相替代。
