---
id: report-proposal-cleanup
kind: report
authority: documentation-cleanup-evidence
status: validated-with-limits
lifecycle: historical
updated: 2026-11-27
---

# 失效候选提案清理与 Git 恢复

后继说明：以下记录首次清理时的范围；异构 ISA 正文随后已直接改写为
[融合内核方案](../specs/2026-11-24-heterogeneous-isa-hybrid-execution-design.md)，旧提案选项与分期已移除。

## 结论与范围

按作者“移除无用的提案”的要求，移除两份外部 RuxOS 基座库候选正文（共 1476 行），
将保留的 Gate 初期递送设计改为 historical，并修正当前入口。融合内核/UNIFICO 提案、
Windows 路线、holder 设计、实现契约、失败、评审与签署报告保留；未实现或未批准本身不是删除理由。

删除对象的共同问题是：仍自称“唯一真相源”或使用“已冻结”措辞，却假定另一套对象/ABI、
工具链与阶段安排。它们没有 Prima 运行/证明证据，已经不承担当前设计工作；必要的决策、
审查风险、未采纳边界在下文及保留的正式裁决链中承接，原文可按哈希从 Git 恢复。
本清理不是丢弃失败历史，也不把原候选的文稿自检写成 Prima 的验收。

## 基线与来源

- Git 基线/两文件恢复版本：`1c283ec1707637190368eff20dba4702ed247d60`。
- 开始时已有上一轮[架构修订](2026-11-27-architecture-isa-re-review.md)的未提交文档变更；
  本次保留它们，未重置、提交或改写代码。两删除文件逐字节等于上述 Git blob，已在删除前核验。
- 阅读全文并检查 `docs.py inspect`、正文/JSON 及裸文件名/DOC-002 引用；两份候选没有主题路由，
  活跃链接仅来自阶段跟踪和 specs 索引，白皮书另有裸路径与候选分解。
- 后继裁决：[阶段 5 准入分析](2026-10-18-stage5-entry-analysis.md)、
  [修订计划](../plans/2026-10-18-stage5-revised-plan.md)、
  [D1–D10](../specs/2026-10-26-stage5-release-decisions.md)、
  [SPEC-152 签署](2026-10-27-stage5-exit-signoff.md)。
- 现行范围：[profile v2](../specs/2026-10-25-stage5-profile-v2.md)、
  [兼容状态](../compat/compat-state.md)、[符号追溯](../compat/symbol-trace.md)、
  [musl 边界](../specs/2026-10-24-musl-port-boundary-design.md)、
  [Rust D6](../specs/2026-10-26-rust-d6-scope-decision.md)。

## 删除清单与恢复身份

两文件没有 front matter id，退役身份为完整原路径；身份、恢复版本和 SHA-256 同时登记在
[生命周期台账](../../lifecycle.json)。不在原路径放占位文件，也不另建 archive 副本。

| 原路径 | 删除理由 | 原文 SHA-256 |
|---|---|---|
| `docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md` | 外部 RuxOS 候选的对象、`0x0xx` 号表、工具链、动态链接 P0、性能与工期承诺不构成 Prima 契约；正式范围已由上列裁决/台账承接 | `aba5b8aa97c88cbe1a2b7a587b4da146e9f40fb9dc383b92889d7b18d8000b21` |
| `docs/superpowers/specs/2026-10-11-baselib-stack-walkthrough.md` | 配套文稿走查及改动摘要，无独立执行/失败实测；重要风险与原候选未决项由本节以下保留，整文可恢复 | `761a2808292449ae24021f105482384eb3575eb77ee8fe0bc17e23f45729de76` |

恢复到仓库外检查，例如：

```sh
git show 1c283ec1707637190368eff20dba4702ed247d60:docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md > /tmp/baselib-stack-spec-v1.1.md
git show 1c283ec1707637190368eff20dba4702ed247d60:docs/superpowers/specs/2026-10-11-baselib-stack-walkthrough.md > /tmp/baselib-stack-walkthrough.md
sha256sum /tmp/baselib-stack-spec-v1.1.md /tmp/baselib-stack-walkthrough.md
```

## 保留的决策、风险与未采纳边界

原候选的 K-1/K-2/K-3 分别是常规 syscall + L4 风格热路径的混合 ABI、musl 主体加自研
边界、自家 C/Rust 程序的有限承诺。原文把它们称为“用户确认”，同时又声明号表/替换点需
内核评审；本仓库的准入与签署材料明确把整份文档当外部候选。这里保留该来源/权威区别，
不追认 K-1 的热路径、原号表或原对象类型为 Prima 批准项。

| 原候选/走查议题 | 保留的事实与风险 | 当前处置/权威入口 |
|---|---|---|
| K-1、D-01/04/06/07/09：命名、混合 ABI、page loan、errno、直接 IPC | 候选有自定义 `0x0xx` 号表/扩展错误、RUXOS 命名与强制直接 IPC；不可与 Prima 现有号混用 | `prima-abi` 与兼容台账定义实际接口；D2/D9 不采用未测 250 ns 门槛；page loan/原错误号不因删除或保留而获得批准 |
| K-2、RED-1/3/5/6/7：musl、符号追溯、信号、匿名内存、版本与 tty | “五个替换点”是估计；符号存在不证明可用；启动、errno、寄存器、版本和权限必须有实际证据 | musl 边界、profile v2、symbol-trace 及各实现报告；原静态/动态 `NT_RUXOS_ABI`、页池/flag 值不作为当前 ABI |
| RED-2、D-13/14/17：fork/vfork/COW | 单/多线程 fork、atfork 重置、fd/OFD/CLOEXEC 与 COW 不可用候选步骤替代 | D3 及进程/fork 后继契约与 profile；完整多线程 fork 不因旧文声称“定稿”而变 supported |
| K-3、RED-4、D-11/16：承诺与存在符号 | 原候选区分 musl/libm 可链接与承诺，busybox 只是探针；其自研程序出口不等于阶段 5 的开源程序出口 | profile v2/SPEC-152 冻结范围；兼容台账保留 partial/unsupported，不采纳旧百分比或符号总数承诺 |
| D-02/03/05/10/18：等待、CLOEXEC、锁序、服务器边界、W^X | 这些是应由实际机制/协议说明的边界，原候选的内核对象、锁序和 syscall 值不是已实现设计 | 原语规范 WaitObject、VFS/进程/内存契约及内核零 unsafe 边界分别承接；不把“已写入红线”视为通过 |
| D-08/12、ld.so P0、persona/符号版本 | 原候选包含 unwinding/静态 PIE 与动态装载前置，并将 AS persona/note 设为强制；与 Prima 静态阶段划分冲突 | D4/D6/S5-D7 及 SPEC-152：阶段 5 静态、有界 Rust 范围；动态链接归 7-E，未实现旧 AS persona/note 不能被隐藏 |
| 权限/错误时序、性能与容量 | 原文给出纳秒预算、恒时错误、百万次压力与容量目标；是计划断言，无本次或原候选实测支撑 | 不把它们并入已签署退出；阶段 7 按冻结方法实测再定门槛，安全/侧信道主张按白皮书范围解释 |
| 重启、重试与外部副作用 | 原候选主张服务 session 自动重建、100 ms 恢复与日志降级；不能据此保证非幂等 I/O 不重复 | 阶段 7-G 端到端故障/去重/结果查询契约；本次不新增恢复保证 |
| 原走查 RED/YELLOW“已闭环” | 仅表示改写了候选正文，未取得 Prima 编译/QEMU/Lean/参考差分；YELLOW 中若干项仍待作者确认 | 不保留成当前待批准清单，不抹除原来源；具体原值可按恢复版本核对 |

原准入分析还明确指出 `mlock/munlock` no-op 与走查 `EUNIMPLEMENTED` 的冲突，以及
候选工具链与仓库不一致；主规格自身还存在无熵错误值、缓冲/降级叙述不一致。
这些差异没有通过删除变成已解决的技术问题；原值不再用作实现蓝本，当前行为只按具体
profile/契约与实测解释。原候选的 22 周/26 人周排期和模板化 CI/发布清单一并退役。

## 保留材料及导航修订

- Gate 初期递送设计保留为 historical：它仍承载 D17–D22 和多份复审/失败报告的输入，
  仅“未实现/候选号段”的历史基线不能再作为当前进度。原契约与引用链保留。
- holder 草案保留：H0 条件与后续 holder 计划仍引用其未决模型；未落地不等于无用。
- 融合内核/UNIFICO 与 Windows 路线保留：仍是作者当前指定工作方向，不把本次清理当作取消。
- 白皮书删除旧候选的当前分解/待裁决口吻，指向正式阶段 5 裁决及本报告；阶段跟踪和
  specs 索引改指清理报告。历史计划、风险/签署报告保留原结论，并通过 DOC-002/台账找回原文。

本报告局部承接[2026-10-24 生命周期报告](2026-10-24-documentation-lifecycle-report.md)中
“DOC-002 尚关联 BLK-003、因此保留”的处置：SPEC-152 已按阶段 5 关闭该阻塞，本次作者
又要求清理无用提案，当前契约与风险摘要已有明确承接；不以日期较新作为单独删除依据。

## 命令与结果

| 命令/校验 | 本次结果 | 范围 |
|---|---|---|
| `python3 scripts/docs.py self-test` | PASS，exit 0 | 文档解析、检索、确定性、生命周期/Git 恢复与变更守卫 |
| `python3 scripts/docs.py build` | PASS，exit 0；447 documents | 再生机器目录与 Markdown 索引 |
| `python3 scripts/docs.py check` | PASS，exit 0；447 documents | 目录、身份、本地链接/锚点与恢复登记 |
| `python3 scripts/docs.py check --base 1c283ec` | PASS，exit 0 | 新文档元数据及两份删除相对基线的哈希守卫 |
| `python3 scripts/docs.py inspect docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md` | PASS，exit 0；removed、入站链接 0 | 原路径查询与恢复入口 |
| `python3 scripts/docs.py inspect docs/superpowers/specs/2026-10-11-baselib-stack-walkthrough.md` | PASS，exit 0；removed、入站链接 0 | 原路径查询与恢复入口 |
| Python 读取两条 `git show <恢复版本>:<原路径>` 并逐字节/ SHA-256 核验 | PASS；与上表一致 | 删除前工作树等于 Git blob；删除后再次核验恢复哈希 |
| `git diff --check` | PASS，exit 0 | 空白检查 |

检查在最终文本更新后复跑；本次没有代码、boot/协议或 profile 改动，不运行 Rust/QEMU/Lean，
不新增 OS 证据，不改变阶段状态或退出勾选。原始检查输出只留在 `/tmp/` 或工具会话。

## 后继与重审

恢复文件作重新研究输入时，须先通过新的范围/ABI 评审，不重新启用退役身份冒称现行规范。
新增当前引用、恢复哈希不符、Git 恢复版本缺失或正式契约出现未承接的决策时重审清理；
不得为通过文档检查忽略历史失败或删除守卫。
