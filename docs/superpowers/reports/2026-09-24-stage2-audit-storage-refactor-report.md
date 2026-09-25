# 切片 AD 报告：审计存储重构（SPEC-075）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-24
>
> 对象：`kernel/src/bootstrap.rs`
>
> 目的：拆掉 SPEC-071 登记的**结构性瓶颈**——把审计日志移出 `BootstrapState`

## 0. 结论速览

- **瓶颈已拆**：审计条目从 `BootstrapState` 内联数组（170×24 = 4080 B）改为**静态 arena +
  每实例区段句柄**（`AuditLog { base, count }`，16 B）。结构体实测 **16312 → 12240 B**，
  守卫余量从 **72 B 升到约 4.1 KiB**；容量同时提到 **256**（用量 135，余量 121 槽）。
- **后续切片不再被物理约束卡住**：容量与段数都**不占结构体空间**——扩容只需改常量。
- **host 单测隔离性保住**：段由原子游标分配、各实例区段不重叠，故并行单测互不干扰
  （这是"审计改模块级静态"这一朴素做法会失去的性质，也是当初不直接那么做的原因）。
- **事件序列与 seq 全部不变**（本切片不新增事件）；仅 4 条事件因镜像尺寸变化而数据改变。
- **不勾选任何交付项与退出条件**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不构成 `REFINEMENT-PROOF`。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `AuditLog` | 从 `{ entries: [Option<AuditEvent>; 170], count }` 改为 `{ base, count }` 句柄 + `claim()`；新增按值接口 `entry(i)`/`for_each`/`any`（不借出 arena 引用）；`len`/`any` 仅测试使用故 `#[cfg(test)]` |
| arena | `static mut AUDIT_ARENA: [Option<AuditEvent>; MAX_AUDIT_EVENTS × AUDIT_REGIONS]` + 原子分配游标；`AUDIT_REGIONS` 内核 1 段、测试 512 段 |
| 容量 | `MAX_AUDIT_EVENTS` 170 → **256**（不再受守卫约束） |
| 结构体 | `BootstrapState` 16312 → **12240 B**；守卫测试由"余量已枯竭"改为**钉住 ≤ 13 KiB**（防回归到内联存储） |
| 调用点 | 15 处直接读 `entries` 改为 `entry`/`for_each`/`any`；97 处 `.audit` 因字段名与 `can_append`/`append` 签名不变而无需改动 |

## 3. 实测证据（2026-09-24）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30648;image=826;free=30648;regions=3;kstart=0x1000000;kend=0x133a000
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31474;free=30538;segment=19;table=91;image=826;conserved=1;disjoint=1
PRIMA|v=1|seq=32|event=state_audit_ready|status=ok|data=total=54;to_running=23;to_ready=22;to_blocked=5;to_faulted=4;used=135;cap=256
```

守恒：`30538 + 19 + 91 + 826 = 31474 = universe` ✅；`conserved=1`、`disjoint=1`。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **305**、xtask **167** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，35 条事件序列与 seq **全部不变**。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件 | 变化 | 原因 |
|---|---|---|
| `memory_ready` | `usable` 30645→**30648**、`image` 829→**826**、`kend` 0x133d000→**0x133a000** | 结构体不再按值搬移 4 KiB 内联数组，编译器生成的搬移代码减少，**镜像缩小 3 帧** |
| `bootstrap_ready` | `image_len` 3392488→3379584、`image_frames`→826 | 同上 |
| `preempt_ready` | `rsp1` 0x1000008eeb8→**0x1000008f508**（`rsp2`/`kstack*_top` 同步） | 镜像体积变化后内核栈位置随之移动 |
| `frames_ready` | `free` 30535→**30538**、`image`→826；**`table=91`、`segment=19` 不变** | 镜像缩小 3 帧 |
| `state_audit_ready` | `used=135` 不变、`cap` 170→**256** | 容量提升 |
| 其余 31 条 | **逐字段不变** | 含全部见证 |

以上按维护规则 5 记录，**不视为回归、也不隐藏**。镜像**缩小**是本次重构的正面副作用
（结构体变小 → 按值搬移的代码变少）。

## 5. host 单测

无新增单测；改动集中在"守卫测试"的语义变化上：`bootstrap_state_stays_within_the_stack_budget`
从"断言余量已枯竭（≥ 16384-128）"改为"断言 ≤ 13 KiB"，使**回归到内联存储**会被立刻发现。
另有 15 处审计读取点由 `entries` 下标改为按值接口，行为由既有测试覆盖（305 条全绿）。

## 6. 边界与反例保留

- **`AuditLog::eq` 只比 `count`**：区段基址不同但长度相同的两份日志会被判为相等。当前没有代码
  依赖"两份日志相等"这一语义（`BootstrapState` 的 `PartialEq` 仅供测试比较其它字段）。登记为
  一条语义注意事项；若将来需要真正的日志比较，应显式实现逐条比较。
- **区段不回收**：`BootstrapState` 是 `Copy`、无 `Drop`，领取的段在进程生命周期内不释放。
  内核只领 1 段，无实际影响；单测由 `AUDIT_REGIONS = 512` 兜住（超出即确定性 panic）。
  **该常量不占结构体空间**——这正是重构要达到的效果。
- **`#[cfg(test)]` 下的段数与实现不同**：内核 1 段 / 测试 512 段。这是有意为之（内核只有一份
  state），已在常量文档注释里写明理由。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。

## 7. 下一步

- 三项待解的"后续工作项"已完成两项（判据收紧 SPEC-074、审计重构 SPEC-075）。
- 剩余一项：**§14 `{R,X}` 的链接脚本级段切分**（X 登记的未解规范冲突）。
- 更远：六节 + 本两项的范畴外复核（组织级门槛）；阶段 3 起点（Namespace / Timer / Audit）
  ——审计重构已为其腾出结构与容量空间。
