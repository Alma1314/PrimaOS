# SPEC-109：单一服务映像段（代码段移交下沉到 ring3 服务管理器）

**日期**：2026-10-14
**阶段**：阶段 4（S4-D3 续，任务 #7 之②）
**基线**：`bfb07c6`（SPEC-108）

## 目标

闭合「加一个服务 = 服务表一行 + 一个二进制」这一不变量：**`init` 与内核都不再逐服务手写代码段移交**。

之前（SPEC-108 后）加一个服务需改 4 处：内核 `include_bytes!` + 建独立代码段 + 铸段 cap、`init` 机器码新增一条移交消息、servicemgr 表一行。本切片把前 3 处收敛为「内核维护一个 ELF 数组」，`init` 侧归零。

## 设计

内核把所有服务 ELF（alpha / beta / servicemgr / log / nsvc / store）**页对齐打包进单一映像段**，段首页是**目录页**：

```
magic(0..8) | count(8..16) | [name(0..8) | offset_pages(8..16) | page_count(16..24)] × count
```

- `SERVICE_IMAGE_MAGIC = 0x5345_5256_494D_4701`
- 目录固定在**段首页**（偏移 0）：管理器只需映射 1 页即可读，且**加服务不改变此偏移**。

段以两条根 cap 供两个用途：
- `SERVICE_IMAGE_CAP_HANDLE`（槽 62）：`init` 自留，用于把各服务代码子区间映射进各自任务根。
- `SERVICE_IMAGE_MGR_CAP_HANDLE`（槽 63）：作为 `prima-servicemgr` 的 **`initial_cap1`**（`task_create` 时**移动**），管理器据此读目录、派生端点代码只读令牌。

`init` 不再发任何 `SERVICE_SETUP_CODE_TAG` 消息（原 5 条 + 5 条无 cap 唤醒全部删除）。

## 上次失败的真因（本次定位）

上一会话同一方案的实现曾在 `seq=12` panic。本次先做**纯静态实验**证伪了当时的归因：

- 在内核里加一页/三页静态数据（`image_frames` 1120→1121→1123），boot **完全正常**（`seq=43 boot_complete`）——「内核体积位移破坏 boot」不成立。
- 复现后逐层用 QEMU 退出码定位（**未加任何串口打印**，避免观察者效应）：
  1. `TaskCreateError` 类别 → `InvalidInitialCap`
  2. 第几次 create → 第 4 次（存活 Rust 客户端）
  3. `InvalidInitialCap` 子条件 → `slots.check(cap) != Live || holder_of(cap) != Some(caller)`

**根因**：请求块（`TASK_REQ_VA`）在页 1 被服务管理器块改写后，`[rdx+0x48]`（`initial_cap1`）被写成映像段 cap；客户端创建**复用了同一请求块**却未重写该字段，于是拿了一个**已被移动给管理器**的句柄。原来管理器的 `initial_cap1 = 0`，复用完恰好还是 0，故从未暴露。

**修复**：客户端创建前显式写回 `initial_cap1 = 0`（`kernel/src/user.rs`，3 条机器码）。

## 改动

| 文件 | 内容 |
|---|---|
| `kernel/src/bootstrap.rs` | `MAX_SEGMENT_FRAMES` 16→48；`TaskCreateImages{image}` 单段；只铸槽 62/63 两条 cap；删 6 个独立代码段（code/code2/code3/log/ns/store）；`SegmentFrames` 加 `large_enum_variant` allow（无堆，帧向量必须内联） |
| `kernel/src/user.rs` | 映像布局常量 + `write_image_directory`；`load_runtime_elf`/`write_image_byte`/`write_image_u64`/`apply_relative_relocations` 加 `page_base`；`init` 机器码删 5 条移交、`initial_cap1` 换映像段、code_offset 改页偏移、客户端 `initial_cap1` 清零 |
| `kernel/src/syscall.rs` | `TASK_CODE_CAP` 改指 `SERVICE_IMAGE_CAP_HANDLE` |
| `prima-runtime/src/protocol.rs` | 目录常量（magic/header/entry_len） |
| `prima-user/src/servicemgr.rs` | `_start` 读 `initial_cap1` 首页目录建服务表；`Pending` 增 `offset_pages`；删 `SERVICE_SETUP_CODE_TAG` 分支 |
| `xtask/src/protocol.rs` | 4 处金标 + 1 处夹具（见下） |

## 事件登记（逐条）

**xtask 金标更新**（结构性 cap/段变化的必然位移）：

| 事件 | 字段 | 旧 | 新 | 依据 |
|---|---|---|---|---|
| `ipc_ready`(23) | queued | 28 | 18 | 删 5 条移交 + 5 条唤醒 = −10 |
| `task_ready`(24) | caps / caps_after | 101 | 97 | 删 6 个代码段 cap、加 2 个映像段 cap，净 −4 |
| `service_negotiated_ready`(27) | token | `0x1_0000_0038` | `0x2_0000_0033` | 派生 cap 槽分配前移 |
| `service_negotiated_ready`(27) | service | `0x1_0000_003d` | `0x1_0000_0038` | 管理器 cap 槽前移 |

**自动通过（关系不变量，无需改）**：`service_selected_ready` 的三条 `*_stack_seg` id（段 id 前移）、`frames_ready` 的 `image`/`segment`、`state_audit_ready.used`、`servicemgr_ready` 的 `entry_a/entry_b`、`fault/reclaim/resume_ready` 的 task id。

## 验证

- `cargo xtask test`：**6/6 PASS**（build kernel / image bios / image uefi / qemu bios run=1 / run=2 / stage1 boot slice）
- `cargo xtask lint`：**4/4 PASS**（含 `-D warnings`、`deny(unsafe_code)`）
- `cargo test --workspace`：**788 passed, 0 failed**
- `cargo xtask tcb`：`mechanism_lines` 21146→**21069**，`ratio=5.65%`，机制层 `unsafe=0`
- QEMU 退出码 33，`seq=43 boot_complete|status=ok`

## 已排除（记录以免重走）

- 内核体积/布局位移（见上，已静态证伪）
- cap 槽 68/69 与保留区间冲突（改用 62/63 后症状不变；真因在客户端请求块复用）
- `prepare` / `prepare_task2` / `prepare_spin` / `prepare_task_create` / `build_kernel_root` 的显式错误分支（均加区分退出码后未命中）
