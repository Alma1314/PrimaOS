# 切片 AF 报告：物理内存与页表 POD 收敛（SPEC-077）

> 文档类型：真实内核实现报告（framekernel 分层序列第 2 片）
>
> 日期：2026-09-25
>
> 设计：`docs/superpowers/specs/2026-09-25-framekernel-mm-pod-design.md`

## 1. 交付物

| 工件 | 内容 |
|---|---|
| `prima-ostd/src/mm.rs`（新） | 穿透映射内存的 POD 访问原语：`read_u64`/`write_u64`/`read_u8`/`write_u8`、按索引 `read_u64_at`/`write_u64_at`/`read_u8_at`/`write_u8_at`、`fill_zero`、`bytes_of`、`bytes_at`/`bytes_at_mut`。全部安全 `fn` + `#[inline]` |
| `kernel/src/safety.rs` | **删除**（唯一原语 `read_phys_byte` 迁入框架 `mm`） |
| 迁移 26 处裸访问 | `mem`、`vspace`、`cpu`、`cow`、`user`、`syscall`、`preempt`、`gdt`、`bootstrap`、`idt`(测试) |
| `kernel/src/lib.rs` | 移除 `mem`/`cpu`/`vspace`/`safety` 的模块级 `#[allow(unsafe_code)]`（4 个） |

框架累计 518 行（`arch`/`sync`/`mm`/`lib`）。

## 2. 收敛进度

| 指标 | AE 后 | AF 后 |
|---|---|---|
| 内核模块级 `#[allow(unsafe_code)]`（lib） | 14 | **10** |
| 非测试代码零 unsafe 模块 | `msr`、bin `serial`/`qemu` | 追加 `mem`、`cpu`、`vspace`（`safety` 删除） |
| 内核裸内存访问站点（volatile/from_raw_parts） | 26 | **0** |

`cpu`/`vspace` 的 host 单测用 `std::alloc` 造页表页，故 `#[allow(unsafe_code)]` 下沉到**测试模块**（非整个模块），注释标明"测试基建、非内核代码"。

## 3. 回归面实测

方法同 AE：以 HEAD 独立 worktree 基线，先取 AE 终点（`/tmp` 抓取），再对 AF 逐条 diff。**事件序列与 seq 不变（35 条）**，AF 相对 AE 终点 **4 条 data 变化**：

| 事件(seq) | AE 终点 | AF | 变化 |
|---|---|---|---|
| `memory_ready` (3) | `usable=30648;image=826;kend=0x133a000` | `usable=30646;image=827;kend=0x133b000` | `usable` −2、`image` +1、`kend` +1 帧 |
| `bootstrap_ready` (9) | `image_len=3382336` | `image_len=3383632` | `image_len` **+1296 B**，跨 4 KiB 边界 → `image_frames` 826→827、`free_frames` −2 |
| `preempt_ready` (25) | `rsp1=0x10000090688`… | `rsp1=0x100000900d8`… | 启动栈地址移动 −0x5b0 |
| `frames_ready` (29) | `universe=31474;free=30538;image=826` | `universe=31473;free=30536;image=827` | `universe` −1、`free` −2、`image` +1；`segment=19`/`table=91` **不变** |

其余 **31 条逐字段不变**。同 AE：`usable`/`universe`/`free`/`image_len` 只做守恒/非零断言，`xtask test` 不会拦住这些变化，靠手工事件流 diff 发现并登记。

## 4. 实施中暴露的缺陷（反例保留）

**指针步长错误**：`preempt.rs` 原代码 `core::ptr::read_volatile(frame.add(16))` 中 `frame: *const u64`，`add(16)` 前进 **128 字节**；迁移时我写成 `prima_ostd::mm::read_u64(frame as u64 + 16)`，只前进 16 字节 → 读错字段，内核在 `task_ready` 之后三重故障，QEMU 退出码 0、未到 `boot_complete`。修正为 `read_u64_at(frame as u64, 16)` 后恢复。

**教训（登记）**：框架按索引访问的 API（`*_at`）与裸指针 `add(n)`（按**元素**前进）语义不同；这类步长错误**host 单测抓不到**（`read_u64_at` 本身的单测通过），只有 QEMU 端到端能暴露。后续迁移同一模式（如 AH 的裸入口）时必须逐点核对字节偏移。

## 5. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 305、**prima-ostd 10**、xtask 167 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 零告警 |
| `cargo fmt --all --check` | 无差异 |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（35 条事件序列） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |
| `cargo build -p kernel --target x86_64-unknown-none` | 通过 |

## 6. 口径偏差登记

1. safe-by-construction ≠ sound：`mm` 的 VA 访问器信任调用方保证映射有效；误用产生 UB，不宣称已消除。
2. 不引入 `PhysMap{base,len}` 类型（相对设计草图的偏差）：以 VA 访问器承载，理由见设计 §3。
3. `bytes_at`/`bytes_at_mut` 返回 `&'static` 借出静态缓冲；调用方遵守单核顺序访问、不得制造重叠可变别名。

## 7. 遗留与后续

- **AG（SPEC-078）**：38 处 `static mut` → `StaticCell`/`Once`（覆盖 `bootstrap`/`cow`/`frames`/`user`/`sched`/`syscall`/`gdt`/`idt` 等）。
- **AH（SPEC-079）**：裸入口 + IDT/GDT 下沉、回调反转。
- **AI（SPEC-080）**：删全部 `#[allow]`、lint/零-unsafe 门禁、TCB 量化指标（G3）。
- 本片不勾选任何阶段状态；闭合判断交复核者。
