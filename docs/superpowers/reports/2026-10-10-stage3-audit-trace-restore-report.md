# SPEC-083 实现恢复报告：`audit(cap)` 在 master 上落地（§13.2）

> 文档类型：实现恢复 + 复核报告（不新增规范能力）
>
> 日期：2026-10-10
>
> 设计依据：`docs/superpowers/specs/2026-09-25-stage3-audit-trace-design.md`（2026-09-25 已冻结）
>
> 原始实现记录：`docs/superpowers/reports/2026-09-25-stage3-audit-trace-report.md`

## 1. 背景：文档已提交，实现未进 master

2026-09-25 的提交 `f4f3441`（标注为「§13.2 `audit(cap)` trace query — stage 3 closing slice」）经核对**只改动了 3 个文件，全部是文档**（tracker + 设计 + 报告），**没有任何代码**。

`SYS_AUDIT` / `AuditTrace` / `audit_ready` 的实现只存在于一个**游离提交** `0da3177`（`Codex worktree snapshot: archive-cleanup`）：

- `git branch --contains 0da3177` 为空——它不属于任何分支；
- 其父提交正是 `f4f3441`，即「文档先提交、代码留在 worktree」；
- master 走了另一条线（`4b0b039` perf → `dd1c08b` perf → `8a25e49` stage4 → …），未包含它。

在恢复之前，master 上 `grep -rn "SYS_AUDIT\|AuditTrace\|audit_trace" kernel/src xtask/src` 命中数为 **0**；`cargo xtask test` 的 38 条事件里也没有 `audit_ready`——而 tracker 仍宣称该片已落地。**这是 tracker 与 master 代码不一致的状态，本报告予以修正。**

## 2. 恢复范围

从 `0da3177` 分离出属于 SPEC-083 的改动（排除该快照里的 stage4 `prima-native-demo` 路线），重放到 master：

| 文件 | 恢复内容 |
|---|---|
| `kernel/src/bootstrap.rs` | `AuditEvent::ObjectDestroyed` 补 `segment`；`MAX_TRACE_CHAIN` / `AuditTrace` / `AuditError`；`ObjectRef::id()`；`BootstrapState::audit_trace` / `audit_len`；4 个 `ObjectDestroyed` 调用点标类型；全局 `audit_trace` / `audit_len`；4 条 host 单测 + 1 条 S3-X4 反例 |
| `kernel/src/syscall.rs` | `pack_audit` / `audit_call`；`AUDIT_RECEIPT_LIVE`/`_STALE`；`cache_audit_receipts`；`AuditWitness` / `audit_witness`；`SYS_AUDIT` 分派臂 |
| `prima-abi/src/lib.rs` | `SYS_AUDIT = 32`（编号此前空缺，master 已把调用号迁到共享 crate） |
| `kernel/src/protocol.rs` | `audit_ready` 事件发射 |
| `kernel/src/resume.rs` | `after_resume` 上报；`user_trap_exit` 缓存回执 |
| `kernel/src/user.rs` | ring3 的两次 `SYS_AUDIT` 见证块 |
| `xtask/src/protocol.rs` | `EventKind::AuditReady`、`EXPECTED_EVENTS`、`parse_line`、`validate_audit_data` + 3 条反例单测、`VALID` 样本 |

### 2.1 与 `0da3177` 原实现的差异（逐条）

1. **调用号来源**：原实现在 `kernel/src/syscall.rs` 直接定义 `SYS_AUDIT = 32`；master 已由 SPEC-084 把调用号集中到 `prima-abi`，故改为在该 crate 新增并由 `syscall.rs` 重导出。
2. **事件位置**：原实现 `audit_ready` 是 **seq 28**；master 另有 SPEC-084 的 `runtime_hello_ready`（seq 25），故恢复后为 **seq 29**，其后 seq 顺移一位。
3. **代码页压力**：master 的 ring3 代码页已接近单页上限（`USER_CODE_LEN = 4096`，`int3` 之前仅余约 30 字节），而 SPEC-083 的 ring3 块需 42 字节。恢复时在**路径数据块之后**把 20 处 `mov rax, imm64`（10 字节）改为 `mov eax, imm32`（5 字节，新增 `put_syscall`），省出 100 字节。该区域**不覆盖任何既有测试断言**（唯一相关断言 `code[2979..2981]` 落在压缩区起点之前），且调用号与本文用到的立即数均 ≤ `u32::MAX`，32 位写入清零 `rax` 高 32 位不影响语义。
4. **注释语言**：恢复时按 master 的代码风格把部分英文注释改为中文（实现逻辑逐行一致）。
5. **既有报告的两处出入**（以本报告的实测为准）：`2026-09-25` 报告的见证串漏写了 `purity_ok=1`（实现与断言一直有该字段），且其引用的 S3-X4 单测名与本实现落地的名字不同。

## 3. 判决性见证（`audit_ready`，master 上实测为 seq 29）

```
chain_len=2;derived=1;moves=2;created=1;destroyed=0;stale_ok=1;purity_ok=1;evidence_ok=1
```

- `chain_len=2`：不可变 parent 链 = image 根 Capability → `ns` 块派生的 `child1`（**S3-X3** 派生链重建）；
- `derived=1` / `moves=2`：目标自身的派生事件 + `ns_bind`/`ns_unbind` 两次位置移动——**位置历史完整可重建**；
- `created=1 / destroyed=0`：目标 image Segment 的创建事件，未销毁；
- `stale_ok=1`：对**目标已销毁**的 Capability（`ns_destroy` 后的 `c_copy`）查询仍返回非负——§13.2 明示不构成失败；
- `purity_ok=1`：查询前后内核审计日志长度不变。

**纯查询的独立证据**：`state_audit_ready.used` 在本片前后**保持 176 不变**，`total` 保持 54 不变——ring3 的两次 `SYS_AUDIT` 未追加任何审计事件（否则「重建证据」会被自身查询污染）。

## 4. 回归面（同一工作目录，与恢复前的 master 逐条 diff）

**新增 1 条事件** `audit_ready`（seq 29），其后 seq 顺移一位（`exc_resume_ready` 29→30 … `boot_complete` 38→39），至 **39 条**。

**5 条既有事件的数据变化**（根因均为镜像体积/布局）：

| 事件 | 字段 | 旧 → 新 |
|---|---|---|
| `memory_ready` | `usable` / `image` / `kend` | 30201 → 30184 / 1269 → 1286 / 0x14f5000 → 0x1506000 |
| `bootstrap_ready` | `image_len` | 5196984 → 5265912（+68928 B） |
| `bootstrap_ready` | `image_frames` / `free_frames` | 1269 → 1286 / 30201 → 30184 |
| `preempt_ready` | `rsp*` / `kstack*_top` | 后移（`ticks`/`pre*`/`root*`/`cr3*`/`data*`/`mark*` 全不变，`mark1=0x1`） |
| `frames_ready` | `free` / `image` | 30084 → 30067 / 1269 → 1286（`universe`/`segment`/`table` 不变） |
| `self_root_ready` | `pages` | 434 → 437 |

**其余 33 条逐字段不变**——其中两条是**判决性的「不变」**：

- `state_audit_ready.used` 保持 **176**、`total` 保持 **54**（纯查询不追加事件）；
- `task_ready`（含 `caps`）**完全未变**（audit 块复用 `ns` 块已派生的 `child1`，不再新铸 Capability）。

**同镜像连跑两次，39 条事件逐字节相同**（差异仅为 cargo 构建耗时文本）。

## 5. 验证（2026-10-10）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **365**（360→365）、prima-ostd 21、xtask **192**（189→192） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件·0 命中） |
| `cargo xtask tcb` | `trusted_lines=1250 mechanism_lines=19671 ratio=5.97%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**39 条事件**） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

**G3 基线复测**：占比 6.08% → **5.97%**（机制层 +366 行、可信面 1250 行未动，故占比下降）。

## 6. 未决与登记

1. **SPEC-083 曾在 master 处于「有文档、无实现」状态**（约 2026-09-25 → 2026-10-10）。此期间 tracker 的该片记录与 master 不符；本报告修正为「实现已落地」，并保留该事实以备复核。
2. **游离提交 `0da3177` 的其余内容未处理**：它同时携带另一条 stage4 路线（`prima-native-demo` crate 与 `2026-10-01-stage4-native-*` 文档），master 走的是 `prima-runtime` + `prima-user` 路线。该分支的去留未在本片决定。
3. **代码页容量仍是硬约束**：`USER_CODE_LEN = 4096` 定死（断言 `USER_CODE_LEN == FRAME_SIZE`）。本片靠压缩既有 syscall 编码腾出空间；后续切片若再需 ring3 见证，应扩页或划分新的用户页，而不是继续压缩。
4. **阶段 3 的交付项与退出条件仍一律不勾选**——沿用组织级独立性门槛，闭合判断交复核者。
