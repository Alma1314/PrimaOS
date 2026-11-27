---
id: docs/superpowers/plans/2026-08-07-exc-resume-slice.md
kind: plan
authority: historical
status: execution-record
lifecycle: historical
updated: 2026-11-27
---

# exception 恢复细化 exc_resume 实现计划

本页是 2026-08-07 host 状态机初建计划的压缩记录，不是现行接口或执行指令。原 978 行代码模板、提交步骤和旧 tracker 替换块可由 `git show 5ebf2791fcf01f21cf6e396227483e74ef1b2dd9:docs/superpowers/plans/2026-08-07-exc-resume-slice.md` 恢复；哈希见[整理记录](../reports/2026-11-27-documentation-consolidation.md)。

原草案的独立 `resume_vaddr`、较短帧和布尔 `seg_ok/exec_ok` stub 后经[2026-08-12 纠偏设计](../specs/2026-08-12-refinement-audit-remediation-design.md)修订；[同步后的设计](../specs/2026-08-07-exc-resume-design.md)及 [COSIR §12.10](../../../COSIR%20原语规范.md#1210-exc_resume--恢复被暂停的异常帧)定义实际契约。不得复制旧模板重新引入已废止接口。

## Global Constraints

- 当时目标：定义异常恢复与 Faulted 细化态，新增不依赖 bootloader 的纯 host `cosir` 状态机；Rust nightly-2026-07-30/2024 edition。
- 只产出 MODEL-TEST，不接真实 IDT/trap/QEMU kernel，不做 Lean/Coq 或真实 iretq；不新增 POSIR 一等原语，不关闭 BLK-001/002，不签署阶段 0。
- 异常帧属于架构细化层；原计划说 0x88/136 字节、16 字节对齐，不能当作现行 wire format（纠偏后为 0xA0/160 字节，恢复 RIP 在帧内）。
- 既有阶段 1 BIOS 启动闭环必须保持；原计划中的测试数量和阶段快照都按当时基线解释，不是当前验收结果。

## File Structure

当时新增 workspace 成员 `cosir`（最初为 bin）：`state.rs` 保存 Task/ExceptionSource/SretFrame/pending 状态，`ops.rs` 放 raise_exception/exc_resume/schedule，`main.rs` 放探针与测试；同步原语规范与阶段证据。当前源码与库布局按[系统地图](../../architecture/system-map.md)定位。

### Task 1: 固定当前验收基线，确认 workspace 可扩展

先运行既有 `cargo xtask test`，预期 BIOS 两轮、镜像与启动六条 PASS；只有 exit 0 才继续。确认并保留其它工作区改动。该输出是原计划的预期，不是本次实跑。

### Task 2: 在 COSIR 规范中定义 exc_resume 与恢复路径

当时计划在 §12.9/12.10、细化要求 #16 和边界 #1/#2 定义自身异常帧恢复：source/订阅、frame Segment 权限/范围/对齐、可执行目标、pending 源匹配、可恢复类型等前置检查；成功消费帧、追加 TaskResumed 并转 Ready。错误时不提交，DoubleFault 保持给用户态决策。原草案成功动作顺序与恢复地址接口不作为现行契约，按上列纠偏设计的验证/提交顺序阅读。

### Task 3: 新增 cosir workspace 成员与状态数据模型

先扩 workspace 与 host 包，再定义 Task/ExcSource/Segment/虚拟地址、寄存器帧、异常种类、TaskStatus、PendingException 和 State。骨架阶段的 noop、AlreadyResumedStub、未用 FaultCause 仅为过渡，不能作为完成实现。

### Task 4: 实现状态转移与测试

原执行顺序：Running→Faulted 并存 pending；恢复只在检查成功后清 pending、置 Ready；schedule 只把 Ready→Running，不隐式恢复 Faulted。早期模板以 `seg_ok/exec_ok` 替代实际授权/帧校验，订阅只由 pending 绑定代表；这些限制保留为纠偏背景。

原计划列出九类测试：raise 转 Faulted、resume 到 Ready、无 pending、重复消费、DoubleFault 不恢复且不销毁、坏帧地址、不可执行目标、Faulted 调度不重播、Ready 正常调度。`cargo test -p cosir` 预期九项通过，`cargo run -p cosir --bin cosir-state` 预期输出 raise/resume/schedule/roundtrip；不得将预期输出当作实测。

### Task 5: 完整多成员静态校验，确认不破坏启动闭环

顺序为 fmt、workspace Clippy、workspace 单测，再运行 `cargo xtask test` 确认原双轮 BIOS；host 单测不运行 QEMU。旧文件中“确认被破坏”的步骤标题与其保持闭环的目标矛盾，本压缩按原预期意图表述，不改变当时实际结果。

### Task 6: 登记阶段 0 细化产出证据，同步实现阶段文档

仅在实际命令通过后计划登记 SPEC-012，关联 S0-D1/S0-D2，移除已定义 exc_resume 的规范缺口；保留 BLK-001/002 和其它未完成退出条件。原计划没有独立实跑记录，本次不补记 PASS，也不据旧脚本修改现有 tracker。真实内核后继见[阶段 2 报告](../reports/2026-09-24-stage2-exc-resume-report.md)，Lean 的独立简化模型见[映照契约](../../../lean/README.md)。

## Plan Self-Review

保留“先契约、再状态机与反例、再全 workspace/双轮 BIOS、最后登记证据”的依赖；现行帧 ABI、完整失败原子性和真实恢复不能由旧 stub 推出。未实现项、失败和证据边界须沿纠偏设计及报告链读取，当前状态只查阶段跟踪。
