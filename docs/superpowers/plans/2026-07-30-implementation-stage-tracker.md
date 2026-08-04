# Prima OS 实现阶段跟踪文档实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `Prima OS.md` 第 15 章的阶段 0–7 转化为 `Prima OS 实现阶段.md` 中可持续维护、以证据为准的进度跟踪文档。

**Architecture:** `Prima OS.md` 继续负责研究假设、架构和阶段定义，`COSIR 原语规范.md` 继续负责精确状态语义；`Prima OS 实现阶段.md` 只记录当前状态、退出条件检查、证据链接和阻塞项。同步采用“引用权威来源、不复制规范正文”的方式，避免三个文档之间发生语义漂移。

**Tech Stack:** Obsidian Markdown、Wikilink、Python 3 标准库只读校验脚本

## Global Constraints

- 当前目录 `C:\Project\PrimaOS` 不是 Git 仓库；不得初始化 Git，也不得执行 commit。
- 只修改现有文件 `Prima OS 实现阶段.md`；不得创建重复的路线图或规范文档。
- `[[Prima OS]]` 是阶段定义和退出条件的权威来源。
- `[[COSIR 原语规范]]` 是对象、操作、不变量和验证义务的权威来源。
- 跟踪文档不得把“已设计”“已定义”记为“已实现”或“已验证”。
- 阶段状态只允许：`未开始`、`进行中`、`阻塞`、`已满足退出条件`。
- 阶段 0–6 必须按顺序晋级；阶段 7 只有在前置阶段的实证结果明确范围后才能拆分。
- 每项完成声明必须附本地文件、测试命令输出、证明工件或实验报告的可访问证据。
- 未提供证据的检查项保持未勾选；当前空仓库状态下不得臆造实现进度。
- 不在跟踪文档内复制 [[COSIR 原语规范]] 的接口参数、前后条件或全局不变量全文。

---

## 文件职责

- Modify: `Prima OS 实现阶段.md` — 当前阶段、阶段 0–7 检查表、证据索引、阻塞项和决策日志。
- Reference only: `Prima OS.md:682-814` — 阶段名称、交付物、退出条件和阶段顺序。
- Reference only: `COSIR 原语规范.md:1167-1196` — 实现细化要求和当前机制类别。
- Plan only: `docs/superpowers/plans/2026-07-30-implementation-stage-tracker.md` — 本实施计划，不作为运行状态来源。

---

### Task 1: 建立跟踪语义与文档骨架

**Files:**
- Modify: `Prima OS 实现阶段.md:1`
- Reference: `Prima OS.md:25-45`
- Reference: `Prima OS.md:682-701`

**Interfaces:**
- Consumes: `Prima OS.md` 中的状态标签和阶段 0 定义。
- Produces: 后续阶段条目统一使用的状态词、证据规则、当前快照与记录模板。

- [ ] **Step 1: 写入文档头和权威来源**

将以下内容写入空文件：

```markdown
# Prima OS 实现阶段

> 文档类型：实施进度与证据跟踪
>
> 更新日期：2026-07-30
>
> 架构与阶段定义：[[Prima OS]]
>
> 精确语义规范：[[COSIR 原语规范]]

## 维护规则

本文只记录实际进度、退出条件证据和阻塞项，不重新定义架构或原语语义。

状态只允许：

- **未开始**：尚无可验收交付物；
- **进行中**：已有工作产物，但尚未满足全部退出条件；
- **阻塞**：存在阻止继续推进的明确问题；
- **已满足退出条件**：全部退出条件均有可复查证据。

维护约束：

1. “已设计”或“已定义”不等于“已实现”；
2. 检查项只有在证据列提供可访问工件后才能勾选；
3. 测试证据记录命令、日期和结果摘要；
4. 证明证据记录工具、入口文件和定理或检查名称；
5. 实验失败和反例必须保留，不得通过修改措辞隐藏；
6. 阶段定义变化先修改 [[Prima OS]]，语义变化先修改 [[COSIR 原语规范]]，再同步本文。

## 当前快照

| 字段 | 当前值 |
|---|---|
| 当前阶段 | 阶段 0 — 语义基线 |
| 当前状态 | 进行中 |
| 已满足退出条件的最高阶段 | 无 |
| 当前实现代码 | 尚无 |
| 当前主要工件 | [[Prima OS]]、[[COSIR 原语规范]] |
| 当前首要缺口 | x86_64 机器状态到 POSIR 状态的细化草案；形式化工具中的可执行状态模型 |

当前只能确认架构白皮书和抽象语义规范已形成；尚无内核实现、机器证明、QEMU 启动证据或兼容性测试证据。
```

- [ ] **Step 2: 增加统一证据索引**

紧接当前快照写入：

```markdown
## 证据索引

| ID | 类型 | 工件或命令 | 日期 | 结果 | 支持的检查项 |
|---|---|---|---|---|---|
| DOC-001 | 架构文档 | [[Prima OS]] | 2026-07-30 | 已形成决策型架构白皮书；不代表实现完成 | S0-D3 |
| SPEC-001 | 抽象规范 | [[COSIR 原语规范]] | 2026-07-30 | 已定义对象能力状态机；尚无机器证明 | S0-D1、S0-D2 |

新增证据时使用稳定 ID；阶段检查项只引用该表中的 ID，不复制大段输出。
```

- [ ] **Step 3: 增加阻塞项和决策日志模板**

在证据索引后写入：

```markdown
## 当前阻塞项

| ID | 阶段 | 问题 | 解除条件 | 状态 |
|---|---|---|---|---|
| BLK-001 | 0 | 尚未选择并建立形式化模型工件 | 明确工具、入口文件和可执行状态迁移 | Open |
| BLK-002 | 0 | 尚无 x86_64 → POSIR 细化草案 | 定义 MachineState、抽象函数和至少 Bootstrap 的对应关系 | Open |

## 决策日志

| 日期 | 决策 | 理由 | 影响 |
|---|---|---|---|
| 2026-07-30 | 首个平台采用 QEMU x86_64 | 先验证启动链、语义和细化关系 | 阶段 1–3 以单核 QEMU profile 为准 |
| 2026-07-30 | 兼容路线采用原生服务 → POSIX 源码 → Linux 静态 ELF | 分离原生模型验证与传统 ABI 复杂度 | 对应阶段 4–6 |
| 2026-07-30 | 不把原语数量作为约束 | 以语义最小、边界清晰和可证明为准 | 新机制必须经过必要性与中立性审查 |
```

- [ ] **Step 4: 运行骨架校验**

Run：

```bash
python -c 'from pathlib import Path; p=Path("Prima OS 实现阶段.md"); s=p.read_text(encoding="utf-8"); required=["# Prima OS 实现阶段","## 维护规则","## 当前快照","## 证据索引","## 当前阻塞项","## 决策日志","[[Prima OS]]","[[COSIR 原语规范]]"]; missing=[x for x in required if x not in s]; assert not missing, missing; print("PASS tracker skeleton")'
```

Expected：

```text
PASS tracker skeleton
```

---

### Task 2: 同步阶段 0–7 检查表

**Files:**
- Modify: `Prima OS 实现阶段.md`，追加在“决策日志”之后
- Reference: `Prima OS.md:682-814`
- Reference: `COSIR 原语规范.md:1167-1196`

**Interfaces:**
- Consumes: Task 1 定义的状态词和证据 ID 格式。
- Produces: 八个阶段的交付项、退出条件、当前证据、阻塞和晋级规则。

- [ ] **Step 1: 写入阶段 0 跟踪项**

追加：

```markdown
## 阶段 0 — 语义基线

**状态：进行中**

### 交付检查

| ID | 交付物 | 状态 | 证据 |
|---|---|---|---|
| S0-D1 | 一致的对象能力状态模型 | [x] 已定义，未证明 | SPEC-001 |
| S0-D2 | 操作、错误、全局不变量和非目标 | [x] 已定义，未证明 | SPEC-001 |
| S0-D3 | 关键研究假设与反例标准 | [x] 已定义 | DOC-001 |
| S0-D4 | x86_64 机器状态到 POSIR 状态的细化草案 | [ ] 未完成 | BLK-002 |

### 退出条件

| ID | 条件 | 满足 | 证据 |
|---|---|---|---|
| S0-X1 | 规范没有已知且未记录的对象生命周期空洞 | [ ] | 需要独立规格审查或机器检查证据 |
| S0-X2 | 每项操作具有明确成功与失败语义 | [ ] | 需要逐操作覆盖报告 |
| S0-X3 | 形式化工具能够表达并执行状态与迁移 | [ ] | BLK-001 |

阶段 0 不能因文档篇幅完整而直接关闭；必须补齐形式化工件、覆盖报告和细化草案。
```

- [ ] **Step 2: 写入阶段 1–3 跟踪项**

追加以下内容；所有实现项保持未勾选：

```markdown
## 阶段 1 — 启动与最小平台层

**状态：未开始**

### 交付检查

- [ ] S1-D1：QEMU 可重复启动；证据：启动命令、固定机器配置和串口日志。
- [ ] S1-D2：串口结构化输出与确定性退出；证据：成功和失败退出码测试。
- [ ] S1-D3：物理内存发现；证据：页帧清单与保留区域测试。
- [ ] S1-D4：初始页表、GDT/IDT 与 trap 框架；证据：异常和权限测试。
- [ ] S1-D5：Bootstrap 创建 init Task；证据：初始对象和 Capability 快照。

### 退出条件

- [ ] S1-X1：脚本可无人值守启动并退出 QEMU。
- [ ] S1-X2：init 只获得规范定义的初始 Authority。
- [ ] S1-X3：活动 Segment 与空闲页帧满足唯一归属不变量。

## 阶段 2 — Capability、Segment、Task 与 IPC

**状态：未开始**

### 交付检查

- [ ] S2-D1：不可伪造且不复用的句柄表示。
- [ ] S2-D2：Capability 派生、移动、drop 与 stale 检测。
- [ ] S2-D3：Segment create/map/unmap/destroy。
- [ ] S2-D4：Task create/yield/destroy 与单核调度。
- [ ] S2-D5：Send/Recv、FIFO 邮箱与阻塞恢复。

### 退出条件

- [ ] S2-X1：每项成功和失败路径具有状态转换测试。
- [ ] S2-X2：两个不可信 Task 只能经授权消息或共享 Segment 交互。
- [ ] S2-X3：页帧在对新 Segment 可见前清零。
- [ ] S2-X4：stale handle 不会因 ID 复用重新获得 Authority。

## 阶段 3 — Namespace、Timer 与 Audit

**状态：未开始**

### 交付检查

- [ ] S3-D1：Namespace bind/unbind/mount/unmount/resolve。
- [ ] S3-D2：Mount 环检测与 stale Mount 清理。
- [ ] S3-D3：绝对单调 Timer。
- [ ] S3-D4：`deadline ≤ now` 时原子立即触发。
- [ ] S3-D5：Capability 派生、位置变化与对象生命周期 Audit。

### 退出条件

- [ ] S3-X1：所有 Namespace 解析均终止，活动 Mount 图无环。
- [ ] S3-X2：每个 Fired Timer 恰有一次对应内核通知。
- [ ] S3-X3：Audit 能重建 Capability 派生链和位置历史。
- [ ] S3-X4：Task、Namespace、Message 或 Timer 容器销毁不破坏其他对象生命周期。
```

- [ ] **Step 3: 写入阶段 4–6 跟踪项**

追加：

```markdown
## 阶段 4 — Prima 原生运行时与服务

**状态：未开始**

### 交付检查

- [ ] S4-D1：原生运行时 API 与版本规则。
- [ ] S4-D2：init / 服务管理器。
- [ ] S4-D3：日志、Namespace 和基本存储服务。
- [ ] S4-D4：第一条用户态驱动或受控 I/O 路径。
- [ ] S4-D5：使用消息与共享 Segment 的原生示例应用。

### 退出条件

- [ ] S4-X1：不依赖 POSIX LibOS 即可自举并运行应用。
- [ ] S4-X2：服务按最小权限拆分，证据列出各服务初始 Capability。
- [ ] S4-X3：大数据路径可使用共享 Segment，且有取消和错误恢复测试。
- [ ] S4-X4：故障服务不能直接破坏无关应用域。

## 阶段 5 — POSIX 源码兼容

**状态：未开始**

### 交付检查

- [ ] S5-D1：冻结且版本化的 POSIX profile。
- [ ] S5-D2：libc 适配。
- [ ] S5-D3：文件描述符、路径、线程和基础网络子集。
- [ ] S5-D4：逐 API 语义映射矩阵。
- [ ] S5-D5：与参考 Linux 的差分测试矩阵。

### 退出条件

- [ ] S5-X1：profile 内每项 API 都有语义映射和可重复测试。
- [ ] S5-X2：每项 API 明确标记 supported / partial / unsupported。
- [ ] S5-X3：选定开源程序可重新编译并通过冻结的功能测试集。

## 阶段 6 — Linux x86_64 静态 ELF

**状态：未开始**

### 交付检查

- [ ] S6-D1：静态 ELF 加载器与 Linux 初始栈。
- [ ] S6-D2：OS 中立的 syscall / exception 递送机制。
- [ ] S6-D3：版本化的 Linux LibOS syscall profile。
- [ ] S6-D4：所选 profile 必需的 futex、TLS 和进程环境。
- [ ] S6-D5：未修改静态 ELF 白名单与测试镜像。

### 退出条件

- [ ] S6-X1：白名单中的静态 ELF 无需修改即可运行。
- [ ] S6-X2：支持的 syscall 与参考 Linux 通过差分测试。
- [ ] S6-X3：HexArc / COSIR 不包含 Linux 专属对象或 syscall 语义。
- [ ] S6-X4：未支持 syscall 得到确定且可审计的失败。
```

- [ ] **Step 4: 写入阶段 7 和晋级规则**

追加：

```markdown
## 阶段 7 — 扩展与硬化

**状态：未开始；范围未冻结**

候选工作不预先视为承诺：

- [ ] SMP；
- [ ] IOMMU 与更完整的用户态驱动；
- [ ] 动态链接；
- [ ] 资源计量与配额；
- [ ] 选择性撤销；
- [ ] 更强的故障恢复；
- [ ] 第二硬件架构。

只有在阶段 0–6 的证据揭示实际优先级后，才把候选项拆成带退出条件的独立里程碑。

## 阶段晋级规则

1. 阶段状态由该阶段的退出条件决定，不由提交数量、代码行数或主观完成度决定；
2. 前一阶段未满足退出条件时，后一阶段可以做探索原型，但不能成为“当前已完成阶段”；
3. 任何失败测试、未解决反例或实现偏差都必须进入阻塞项或决策日志；
4. 关闭阶段时，在证据索引中记录完整命令、工件版本和结果；
5. 架构或语义变化必须先更新权威文档，再重新评估受影响的已关闭检查项。
```

- [ ] **Step 5: 运行阶段覆盖校验**

Run：

```bash
python -c 'from pathlib import Path; import re; s=Path("Prima OS 实现阶段.md").read_text(encoding="utf-8"); stages=[int(x) for x in re.findall(r"^## 阶段 (\d+) —",s,re.M)]; assert stages==list(range(8)), stages; required=["S0-D1","S0-X3","S1-D1","S1-X3","S2-D1","S2-X4","S3-D1","S3-X4","S4-D1","S4-X4","S5-D1","S5-X3","S6-D1","S6-X4","## 阶段晋级规则"]; missing=[x for x in required if x not in s]; assert not missing, missing; print("PASS stages=0..7 checks=complete")'
```

Expected：

```text
PASS stages=0..7 checks=complete
```

---

### Task 3: 校验链接、状态与权威边界

**Files:**
- Modify only if validation exposes a defect: `Prima OS 实现阶段.md`
- Reference: `Prima OS.md`
- Reference: `COSIR 原语规范.md`

**Interfaces:**
- Consumes: Tasks 1–2 完成的跟踪文档。
- Produces: 一个链接有效、结构完整、不虚报进度且不复制规范正文的最终文档。

- [ ] **Step 1: 校验所有 Wikilink 目标存在**

Run：

```bash
python -c 'from pathlib import Path; import re; p=Path("Prima OS 实现阶段.md"); s=p.read_text(encoding="utf-8"); links=re.findall(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]",s); missing=[x for x in sorted(set(links)) if not (p.parent/(x+".md")).exists()]; assert not missing, missing; print(f"PASS wikilinks={len(links)}")'
```

Expected：`PASS wikilinks=<正整数>`。

- [ ] **Step 2: 校验状态词和当前进度没有虚报**

Run：

```bash
python -c 'from pathlib import Path; s=Path("Prima OS 实现阶段.md").read_text(encoding="utf-8"); allowed=["未开始","进行中","阻塞","已满足退出条件"]; import re; states=re.findall(r"\*\*状态：([^*；]+)",s); bad=[x for x in states if x.strip() not in allowed]; assert not bad, bad; assert "当前实现代码 | 尚无" in s; assert "尚无内核实现、机器证明、QEMU 启动证据或兼容性测试证据" in s; print(f"PASS states={len(states)} no false completion")'
```

Expected：输出 `PASS`，且没有阶段 1–7 被标记为“已满足退出条件”。

- [ ] **Step 3: 校验无占位符和旧绝对主张**

Run：

```bash
python -c 'from pathlib import Path; s=Path("Prima OS 实现阶段.md").read_text(encoding="utf-8"); forbidden=["TBD","TODO","兼容一切操作系统","六条原语的组合闭包","已经形式化证明安全"]; found=[x for x in forbidden if x in s]; assert not found, found; print("PASS placeholders=0 stale_claims=0")'
```

Expected：

```text
PASS placeholders=0 stale_claims=0
```

- [ ] **Step 4: 校验 Markdown 结构和完整需求**

Run：

```bash
python -c 'from pathlib import Path; import re; p=Path("Prima OS 实现阶段.md"); s=p.read_text(encoding="utf-8"); required=["## 维护规则","## 当前快照","## 证据索引","## 当前阻塞项","## 决策日志","## 阶段晋级规则","[[Prima OS]]","[[COSIR 原语规范]]","deadline ≤ now","OS 中立的 syscall / exception 递送机制"]; missing=[x for x in required if x not in s]; stages=[int(x) for x in re.findall(r"^## 阶段 (\d+) —",s,re.M)]; fence=chr(96)*3; fences=len(re.findall("^"+fence,s,re.M)); assert not missing, missing; assert stages==list(range(8)), stages; assert fences%2==0; print(f"PASS lines={len(s.splitlines())} stages={len(stages)} fences={fences}")'
```

Expected：`PASS`，`stages=8`，围栏数为偶数。

- [ ] **Step 5: 人工核对权威边界**

逐条确认：

```text
[ ] 阶段名称、交付和退出条件与 Prima OS.md 第 15 章一致
[ ] 没有复制 COSIR 原语规范中的完整函数签名或 W1–W14 正文
[ ] 当前阶段只标记为阶段 0 / 进行中
[ ] S0-D1、S0-D2 只标记“已定义，未证明”
[ ] S0-X1、S0-X2、S0-X3 均未勾选
[ ] 阶段 1–6 所有实现与退出条件均未勾选
[ ] 阶段 7 明确为候选范围，不是实现承诺
[ ] 每个未来完成声明都要求证据 ID
```

若任一项不满足，只修正 `Prima OS 实现阶段.md` 后，重新运行 Steps 1–4。

---

## Plan Self-Review

- **Spec coverage:** 已覆盖 `Prima OS.md:682-814` 的阶段 0–7、交付项、退出条件和阶段 7 范围约束。
- **Authority boundary:** 明确引用 `Prima OS.md` 与 `COSIR 原语规范.md`，不复制操作语义。
- **Current truth:** 当前只有文档工件；阶段 0 为进行中，所有实现与验证退出条件保持未完成。
- **Placeholder scan:** 计划写入内容不含 `TBD`、`TODO` 或模糊的“补充测试”等占位指令。
- **Interface consistency:** 所有检查项使用 `S<阶段>-D<编号>` / `S<阶段>-X<编号>`，证据使用稳定 ID，状态词全局统一。
- **Repository constraint:** 目录不是 Git 仓库，计划不包含初始化、commit、push 或 PR 步骤。
