---
id: report-documentation-log-inline
kind: report
authority: documentation-maintenance-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# 文档日志附件整理：关键证据内嵌 Markdown

## 范围与结论

按作者要求，移除 `docs/superpowers/reports/artifacts/` 下131个日志、命令输出和哈希附件，
共1703194字节。13份原报告继续保留命令、运行范围、失败、整改链和限制；
必要的协议输出、故障字段及源码/输入身份内嵌于各报告“关键输出摘录”节。
重复启动过程、逐项通过用例名和完整编译输出不再占用工作树。

这是纯文档整理，未重跑或扩大任何OS验证；历史PASS、失败、超时及阶段6退出0/4均不改变。
原报告的命令表仍描述各自当时的运行，不是本轮新运行。

## 附件归属与恢复

清理前版本：`507a5770f662a04269a7e937c4db47c8bada28d3`。逐个核对131个附件与该版本的Git blob字节相同后移除。
没有删除Markdown报告；`docs/lifecycle.json`按现有契约只登记Markdown文档的生命周期，
日志附件的原路径由下表目录及该版本的Git树保留。没有改写Git历史。

以下目录均相对于旧 `docs/superpowers/reports/artifacts/`：

| 旧目录 | 文件数 | 保留证据的报告 |
|---|---|---|
| `stage6-audit` | 4 | [2026-10-30-stage6-audit-fail-stop.md](2026-10-30-stage6-audit-fail-stop.md) |
| `stage6-audit-qemu` | 30 | [2026-10-30-stage6-audit-stop-qemu.md](2026-10-30-stage6-audit-stop-qemu.md) |
| `stage6-completion` | 4 | [2026-10-30-stage6-private-completion-waits.md](2026-10-30-stage6-private-completion-waits.md) |
| `stage6-exec-capacity` | 13 | [2026-10-31-stage6-exec-frame-capacity.md](2026-10-31-stage6-exec-frame-capacity.md) |
| `stage6-gate` | 10 | [2026-10-31-stage6-gate-lifecycle.md](2026-10-31-stage6-gate-lifecycle.md) |
| `stage6-gate-admission` | 11 | [2026-10-31-stage6-gate-admission.md](2026-10-31-stage6-gate-admission.md) |
| `stage6-gate-authority` | 6 | [2026-10-31-stage6-gate-authority-loss.md](2026-10-31-stage6-gate-authority-loss.md) |
| `stage6-gate-control-rejections` | 11 | [2026-10-31-stage6-gate-control-rejections.md](2026-10-31-stage6-gate-control-rejections.md) |
| `stage6-gate-dispatch` | 19 | [2026-10-31-stage6-gate-dispatch.md](2026-10-31-stage6-gate-dispatch.md) |
| `stage6-m1` | 4 | [2026-10-29-stage6-m1-blocker-closure.md](2026-10-29-stage6-m1-blocker-closure.md) |
| `stage6-main-merge` | 1 | [2026-10-31-stage6-main-merge.md](2026-10-31-stage6-main-merge.md) |
| `stage6-streamed-elf` | 12 | [2026-10-31-stage6-streamed-elf-input.md](2026-10-31-stage6-streamed-elf-input.md) |
| `stage6-user-copy` | 6 | [2026-10-30-stage6-user-copy-transactions.md](2026-10-30-stage6-user-copy-transactions.md) |

列举旧文件或恢复某个附件到临时目录：

```sh
git ls-tree -r --name-only 507a5770f662a04269a7e937c4db47c8bada28d3 -- docs/superpowers/reports/artifacts/
git show 507a5770f662a04269a7e937c4db47c8bada28d3:docs/superpowers/reports/artifacts/stage6-gate-control-rejections/full-suite-errors.txt > /tmp/prima-old-full-suite-errors.txt
```

恢复命令供定位历史原始输出，不应将完整日志重新加入仓库。旧日志哈希附件中的源码身份
已分别内嵌到dispatch、admission和control-rejections报告；冻结busybox输入身份也保留在解析报告中。

## 原记录计数差异

门控对象报告原称13组／18轮超时；其失败附件实际逐条列出13组／17轮。已在
[原报告摘录](2026-10-31-stage6-gate-lifecycle.md#关键输出摘录)保留完整可核对清单并注明差异，
不补造缺失轮次、不改写原运行结论，也不重新推断超时原因。

## 后续写法

[文档契约](../specs/2026-10-23-documentation-system-design.md)、
[维护手册](../../architecture/documentation-maintenance.md)、
[报告模板](../../architecture/templates/report.md)及仓库约定统一为：原始日志留在`target/`或`/tmp/`，
Markdown内记录命令、基线、环境、退出状态和足以判断结论的短片段，失败与限制同样保留。
不得把整份日志改扩展名塞进Markdown。机器可读的契约、冻结输入台账及生成目录不属于运行日志。

## 本轮验证

| 本轮检查 | 结果 |
|---|---|
| `python3 scripts/docs.py self-test` | PASS：文档解析、生命周期及Git恢复自检 |
| `python3 scripts/docs.py build` | PASS：353份文档，生成目录从正文重建 |
| `python3 scripts/docs.py check` | PASS：目录新鲜度、ID、主题、本地链接和锚点 |
| `python3 scripts/docs.py check --base 507a577` | PASS：以清理前版本检查元数据及变更 |
| 摘录逐字核对 | PASS：63组所选输出与原Git内容和对应Markdown一致；D14和G1双轮共同记录另核对相同 |
| 附件与忽略规则 | PASS：工作树不再含独立`.log`/`.txt`附件；旧附件目录和`.log`文件匹配忽略规则 |
| `git diff --check`、阶段6勾选 | PASS：无空白错误，9项交付／退出勾选均保持未勾选 |

本轮未运行Rust、QEMU或Lean验证；内嵌的旧测试输出不计为本次新验证。
