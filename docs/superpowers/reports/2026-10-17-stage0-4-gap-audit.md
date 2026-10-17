# 阶段 0–4 缺口复核包

**日期**：2026-10-17
**核对基线**：`e2b17b2`（SPEC-133）；本报告同步核对了后续 `f0399a5` 的 SPEC-134 文档更新。
**判定口径**：按 `Prima OS 实现阶段.md` 的退出条件和独立性门槛；历史报告的局部“未完成”不能覆盖后来证据，测试通过也不能替代组织级第三方复核。

## 当前判定

| 阶段 | 已具备的证据 | 仍需完成的事项 |
|---|---|---|
| 0 | S0-D1–D4、S0-X2/X3。SPEC-054 与 SPEC-134 覆盖现行 34 项常规操作的成功/失败语义；`lake build` 通过 | **S0-X1**：范畴外、组织级第三方复核 SPEC-055–059 修订后的对象生命周期规范。不能由本仓库作者或同一 harness 自行签署 |
| 1 | S1-D1–D5、S1-X1 的构建与 QEMU 证据；SPEC-061/TEST-008 覆盖初始 Authority 和帧归属 | **S1-X2/X3** 的范畴外复核；同时裁决 `ISSUE-FRAME-001` 的逐帧 owner 解释、`ISSUE-AUTH-001` 的 §14 image 与真实 init/内核镜像映射差异 |
| 2 | S2-D1–D5 与 X1–X4 的实现侧证据见 SPEC-062–075；SPEC-067 已实现自毁，SPEC-071 已补调度迁移 Audit，原 S2-X1 文案已纠正 | 交付项与退出条件仍按既定独立性门槛未勾选；复核 `ISSUE-TASK-001/003` 的已实现行为与残留偏差，以及跨任务授权/清零/stale 句柄负例 |
| 3 | SPEC-081–083 的 Namespace、Timer、`audit(cap)` 实现与见证覆盖 D1–D5、X1–X4；SPEC-083 §7 列出逐项映射 | 交付项与退出条件待范畴外复核，不能把 harness 内审查当作组织级结论 |
| 4 | SPEC-130 覆盖 D1–D5/X1–X4 的技术退出条件，SPEC-131–133/135 继续硬化；本轮 BIOS QEMU 双次通过 | 技术退出条件已具证据；**正式晋级**受 S0-X1 的阶段顺序约束。EXO-3 完整 ring 与静态缓冲退出、EXO-4 其他设备路径、DOC-007 新机制实现与 W17 证明是另列的后续工作，现有阶段 4 退出定义未要求它们 |

## 本轮可直接闭合的证据漂移

1. DOC-007 新增 `memseg_protect` 与 7 项 WaitObject/upcall 操作后，SPEC-054 的 26 项覆盖面已过期。SPEC-134 逐项复核 8 项增量及 3 项联动修订，跟踪文档 S0-X2 引用已补到 34 项；白皮书“ S0-X2 未补齐”的历史快照已修订。此处只关闭**规范文本覆盖差异**，不宣称这 8 项已有可执行实现。
2. 跟踪文档 S2-X1 原把 `TaskStateChanged` 列为仍缺，和 SPEC-071/TEST-018 的已交付证据冲突；现改为“实现侧已补，独立复核未过”。
3. 跟踪文档的“当前首要缺口”原混入 2026-10-10 的已完成握手和服务名切片，现用本表替换，避免把历史工作误读为当前阻塞。

## 2026-10-17 复验

| 命令 | 结果与证据范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace --quiet` | 801 passed；host 模型、内核、runtime、user、xtask 测试 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | fmt、Clippy、`x86_64-unknown-none` 内核构建、`kernel/src` 零 `unsafe` 四项 PASS |
| `CARGO_NET_OFFLINE=true cargo xtask test` | BIOS QEMU run=1/2、stage1 boot slice 六项 PASS；包含阶段 4 原生应用、驱动故障隔离、原始 trace 验收 |
| `CARGO_NET_OFFLINE=true cargo xtask compat` | `PASS compat cow-write-fault`；仅限有界 COW 差分 |
| `lake build`（`lean/`） | 8 jobs 成功；不是全规范细化证明 |
| `CARGO_NET_OFFLINE=true cargo xtask tcb` | trusted=1262、mechanism=20231、ratio=5.87%、机制层 unsafe=0；服务耦合 `include_bytes=0`、`prima_user_dep=false`、`service_name_lines=1`、`protocol_lines=94` |

## 供范畴外审查者逐项签署

1. **S0-X1**：从 `COSIR 原语规范.md` 的对象生命周期、W1–W17 与 §17 义务出发，逐条重放 [SPEC-055](2026-09-13-s0x1-independent-lifecycle-review.md)、[SPEC-057](2026-09-13-s0x1-fix-reverification.md)、[SPEC-058](2026-09-13-spec058-lifecycle-unification.md) 和跟踪文档所记 SPEC-059 的反例/补正；记录审查者身份、基线提交、发现、每项 verdict。发现新空洞则登记 ISSUE 后保持 S0-X1 未勾选。
2. **S1**：依据 [SPEC-061](2026-09-16-stage1-closure-report.md) 复核 init 的初始 Authority、逐帧所有权和 `ISSUE-FRAME-001` / `ISSUE-AUTH-001` 的规范解释；对仍不一致之处给出修正规范或实现的明确结论。
3. **S2–S3**：核对 [SPEC-067](2026-09-17-stage2-task-self-destroy-report.md)、[SPEC-071](2026-09-24-stage2-scheduler-state-audit-report.md)、[SPEC-083](2026-09-25-stage3-audit-trace-report.md) 及相应 host/QEMU 反例，逐项决定 D/X 是否可勾选；不能仅凭本轮全绿测试推定覆盖了所有规范状态迁移。
4. **阶段顺序**：只有上述审查结论满足各阶段退出条件后，依次更新跟踪文档的阶段状态与证据 ID。若 S0-X1 未获签署，当前阶段继续为 0，阶段 4 保留“技术退出条件已满足”表述。

## 已登记而未冒称关闭的范围

- `ISSUE-CAP-001` 的 host 模型位置表/完整溯源、`ISSUE-FRAME-001`、`ISSUE-AUTH-001`、`ISSUE-TASK-001/003` 的剩余解释或实现偏差，按跟踪文档状态保留。
- `ISSUE-EXO-001` 已部分处置：服务 ELF 与构建依赖从内核移出、服务固定偏移 witness 迁至 trace；SPEC-135 又加入 13 条服务记录的共享页原始字节流。内核仍有静态 trace buffer 和少量通用 Y 页机制回执，任意长度字节流/共享 ring 未交付。
- DOC-007 的 8 项新增操作只有规范文本覆盖；WaitObject/upcall、`memseg_protect` 的 cosir/内核实现以及 W17/§17.9 证明尚未交付。SPEC-134 不把这些列为 S0-X2 文本覆盖缺口，也不把它们记成已实现。
