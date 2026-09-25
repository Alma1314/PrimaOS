# 切片 AJ 报告：阶段 3 第一片 —— Namespace 路径操作（§11 全量）

> 文档类型：真实内核实现报告（阶段 3 第一片）
>
> 日期：2026-09-25
>
> 设计：`docs/superpowers/specs/2026-09-25-stage3-namespace-design.md`

## 1. 交付物

| 交付 | 内容 |
|---|---|
| `Path` 与条目类型 | 新增 `kernel/src/namespace.rs`：`Component`/`Path`（§3.1 全部约束逐条对应）、`NsEntryKind`/`NsEntry`、`is_prefix_of`/`strict_prefix_of`/`parent`/`extend`/`suffix_from`/`render`；13 条单测 |
| 通用分段 arena | 新增 `kernel/src/arena.rs`：`Arena<T, SLOTS, REGIONS>`（`StaticCell` + 原子段游标，`claim`/`get`/`set`），内核 1 段 / `#[cfg(test)]` 512 段；5 条单测（含段不重叠与耗尽 panic） |
| Namespace 条目存储 | `NsEntryTable`（arena 句柄 + 8 个查询/更新原语）；`MAX_NS_ENTRIES = 16`、`MAX_NAMESPACE_OBJECTS` 2→8 |
| 位置模型 | `CapabilityLocation::InNamespace(u32)` + `namespace_id()`；`holder_id()` 对它返回 `None`；新增 `move_to_namespace`/`move_from_namespace`；`validate_authority` 增条目一致性分支 |
| 七个操作 | `ns_create`/`ns_bind`/`ns_unbind`/`ns_mount`/`ns_unmount`/`ns_resolve`/`ns_destroy`（`BootstrapState` 方法 + 全局包装）；`NsError` 15 个规范错误名 + 2 个实现层错误 |
| 环检测 | `namespace_reaches`：沿 `Mount` 边的 DFS + `visited` 位集（节点 ≤ `MAX_NAMESPACE_OBJECTS`，**确定性终止**）；解析终止由「每消费一个分量就缩短剩余路径」保证 |
| 审计事件 | `AuditEvent` 增 6 个变体：`CapabilityMinted`（§13.1 `CapMinted`）、`NsBound`/`NsUnbound`/`NsMounted`/`NsUnmounted`/`NsResolved`（含起始 cap、规范化路径、经过的挂载 cap、最终父项） |
| ring3 面 | 7 个 syscall（21–27）+ 路径文本按 `copy_in` 分段读入 + 2 个新 errno（`EEXIST`/`ENOTDIR`）；`user.rs` 新增约 110 条指令的 CPL3 见证块（`USER_CODE_LEN` 2980→4056） |
| 协议 | 新事件 `namespace_ready`（seq 25）+ `validate_namespace_data` + 6 条反例单测 |

## 2. 判决性见证（`namespace_ready`，seq 25）

```
namespaces=3;entries=1;binds=2;unbinds=1;mounts=2;unmounts=1;resolves=2;
cycles_rejected=1;overlaps_rejected=1;invalid_path_rejected=1;acyclic_ok=2;
create_ok=1;resolve_ok=1;overlap_rejected=1;cycle_rejected=1;evidence_ok=1
```

逐项判决含义：

- `namespaces=3`：init + a + b 存活（c 已被 `ns_destroy` 从活动表删除）；
- `entries=1`：只有 a 的 `x`（`Mount(b)`）留下——init 的 `lib`/`mnt` 分别被 `ns_unbind`/`ns_unmount`
  清掉，c 的 `sub` 随 `ns_destroy` 删除；
- `binds=2 / unbinds=1 / mounts=2 / unmounts=1 / resolves=2`：每类 `Ns*` 审计事件条数与 ring3
  实际执行的成功路径操作**一一对应**（§13.1）；
- `cycles_rejected=1` + `cycle_rejected=1`：`mount(b_copy, "y", ns_a)` 在「a 沿边可达 b」时被
  拒为 `MountCycle`（EINVAL）——**这是 S3-X1 的核心见证**；
- `acyclic_ok=2`：两次成功 `ns_mount` 都在提交前跑完环检测 DFS 并判无环；
- `overlaps_rejected=1` / `invalid_path_rejected=1`：同名绑定得 `EEXIST`、前导 `/` 的路径文本得
  `EINVAL`；
- 跨挂载解析 `resolve(init, "mnt/sub")` 成功：路径先匹配 init 的 `mnt`（`Mount`）切到子 Namespace，
  再匹配其内的 `sub`（`Bound`），最终父项是绑定项的 Capability。

## 3. 实施中的失败与反例（全部如实保留）

1. **指令长度写错导致三重故障（真实缺陷，QEMU 抓到，host 单测抓不到）**：`mov [rcx+disp32], rax`
   与 `mov r64, [rcx+disp32]` 都是 **7 字节**（3 字节前缀 + 4 字节 disp32），我按 6 字节推进
   `at`，于是每条回执写入都覆盖了下一条指令的首字节——**整段指令流错位**，内核在用户态执行期
   三重故障（QEMU 退出码 0，无任何 panic 事件）。定位手段：临时 dump 生成字节 + 二分提前 `int3`。
   **教训**：`int3` 之前的机器码错位**不会**报编译错，也不会被 `user_code_bytes_are_fixed`
  这类只断言块首/块尾的测试发现；只有 QEMU 端到端能暴露。已把「块内必须恰有一次 `int3`、其后
   到路径数据区之间为零填充」写进单测。
2. **`Path` 文本形式误读规范（由单测暴露）**：初版 `render` 输出前导 `/`（`/a/b`），测试亦按
   `"/lib"` 书写。§3.1 约束 3 明写「原始文本不得包含 `//` 或**首尾** `/`」——因为它会产生空分量。
   修正为「分量以 `/` 连接、无首尾斜杠」，测试与设计文档同步更正。
3. **ring3 序列的权限取值错误（QEMU 抓到）**：`resolve(init, "lib", {NsResolve})` 的父项是绑定项
   （Segment cap 的派生，权限 `{Derive, SegMapR}`），请求 `NsResolve` 超出父权限 ⇒ `RightsExceeded`
   ⇒ 审计计数 `resolves` 只有 1 而非 2。改为请求 `{SegMapR}`。host 单测覆盖了「subset 被拒」这条
   语义，但**序列里的取值**只有端到端计数能发现。
4. **`ns_destroy`/`ns_unmount` 误释放 Capability 槽（口径修正）**：初版按 `drop_capability` 的写法
   释放槽位（`slots.free` + `records.remove`）。但 §4.6 只说「删除**位置**」，`CapRecord` 不变；对
   `Bound` 的初始 Capability（如 `IMAGE_CAP_HANDLE`，槽 1 是保留槽）误释放会破坏初始 Authority。
   改为**只删位置**并登记细化层后果（见 §6 第 3 条）。

## 4. 回归面实测（事件流逐条 diff，对照 HEAD `ac3f22d`）

**新增 1 条事件**（`namespace_ready` 插在 `task_ready` 之后），故 seq 25 起顺移一位
（`preempt_ready` 25→26 … `boot_complete` 35→36）。

**8 条既有事件的数据变化**（根因均为镜像体积/布局，无语义变化）：

| 事件 | 字段 | 旧 → 新 |
|---|---|---|
| `memory_ready` | `usable` / `image` / `kend` | 30587 → 30507 / 886 → 965 / 0x1376000 → 0x13c5000 |
| `bootstrap_ready` | `image_len` | 3628064 → 3950576（+322512 B） |
| `bootstrap_ready` | `image_frames` / `free_frames` | 886 → 965 / 30587 → 30507 |
| `task_ready` | `caps` / `caps_after` | 29 → 40 |
| `preempt_ready` | `rsp1`/`rsp2`/`kstack*_top` | 后移（`root*`/`cr3*`/`data*`/`mark*` 不变） |
| `frames_ready` | `universe` / `free` / `image` | 31473 → 31472 / 30477 → 30397 / 886 → 965（`segment=19`/`table=91` 不变） |
| `self_root_ready` | `pages` | 178 → 202 |
| `state_audit_ready` | `used` | 135 → 163 |

**其余 27 条逐字段不变**（含 `page_table_ready.translated`、全部用户阶段事件、
`platform_segments_ready`、`authority_ready`、`fault_ready`、`reclaim_ready`、`exc_resume_ready`、
`resume_ready`）。

`task_ready.caps` 的 +11 是真实增量：3 条 Namespace 根 cap（预留槽 52/53/54）+ 2 条 derive +
2 条 resolve 派生 + 1 条挂载子 cap + 3 条 `ns_destroy` 后仍由调用者持有的 stale token。
**同镜像连跑两次输出逐字节相同**（确定性复核）。

## 5. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **343**（306→343）、prima-ostd 21、xtask **182**（176→182） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·**27 文件**·0 命中） |
| `cargo xtask tcb` | `trusted_lines=1217 mechanism_lines=17698 ratio=6.43%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**36 条事件**） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

**G3 基线复测**：占比 7.35% → **6.43%**（可信面不变、机制层 +2359 行，故**下降**）。这不是
tracker 记的「若把机制搬进框架则应上升」那一类变化——本片只加机制层代码，可信面未动；按维护
规则 5 复测并登记。

## 6. 口径偏差登记

1. **细化层容量上限**：`MAX_COMPONENT_LEN=16`（规范允许 ≤255）、`MAX_PATH_COMPONENTS=4`
   （规范命名为 `MAX_PATH_COMPONENTS` 但未给值）、`MAX_NS_ENTRIES=16`（规范未定）。都**比规范
   允许的更紧**，不是放宽。
2. **`InNamespace` 位置不带路径**：位置只编码 `ns` id，路径由 `NsEntry` 与 `Ns*` 审计事件承载
   （§4.6「移动不改变 `CapRecord`」，且位置表按槽直索引）。
3. **无位置 Capability 不回收槽（实现层泄漏）**：§4.6/§11.6/§11.8 只要求「删除位置」，细化层没有
   「无位置 Capability」的表示，故这些槽位在 `ns_unmount`/`ns_destroy` 后成为泄漏，由
   `MAX_CAPABILITY_SLOTS = 64` 兜住。**这是规范的直接后果，不是实现选择**——登记为容量缺口。
4. **`CapabilityMinted` 的引入**：§13.1 列有 `CapMinted`，既有实现只有 `CapabilityDerived`；本片
   为 `ns_create` 补上。`bootstrap` 期的三条初始根仍用 `InitialCapability` 表示、**不**补发该事件。
5. **`ns_create` 的根 Capability 用预留槽区间**（52 起，同 SPEC-066 的平台 cap 取向）：ring3 见证
   程序以 const fn 拼装机码，句柄必须编译期可知。
6. **`AuditEvent` 尺寸 24 B → 约 136 B**（`NsResolved` 最大）：存储仍在 `AUDIT_ARENA` 中，
   **不占 `BootstrapState`**（13 KiB 守卫不受影响）；内核只 1 段（约 35 KB），host 测试 512 段
   （约 17 MB bss，惰性）。`BootstrapState` 新增字段合计 8+4 B（arena 句柄 + 拒绝计数）。
7. **`namespace_reaches` 的自挂判定**：`namespace_reaches(ns, ns)` 恒真，故 `ns_mount(ns, p, ns)`
   得 `MountCycle`。规范未显式讨论自挂，本实现按「引入环」处理（已有单测）。
8. **safe-by-construction**：本片沿用既有的「信任调用方」契约，不宣称 sound。
9. **`record.holder` 与位置的分离**：`ns_mount` 派生的挂载子 Capability 位于 Namespace 中、无 Task
   持有者，其 `record.holder` 记为派生调用者（唯一可归属的 actor）。既有 `exc_subscribe` 用
   `record.holder` 而非位置判定的口径差异**不在本片修正**，登记为注意事项。

## 7. 遗留与后续

- 阶段 3 的 **Timer**（S3-D3/D4）与 **`audit(cap)` 查询**（S3-X3）未做，属切片 AK/AL。
- **不勾选任何阶段 3 交付项或退出条件**——沿用阶段 0/1/2 的组织级独立性门槛，闭合判断交复核者。
- `AUDIT_ARENA` 未迁移到通用 `Arena`（两份段分配逻辑并存），登记为遗留。
