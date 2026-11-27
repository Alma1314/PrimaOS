---
id: report-documentation-consolidation
kind: report
authority: documentation-cleanup-evidence
status: validated-with-limits
lifecycle: historical
updated: 2026-11-28
---

# 文档整理、合并与压缩记录

## 范围与依据

按作者“整理，删除，压缩，合并现有文档”的要求，合并重复导航和一次性短计划，压缩三份早期执行模板，收紧索引中的过时进度摘要。规则以[文档治理契约 §9–10](../specs/2026-10-23-documentation-system-design.md#9-生命周期与废弃决策)和[维护手册](../../architecture/documentation-maintenance.md)为准；前序清理见[生命周期治理](2026-10-24-documentation-lifecycle-report.md)、[日志内嵌](2026-10-31-documentation-log-inline.md)、[候选提案退役](2026-11-27-proposal-cleanup.md)。

基线/恢复版本为 `5ebf2791fcf01f21cf6e396227483e74ef1b2dd9`，开始时目录为 452 documents。清理前逐份读取七个删除/压缩对象，验证工作树等于该提交的 Git blob；用 `docs.py inspect` 检查四份删除对象的入站链接/主题，辅以全仓 Markdown/JSON 的裸路径和文件名检索。

本轮只改文档。开始已有 `prima-ostd/Cargo.toml` 修改，期间另有 kernel/OSTD/xtask 并行代码变更；本轮不修改、回退、暂存或验证那些代码。没有改变规范行为、profile、阶段退出勾选，也没有新增 OS 运行/证明证据。

## 合并与删除

全部退役原 id、恢复版本和 SHA-256 已写入[生命周期台账](../../lifecycle.json)。不保留占位正文或 archive 副本，旧路径可用 `python3 scripts/docs.py inspect <旧路径>` 查询。

| 删除文件 | 合并去向与保留内容 | 原 id |
|---|---|---|
| `docs/architecture/getting-started.md`（43 行） | [文档中心首次阅读](../../README.md#首次阅读)：章节顺序、角色最小上下文；维护/开发手册继续负责交付流程 | `guide-getting-started` |
| `docs/architecture/README.md`（32 行） | 文档中心及维护手册：手册、主题、目录与四种模板入口；原链接正文全部保留 | `architecture-home` |
| `docs/superpowers/README.md`（42 行） | 文档中心的权威分工和工件布局；AI 检索协议负责追溯；维护手册负责命名/维护 | `artifacts-home` |
| `docs/superpowers/plans/2026-10-23-vfs-libos-foundation-plan.md`（18 行） | [既有设计中的历史执行顺序](../specs/2026-10-23-vfs-libos-foundation-design.md#执行顺序历史记录)：五步完整承接；原验证报告的 QEMU 超时和范围不变 | `plan-vfs-libos-foundation` |

四份共 135 行，无独立决定/实跑被删除。VFS 的两份历史报告仅改原计划链接并注明合并/恢复，不重写失败或实验结果；所有入站导航与 vfs 主题路由同次更新。

## 压缩与保留依据

三份计划保留原路径、原身份和任务标题，声明 historical/execution-record；去掉逐行源码、安装/提交脚本、过渡占位和旧 tracker 替换快照。具体实现只从生产代码读取，现行工具前置只从维护手册/Lean 映照读取。下列关键选择与独立观察仍在各原文件中：

| 计划 | 压缩前行数 | 必须保留的内容 |
|---|---:|---|
| [首个 BIOS 启动闭环](../plans/2026-08-04-stage1-boot-slice.md) | 1844 | BIOS/UEFI 范围区别、协议/退出、readonly/no-shutdown 两项例外、kill/reap/reader 失败清理、Task 6 RED/GREEN、Task 7 原 10 秒超时/100% CPU/7.079 秒/17.156 秒观察、TEST-001/002/003 与阶段边界 |
| [exc_resume 初建](../plans/2026-08-07-exc-resume-slice.md) | 978 | 纯 host 范围、状态迁移和九类原测试目标、旧独立恢复地址/短帧/布尔授权 stub 的缺陷、2026-08-12 后继纠偏及现行契约入口；不把预期输出改成实测 |
| [BLK-001 Lean 初建](../plans/2026-08-18-blk001-lean-formal-model.md) | 803 | 独立 Lake 包、函数式投影、cosir bin+lib 的用户选择、原 WellFormed=True 与仅授权 excResume 的证明局限、手工对拍边界、tlb/memseg/cow 后继与独立审查要求 |

原计划含失败和设计纠偏价值，故保留压缩正文而非退役整份。初建 Lean 草稿的占位、浮动 stable/std4 main、旧异常帧不能继续作为实现蓝本。三份合计从 3625 行压缩为 196 行（减少 3429 行，约 94.6%）。原文完整内容及下表哈希可从固定提交恢复；本次不修改这些计划的上位设计或任何 Lean 模型。

| 原路径（仓库相对） | 压缩/删除前 SHA-256 |
|---|---|
| `docs/architecture/getting-started.md` | `71ee209b0cfa25c3e2233883eaf29a943e76687e86be173a36b46c5faeadfab1` |
| `docs/architecture/README.md` | `7697aad2a9568ab43480192a1fb581a2c0e5e492c25b5c1d3841e7393959734e` |
| `docs/superpowers/README.md` | `50adf2f45da488bc0db04e0b748df8a62de3d1e220fb89355f7aa429dd1bd75a` |
| `docs/superpowers/plans/2026-10-23-vfs-libos-foundation-plan.md` | `4923bf4e3563771fa31f23408484f70c13b39f080722f076d15626115bcf4baa` |
| `docs/superpowers/plans/2026-08-04-stage1-boot-slice.md` | `aa89c3fc1631cab65528d8d5e61d1edf6c5b039d2915260016a4e828c7efcc23` |
| `docs/superpowers/plans/2026-08-07-exc-resume-slice.md` | `cf3e1ec1bfeb8a63c8af9a3d4bd88f5b625d8266c12e4dc0f7c45639cba2bc2e` |
| `docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md` | `8d5bac991cce1aea41109a59646afa84e581885dcf27c72eeb01d0e99c5de2f7` |

例如 `git show 5ebf2791fcf01f21cf6e396227483e74ef1b2dd9:docs/superpowers/plans/2026-08-04-stage1-boot-slice.md > /tmp/stage1-boot-original.md`，再用 `sha256sum /tmp/stage1-boot-original.md` 核对。浅克隆需补齐该提交，不跳过恢复校验。

## 索引与检索

- 计划索引保留全部 25 份未退役计划，移除已过时的 P2b/P3/Windows 待办摘要，改指阶段跟踪、后继审计及报告。
- 报告索引压缩近期长摘要，保留原链接及历史分组；原始报告未据后继通过改写。日期/测试数不再成为索引中的第二份当前状态。
- 根 README 修正 C/musl 所属目录为 `libos/linux/`；vfs 主题改用 P2b/P3 后继阅读链，避免继续以 P2a 时点的未接线描述现状。
- 机器目录通过 `docs.py build` 再生；其它契约、兼容台账、阶段签署、Lean 映照和独立失败/反例报告保留。

## 验证与限制

| 检查 | 结果与范围 |
|---|---|
| `python3 scripts/docs.py self-test` | PASS，exit 0；解析、检索、确定性、生命周期、Git 恢复及变更守卫 |
| `python3 scripts/docs.py build` | PASS，exit 0；450 documents（含并行新增的 P5-3b-1 报告），再生两个目录；本轮自身净减 3 份（删除 4、新增本报告 1） |
| `python3 scripts/docs.py check` | 最终 PASS，exit 0；首轮因并行新增 P5-3b-1 报告尚未加入 reports README 失败，补入中性导航链接后通过；不补写其运行结论 |
| `python3 scripts/docs.py check --base 5ebf2791fcf01f21cf6e396227483e74ef1b2dd9` | PASS，exit 0；新文档元数据与删除恢复守卫 |
| 四份旧路径 inspect/Git blob SHA-256 核验 | PASS，全部 removed、入站链接/主题引用为 0；七份删除/压缩对象的原文恢复哈希全部匹配 |
| 原计划 Task 标题/报告索引链接集核对 | PASS；三份全部 Task 标题保留，原报告索引的链接无遗漏 |
| `git diff --check` | PASS，exit 0 |

本轮是纯文档变更，未运行 Rust/Lean/QEMU；并行任务的命令与结果属于其报告，不在此复用为本轮 PASS。原始检查输出只留 `/tmp/`。

清理仅覆盖上述对象和相关入口，没有逐份重审其余历史计划的全部代码模板，也不裁决尚存提案或发布任何 profile。远端链接未联网验证；外部旧路径会失效，但台账保留替代与 Git 恢复。新增引用、恢复哈希变化、承接遗漏或文档检查失败时重审；不能凭日期或低引用数自动删除独立契约、失败或证据。

## 2026-11-28 提交范围复核

提交快照只包含本轮文档整理：并行内核/OSTD/xtask、阶段跟踪、SMP 设计及 P5-3b-1 报告保留在原工作区。共享报告索引的 P5-3b-1 导航行暂不纳入此次提交；目录按提交快照重新生成，避免引用未提交正文。上节 450 documents 是 2026-11-27 完整工作区的历史检查；独立文档提交快照为 449 documents。

在临时本地克隆中覆盖本轮文件，重新运行 `python3 scripts/docs.py self-test`、`build`、`check`、`check --base 5ebf2791fcf01f21cf6e396227483e74ef1b2dd9` 与 `git diff --check`，均 exit 0。此复核不验证并行代码，也不新增 Rust/Lean/QEMU 运行结论。
