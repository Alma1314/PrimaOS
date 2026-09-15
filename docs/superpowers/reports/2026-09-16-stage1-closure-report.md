# 切片 P 报告：阶段 1 退出条件——帧唯一归属与初始 Authority 物化

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-16
>
> 对象：`kernel/src/{rights,frames,bootstrap,main,resume,protocol,cow,user,vspace}.rs`、
> `xtask/src/protocol.rs`
>
> 目的：补齐 S1-X3（帧唯一归属）与 S1-X2（初始 Authority）的真实内核实现与可复查证据

## 0. 结论速览

- **S1-X3**：新增 `FrameRegistry`，`frame_universe = free ⊎ 活动 Segment 帧 ⊎ 保留页表帧`，
  QEMU 实测 `free 30843 + segment 16 + table 24 + image 663 = universe 31546`（`conserved=1`）；
  `cow.rs` 的分配器副本已删除，全部取帧经带标签入口。
- **S1-X2**：image Segment、空 Namespace、`root_ns_cap` 结构性根、init `Ready→Running`、
  初始 Audit、`booted`/`AlreadyBooted`、入口 image 范围前置、权限 bitset（去 `"all"` 通配符）
  全部落地。
- 两条阶段末见证 `authority_ready`/`frames_ready` 追加在既有 21 条之后，既有 seq 1..21 与 data
  **除一处有意修正外**逐字节不变。
- **不勾选 S1-X2/S1-X3、不宣告阶段 1 关闭**（组织级独立性门槛，见 §6）。

## 1. 方法与独立性声明

- 全部为真实内核实现 + QEMU 实测 + host 单测；无机器证明。
- 本报告复核独立性为 **harness 内会话级别**，**不是组织级第三方审查**，也不构成
  `REFINEMENT-PROOF`；不改变阶段 0 / 阶段 1 的独立性门槛。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| W6 universe | `universe = usable_frames + image_frames`（§14 步骤 1；image 属 universe） |
| 单一归属点 | `allocate_for_segment(id)` / `allocate_table_frame()` 两个带标签入口；无未标注取帧 API |
| 守恒 | `free + segment + table + image == universe`；不交性由构造保证 |
| 消除状态分叉 | 删除 `CowContext.alloc` 副本，COW 帧经全局注册表 |
| 页表帧记账 | `vspace::map_4k` 与任务根页表帧全部登记为保留帧 |
| 权限 | `Right`(20)/`Rights`(u32) + `well_typed` 类型匹配；`NAMESPACE_RIGHTS` 去通配符 |
| 对象物化 | image Segment（`{R,X}`、CPU、frames=(kernel_phys_start, image_frames)）、空 Namespace |
| init 状态机 | bootstrap `Ready` → `activate_init` `Running` + `TaskStateChanged` |
| 初始 Audit | `Bootstrapped` + 两个 `ObjectCreated` + `TaskStateChanged(Ready)` |
| W14 | `booted` + 已安装时 `AlreadyBooted` |
| §14 前置 3 | `entry_in_image(entry, base, len)`，`create_state` 收 `entry` 参数并校验 |

## 3. 实测证据（2026-09-16）

```
PRIMA|v=1|seq=22|event=authority_ready|status=ok|data=caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=2;init_running=1
PRIMA|v=1|seq=23|event=frames_ready|status=ok|data=universe=31546;free=30843;segment=16;table=24;image=663;conserved=1
```

- `entry_in_image=1` 是以 `kernel_main` 的运行时地址对 `boot_info.kernel_image_offset`
  （0x10000000000）与 `kernel_len` 校验的结果。
- 绝对帧数随内核镜像大小变化（本次 `image=663`，此前一次构建为 639）；xtask 断言的是
  **关系**（W6 等式与 `conserved`），不是绝对常量。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel 225、xtask 125 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS（新事件被断言）。
- `cargo xtask compat`：`PASS compat cow-write-fault`（Linux 有界差分不受影响）。

## 4. 既有事件的变化（显式登记，不隐藏）

**唯一一处**：`bootstrap_ready.namespace_rights` 由 `all` 改为
`Derive,NsResolve,NsBind,NsUnbind,NsMount,NsUnmount,NsDestroy`。

- 原因：§4.1 要求 `rights: FiniteSet<Right>` 且 `well_typed_rights(target, rights)` 做类型匹配；
  通配符 `all` 既不有限也无法类型匹配（改前 `authorize(Namespace cap, SegMapW)` 会被错误放行，
  现已修正为 `MissingRight`，host 单测覆盖）。
- 影响：`xtask/src/protocol.rs` 的期望常量与 golden 同步更新；`bootstrap_ready` 的其余字段与
  其它 21 条既有事件的 seq/data 不变。
- 该变化按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. 边界与反例保留

- image Segment 的 `frames` 用 `(base, count)` 表达，非规范 `Vector<FrameId>`；不替换
  bootloader CR3（§14 步骤 4 的 vspace 映射未物化到真实页表）。
- bring-up Segment 是 ID 标签 + 帧计数，待阶段 2 `memseg_create` 接管。
- **不交性由构造保证 + 守恒计数校验**：无逐帧 owner 映射，故检测不到"同帧计入两个 Segment"
  这类构造外错误（该路径当前不可达）。host 反例覆盖"绕过标签入口取帧 ⇒ 守恒失衡"。
- 未实现：`memseg_*`/`ns_*` 用户可调用原语、`TaskStatus::Faulted`、`REFINEMENT-PROOF`。
- 既有 `cargo xtask test` 的 30s/无网络/精确序列语义不变。

## 6. 关闭口径与下一步

- **S1-X2/S1-X3 不勾选，阶段 1 维持“进行中”**：证据虽齐，独立性仍为 harness 内，
  需范畴外第三方复核——与阶段 0 / S0-X1 同一门槛，不为“收尾”放宽。
- 下一步候选：启动阶段 2（S2-D1 句柄编码、S2-D2 派生/移动/drop 用户化、S2-D3 Segment
  create/map/unmap/destroy；帧注册表与 `SegmentObject` 已为其铺好归属基础）。
