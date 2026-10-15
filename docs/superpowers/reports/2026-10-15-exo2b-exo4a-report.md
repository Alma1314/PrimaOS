# SPEC-117 / EXO-2B + EXO-4A：共享映像格式与设备可见撤销

**日期**：2026-10-15
**阶段**：阶段 4（Exokernel 对齐系列）
**基线**：SPEC-115 / SPEC-116

## 目标

本片包含两个独立但都属于 Exokernel 对齐的实现小步：

1. **EXO-2B**：把 ring3 服务映像目录格式从内核/运行时的重复常量提升为 `prima-abi` 共享格式，
   为后续用户态 loader/manifest 提供单一契约。
2. **EXO-4A**：为 `Device` 对象实现 `dev_destroy` 与通用授权路径，给设备 Capability 一条
   可观察的撤销/abort 路径，并补 host 反例。

## EXO-2B：共享服务映像格式

`prima-abi::boot_image` 新增：

```text
SERVICE_IMAGE_MAGIC
SERVICE_IMAGE_DIR_HEADER
SERVICE_IMAGE_ENTRY_LEN
SERVICE_IMAGE_ENTRY_COUNT
```

`kernel/src/user.rs` 不再定义 `SERVICE_IMAGE_MAGIC`，写目录时直接使用共享常量；
`prima-runtime::protocol` 改为 `pub use` 同一组常量。目录仍无服务名，名字由 ring3
`prima-servicemgr::SERVICE_TABLE` 按索引持有。

这消除了一处内核/运行时格式重复，但**不**改变 ring3 加载逻辑；完整用户态 loader 仍属
后续 EXO-2 工作。

## EXO-4A：`dev_destroy`

### 内核状态机

- 新增 `DeviceTable::remove(id)`；
- 把端口授权共用部分抽为 `authorize_device(caller, cap, right)`；
- 新增 `BootstrapState::dev_destroy(caller, cap)` 与自由函数包装；
- 成功动作：校验 `DevDestroy` / 活动 Device / holder → 审计预检 → 从活动表删除 →
  追加 `ObjectDestroyed{segment:false}`；
- 删除后，旧 cap 在 `authorize_device` 上稳定得到 `DeadDevice`——即可见撤销/abort 路径。

### ABI 与运行时

- `prima-abi::syscall`：新增 `SYS_DEV_DESTROY = 36`；
- `kernel/src/syscall.rs`：新增 `dev_destroy_call` 与分派；
- `prima-runtime`：新增 `Runtime::dev_destroy(cap)`。

### Host 测试

新增两条 kernel 单测：

- `dev_destroy_stales_existing_caps_and_frees_capacity`：旧 cap 失效、活动表容量释放、可再铸；
- `dev_destroy_requires_the_destroy_right`：缺 `DevDestroy` 的派生 cap 被拒且失败原子。

kernel 单测从 376 增至 **378**。

## TCB 耦合面门禁

`cargo xtask tcb` 除原有行数/unsafe 指标外，新增内核-服务耦合面计数，并守住 EXO-1 回归：

```text
TCB kernel_service_coupling include_bytes=0 prima_user_dep=false service_name_lines=17 protocol_lines=227
```

当前 EXO-1 不变量被硬门禁钉住：

- `kernel/src` 中 `include_bytes!` 必须为 0；
- `kernel/Cargo.toml` 不得依赖 `prima-user` artifact 包。

## 验证

- `cargo xtask test`：**6/6 PASS**（BIOS 两次退出码 33、43 条事件、`boot_complete` 正常）。
- `cargo test --workspace`：**795 passed, 0 failed**（kernel 378）。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=21775 total=23037 ratio=5.48%`，机制层 `unsafe=0`。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 未决

1. `dev_destroy` 尚无 ring3 调用者；host 单测覆盖状态机，QEMU 仅验证零回归。
2. 服务映像目录顺序仍由内核与 `SERVICE_TABLE` 注释对齐；完整 EXO-2 需要 manifest/loader。
3. 服务协议 tag、共享段 witness、`user_code()` init 机器码仍未下沉。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
