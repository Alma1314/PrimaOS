# SPEC-113：设备 `IOPortRange` 对象与受控 I/O 端口路径（S4-D4 内核侧）

**日期**：2026-10-15
**阶段**：阶段 4（S4-D4 第一步）
**基线**：`e922241`

## 目标

S4-D4 要「第一条用户态驱动**或受控 I/O 路径**」。内核此前**没有任何设备/端口委派机制**——所有
端口 I/O（COM1/PIC/PIT/QEMU 退出口）都是内核直做，ring3 无一字端口权限。本片按 `COSIR 原语
规范.md` §15 **落地设备对象模型与受控 I/O 端口路径的内核侧**：端口权限从此是**一等 Capability**。

## 设计（照规范命名，不另创）

| 规范条目 | 实现 |
|---|---|
| §15.2 `ObjectRef::Device(DevId)` | `kernel/src/bootstrap.rs` 的 `ObjectRef::Device(u32)` + `DeviceObject`/`DeviceTable` |
| §15.2 `DeviceKind = … | IOPortRange | …` | 本片**仅** `IOPortRange { base_port, port_count }`（其余种在实现对应路径时再加） |
| §15.3 `DevIORead/DevIOWrite/DevDestroy` | `kernel/src/rights.rs` 的 `Right::{DevIORead,DevIOWrite,DevDestroy}`（24..26 位）+ `ObjectKind::Device` |
| §15.5 `dev_ioport_create(base_port, port_count)` | `BootstrapState::dev_ioport_create` 铸根 cap，权限 `{Derive, DevIORead, DevIOWrite, DevDestroy}` |

**受控 I/O 路径**：`SYS_DEV_IOPORT_OUT/IN`（内核代执行 `outb`/`inb`）。授权在 `authorize_ioport`
里逐条校验：cap 可用 → 指向活动 `Device` → 调用者**持有** → 含所需 `DevIO` 权利 →
**端口落在该设备区间内**（`DeviceObject::covers`，按 u32 计算以避免 u16 回绕）。任一不满足即拒，
其中「端口越界」是受控路径的核心拒绝分支。

### 与规范 §15.5 的关系（明确登记）

§15.5 规定授权在**任务切换时通过 IOPL 或 TSS I/O bitmap** 实现，即「Task 自己执行 I/O 指令」。
本片先落**内核代执行**的受控路径，理由是它同样是「有效机制」且是 **per-port 精确**的（§15.5 的
架构警告指出 IOPL/bitmap 的粒度天然是 per-Task，需额外每端口位控制才精确）。硬件路径
（per-Task TSS I/O bitmap，使 ring3 `outb`/`inb` 本身被硬件仲裁）登记为后续片——它需要动 GDT/TSS
与任务切换，且要处理越权端口的 `#GP`，风险面独立。

## 判决性证据（本片）

- **内核单测**（6 条新增）：区间内端口授权通过；**区间外端口被拒**（`PortOutOfRange`）；
  `port_count == 0` 被拒（`EmptyRange`）；无 `DevIOWrite` 的只读派生 cap 写被拒
  （`MissingRight`）而读通过；非 Device cap（image Segment）被拒（`NotADevice`）；设备表容量饱和
  返回 `CapacityExceeded`。
- **权限类型化**：`rights::tests` 断言 `{Derive,DevIORead,DevIOWrite,DevDestroy}` 渲染序与
  `well_typed(ObjectKind::Device)`，且 `DevIORead` 不落在 `Segment` 上。
- **无回归**：QEMU 事件流**逐字节不变**（本片无 ring3 调用者，新权利不并入任何既有根 cap，
  `bootstrap_ready` 的三条 rights 串不变）。

## 改动

| 文件 | 内容 |
|---|---|
| `prima-abi/src/lib.rs` | `SYS_DEV_IOPORT_CREATE=33`、`SYS_DEV_IOPORT_OUT=34`、`SYS_DEV_IOPORT_IN=35` |
| `kernel/src/rights.rs` | `Right::{DevIORead,DevIOWrite,DevDestroy}`、`ALL_RIGHTS` 20→23、`ObjectKind::Device` 及 `allowed()` |
| `kernel/src/bootstrap.rs` | `ObjectRef::Device`、`kind/id/target_live` 三处加臂、`DeviceObject`/`DeviceTable`/`MAX_DEVICES`、`DEVICE_IOPORT_RIGHTS`、`DeviceError`、`dev_ioport_create`、`authorize_ioport`、`dev_ioport_{out,in}`、自由函数包装、6 条单测 |
| `kernel/src/syscall.rs` | 再导出三个新号；`serve()` 三个分支；`device_errno`、`dev_ioport_{create,out,in}_call` |
| `prima-runtime/src/{syscall,lib}.rs` + 新 `device.rs` | 号别名；`Runtime::{dev_ioport_create,dev_ioport_out,dev_ioport_in}` |

## 验证

- `cargo xtask test`：**6/6 PASS**（QEMU 事件流与 SPEC-112 逐字节相同）
- `cargo xtask lint`：**4/4 PASS**（含 `clippy -D warnings`、`deny(unsafe_code)`、机制层零 `unsafe`）
- `cargo test --workspace`：**795 passed, 0 failed**（kernel 369→**376**）
- `cargo xtask tcb`：`trusted=1262 mechanism_lines=21505 total=22767 ratio=5.54%`，机制层 `unsafe=0`

## 未决

1. **ring3 调用者尚未接入**：本片只落内核侧机制，尚无任务实际持有 `IOPortRange` cap 并读/写端口。
   下一片（SPEC-114）做「第一个用户态驱动任务」：由管理器 `dev_ioport_create` 铸 COM1 区间 cap、
   运行期移交驱动任务，驱动写/读串口 Scratch 寄存器（无输出副作用）作正例、越界端口作负例，
   内核侧见证。
2. **硬件 IOPL/TSS bitmap 路径**（§15.5）：使 ring3 `outb`/`inb` 由硬件按 per-Task bitmap 仲裁，
   而非内核代执行。需动 GDT/TSS 与任务切换、处理越权 `#GP`。
3. **`dev_destroy`（§15.8）未实现**：设备对象当前不可销毁（`DeviceTable::remove` 未落），
   MMIO/DMA/IRQ 三种 `DeviceKind` 亦未建模。
4. 本片**不勾选**任何阶段 4 交付项/退出条件；闭合判断交复核者。
