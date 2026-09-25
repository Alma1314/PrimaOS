# 切片 AD 设计：审计存储重构（SPEC-075）

> 文档类型：真实内核实现设计（阶段 2 收尾，第八节）
>
> 日期：2026-09-24
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-24，"三项都解决"）

## 0. 定位与前置

切片 Z（SPEC-071）为补齐 §13.1 的迁移审计，把 `MAX_AUDIT_EVENTS` 从 128 提到 170，
使 `BootstrapState` 达到 **16312 B**、贴着 16 KiB 守卫只剩 **72 B**。当时显式登记：

> 结构体余量只剩 72 B，任何进一步存储增长都**必须先做审计存储重构**（把 `AuditLog` 移出
> `BootstrapState`）。

这条瓶颈是**会卡住后续所有切片**的物理约束（阶段 3 的 Namespace/Timer/Audit 都要往该结构
加存储）。本切片拆掉它。

## 1. 设计

### 1.1 存储与结构体解耦（arena + 每实例区段）

```rust
pub struct AuditLog { base: usize, count: usize }   // 16 B 句柄（原为 170×24 = 4080 B 内联数组）
static mut AUDIT_ARENA: [Option<AuditEvent>; MAX_AUDIT_EVENTS * AUDIT_REGIONS];
static AUDIT_CURSOR: AtomicUsize;                   // 区段分配游标
```

- `AuditLog::claim()` 用原子游标在 arena 里为本实例领一段 `MAX_AUDIT_EVENTS` 槽；
  段耗尽**确定性 panic**（不静默复用他人区段）。
- 条目读写经 `addr_of!` + `ptr::read/write` 直接把索引到 arena 的 `base + i`；**不对外借出
  arena 引用**（新增 `entry(i)` / `for_each` / `any` 三个按值接口），避免 `&'static` 别名的
  不健全性。
- `AUDIT_REGIONS`：内核**只需要 1 段**（只构造一个 `BootstrapState`）；host 单测并行构造多个
  state，故 `#[cfg(test)]` 下取 512 段，使各实例**区段互不重叠**、测试互不干扰。
  **段数与容量都不占 `BootstrapState` 空间。**

### 1.2 容量随之提到 256

存储既已解耦，容量就不必再挤守卫。`MAX_AUDIT_EVENTS` 170 → **256**（实测占用 135，
余量从 39 槽升到 121 槽）。内核 arena 为 1 段 × 256 槽 = 6 KiB 静态。

### 1.3 结构体尺寸

实测 `BootstrapState` 由 **16312 B 降到 12240 B**（释放 **4072 B**），守卫余量从 72 B
升到约 4.1 KiB。守卫测试从"余量已枯竭"改为**钉住 ≤ 13 KiB**——既防再次撞 16 KiB 守卫，
也**防悄悄回归到内联存储**。

## 2. 回归面（逐条登记，实测自事件流逐条 diff）

| 事件 | 改前（SPEC-073） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30645;image=829;kend=0x133d000` | `usable=30648;image=**826**;kend=**0x133a000**` | 结构体不再按值搬移 4 KiB 内联数组，编译器生成的搬移代码随之减少，**镜像缩小 3 帧** |
| `bootstrap_ready` | `image_len=3392488;image_frames=829` | `image_len=3379584;image_frames=826` | 同上 |
| `preempt_ready` | `rsp1=0x1000008eeb8` 等 | `rsp1=0x1000008f508` 等 | 镜像体积变化后内核栈位置随之移动 |
| `frames_ready` | `free=30535;image=829` | `free=**30538**;image=826`；`table=91`/`segment=19` 不变 | 镜像缩小 3 帧 |
| `state_audit_ready` | `…;used=135;cap=170` | `…;used=135;cap=**256**` | 容量提升（用量不变，余量 121 槽） |
| 其余 31 条 | — | **逐字段不变** | 含全部见证与 seq |

**事件序列长度与 seq 全部不变**（本切片不新增事件）。

## 3. 验证

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **305**、xtask **167** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：六条 PASS（35 条事件）。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 边界

- 证据为真实内核 + QEMU，独立性为 **harness 内**；**不勾选**任何交付项与退出条件。
- **`AuditLog` 的相等性语义变化**：`PartialEq` 现在只比较 `count`（不比较区段基址），因为两个
  不同实例的日志在语义上"同长度即同内容"的假设**不成立**。当前无代码依赖 `BootstrapState`
  的整体相等性（`PartialEq` 仍由 derive 提供以便测试比较其它字段），登记为一条语义注意事项。
- **arena 段数在单测下是有限资源**：`AUDIT_REGIONS = 512`，超出即 panic（确定性、可诊断）。
  单测实际构造 64 个 state，余量充足；若未来测试大量循环构造 state，需要提高该常量——
  它**不占结构体空间**，代价已在文档里写明。
- **区段一经领取不回收**：`BootstrapState` 是 `Copy`、无 `Drop`，故段在进程生命周期内不释放。
  内核只需 1 段，实际无影响；单测下由段数上限兜住。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。
