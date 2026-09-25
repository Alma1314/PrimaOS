# 切片 AI 设计：收口与 TCB 量化（SPEC-080）

> 文档类型：真实内核实现设计（framekernel 分层序列第 5 片 / 收口片）
>
> 日期：2026-09-25
>
> 状态：已获用户确认（2026-09-25，方案批准：独立底层 crate / 完整序列 / **纳入 G3**）
>
> 前置：AE（SPEC-076）建 crate 边界；AF（SPEC-077）收敛内存访问；AG（SPEC-078）收敛全局状态；AH（SPEC-079）入口反转。

## 1. 目标与范围

1. **内核 crate 内零 `unsafe`**（含 `#[cfg(test)]` 代码）——处置 AH 遗留的两处测试基建 allow。
2. **可复跑的门禁**：新增 `cargo xtask lint`，把 framekernel 边界变成机器可判定的检查。
3. **G3：TCB 量化指标**：新增 `cargo xtask tcb`，给出可信代码行占比并入库基线。

## 2. 处置测试 allow：换掉 `std::alloc` 裸 API

AH 后仅剩 `cpu.rs:190`、`vspace.rs:502` 两处 `#[allow(unsafe_code)]`，内容都是 host 单测用
`std::alloc::{Layout, alloc_zeroed, dealloc}` 手工造 4 KiB 对齐的页表页。

**新方案（零 unsafe）**：页表页用 `Box::new(Page::zeroed())` 分配——`Page` 自身
`#[repr(C, align(4096))]`，`Box` 的分配器按类型的 `Layout` 对齐（含 4096），故**无需**裸 API 即得
页对齐、全零、**地址稳定**（`Box` 的堆块不随容器搬移，页表项里存的帧地址被 `translate` 掩码后
仍指向同一页）的页。池化为 `kernel/src/test_support.rs`（`#[cfg(test)]`，不进内核镜像）：

```rust
#[repr(C, align(4096))] pub struct Page([u64; 512]);   // 4096 B，对齐 4096
pub struct PagePool { pages: Vec<Box<Page>> }          // Drop 自动回收，无手工 free
```

读写一律走 `prima_ostd::mm::read_u64_at`/`write_u64_at` 安全访问器。`cpu.rs` 的四处
`alloc_page()/free_page()` 与 `vspace.rs` 内的 `PagePool` 定义一并删除，改用共享池。

## 3. `cargo xtask lint`：四道门禁

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo build -p kernel --target x86_64-unknown-none`——**编译期**证明内核（lib + bin）在
   `#![deny(unsafe_code)]` 下可编
4. **源码级**证明：扫描 `kernel/src` 全部 `.rs`，断言 0 个 `unsafe` 标识符、0 个 `#[allow(unsafe_code)]`

扫描器口径（可证伪、可单测）：先剥离 `//` / `/* */`（**嵌套**计数）注释与字符串/字符字面量内容
（换行保留，故行号不变），再按**标识符边界**匹配 `unsafe`——故 `#![deny(unsafe_code)]` 与
`#![deny(unsafe_op_in_unsafe_fn)]` 这类 lint 名**不**误报（这两条最初确实误报过，见报告 §4）。

## 4. `cargo xtask tcb`：G3 的可证伪度量

| 量 | 定义 |
|---|---|
| 可信面（TCB） | `prima-ostd/src` —— 唯一被允许写 `unsafe` 的 crate |
| 机制层（对照） | `kernel/src` —— 零 `unsafe` |
| 指标 | **可信代码行占比** = TCB 代码行 /（TCB + 机制层）代码行；另报两侧 `unsafe` 标识符数 |

代码行 = 剥离注释/字符串后的非空行。口径边界：`cosir`/`cosir_state` 是可执行模型、`xtask` 是
构建工具，都不进内核镜像，故不计入；指标只覆盖「OS 实现代码」两 crate。

`tcb` 同时把「机制层 `unsafe` 数必须为 0」作为失败条件（度量与门禁取同一定义）。

## 5. 回归面

本片**不触碰内核非测试代码**（`test_support` 为 `#[cfg(test)]`），故预期 **QEMU 事件流逐字节不变**
（与 AH 终点 diff 为空）。host 单测计数只增不减：kernel 305→306（新增页池自测）、
xtask 167→**176**（扫描器/剥离器/占比 9 条）。

## 6. 验证

`cargo test --workspace`、`cargo xtask lint`、`cargo xtask tcb`、`cargo xtask test`、
`cargo xtask compat`；事件流与 AH 终点逐字节 diff。
