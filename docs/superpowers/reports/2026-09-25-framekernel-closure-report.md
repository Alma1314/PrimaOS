# 切片 AI 报告：收口与 TCB 量化（SPEC-080）

> 文档类型：真实内核实现报告（framekernel 分层序列第 5 片 / 收口片）
>
> 日期：2026-09-25
>
> 设计：`docs/superpowers/specs/2026-09-25-framekernel-closure-design.md`

## 1. 交付物

| 交付 | 内容 |
|---|---|
| 内核零 `unsafe`（含测试） | 新增 `kernel/src/test_support.rs`（`#[cfg(test)]`）：`Page`（`#[repr(C, align(4096))]`，4096 B）+ `PagePool`（`Vec<Box<Page>>`，`Drop` 自动回收）；`cpu.rs` 删 `alloc_page`/`free_page` + `std::alloc` 裸 API，`vspace.rs` 删内联 `PagePool`/`Drop`，两处测试模块的 `#[allow(unsafe_code)]` 删除，页读写全走 `prima_ostd::mm` 安全访问器 |
| `cargo xtask lint` | 四道门禁：`fmt --all --check`、`clippy --workspace --all-targets -- -D warnings`、`build -p kernel --target x86_64-unknown-none`、**源码断言 `kernel/src` 零 `unsafe` 标识符与零 `#[allow(unsafe_code)]`** |
| `cargo xtask tcb` | G3 度量：可信面/机制层代码行与占比、两侧 `unsafe` 标识符数；机制层非零即失败 |
| 扫描器 | `xtask/src/lint.rs`：剥离 `//`、`/* */`（嵌套）、字符串与字符字面量（含 `'"'` 与 `'\''`），保留换行；按**标识符边界**匹配 `unsafe` |

## 2. 最终收敛状态（framekernel 五片合计）

| 指标 | AE 前 | AE | AF | AG | AH | **AI** |
|---|---|---|---|---|---|---|
| `static mut`（kernel） | 38 | 38 | 38 | 5 | 0 | **0** |
| 模块级 `#[allow(unsafe_code)]` | — | lib 14 + bin 1 | lib 10 + bin 1 | lib 6 + bin 1 | lib 0 + bin 0 | **0（含测试代码）** |
| 内核 `unsafe` 标识符（含测试） | 250+ | 多数 | 少 | 少 | 仅 2 处测试 allow | **0** |
| 内核 `#![feature(abi_x86_interrupt)]` | 有 | 有 | 有 | 有 | **删除** | 删除 |
| `prima-ostd` host 单测 | — | 5 | 10 | 11 | 21 | **21** |

**`cargo xtask lint` 实测**：`kernel/src` 25 个文件扫描、0 命中；`cargo xtask tcb` 实测
**机制层 `unsafe` 标识符 = 0**。

## 3. G3：TCB 量化基线（可证伪度量）

`cargo xtask tcb` 输出（2026-09-25，本片终点）：

```
TCB trusted_lines=1217 mechanism_lines=15339 total_lines=16556 ratio=7.35%
TCB unsafe_tokens trusted=53 mechanism=0
```

| 量 | 值 |
|---|---|
| 可信面（`prima-ostd/src`）代码行 | **1217** |
| 机制层（`kernel/src`）代码行 | **15339** |
| 占比 | **7.35 %** |
| 可信面 `unsafe` 标识符 / 机制层 | **53 / 0** |

**口径边界（登记）**：`cosir`/`cosir_state` 是可执行模型、`xtask` 是构建工具，都不进内核镜像，
故不计入分母；指标只覆盖「OS 实现代码」两 crate。代码行 = 剥离注释与字符串后的非空行，
**不是** LCS/语义等价意义上的可信计算基（G3 原文即允许「以简单行数起步」）。该数是**增长基线**：
后续切片若把机制搬进框架、或框架内新增 unsafe 原语，占比应上升且必须在 tracker 复测登记。

## 4. 实施中的失败与反例

1. **子串匹配把 lint 名当成了 `unsafe` 关键字（真实失败，已修）**：扫描器首版用
   `line.contains("unsafe")`，于是 `#![deny(unsafe_code)]`、`#![deny(unsafe_op_in_unsafe_fn)]`
   全部命中——门禁会**永远失败**，且 `scan_source` 的单测立刻抓到。改为按标识符边界匹配
   （前后字符均非 `[A-Za-z0-9_]`）后通过。**教训**：门禁自身的判据必须先被证伪（本次由单测提供）。
2. **字符串/字符字面量导致的假阴性风险**：若只剥注释，`let s = "unsafe";` 会误报；反之若不做
   跳过，`'"'`（字符字面量里的引号）会被当成字符串起点而**吞掉后续整段源码**，形成假阴性。
   两者都加了处理与专门单测（`strip_comments_blanks_string_literal_contents`、
   `strip_comments_skips_char_literal_containing_a_quote`）。该风险**不报编译错**，只有门禁
   单测能暴露。
3. **`'\''` 的转义边界**：转义分支原先在 `\` 之后立即找闭引号，`'\''` 会提前一个字符收尾；
   改为跳过转义字符本身后再找闭引号。

## 5. 回归面实测

**事件流与 AH 终点逐字节 diff 为空**——本片不触碰内核非测试代码（`test_support` 为
`#[cfg(test)]`，不进镜像），故 35 条事件的序列、seq 与全部 data **零变化**（含 AH 登记过的
`image_len`/`frames_ready`/`preempt_ready` 等字段）。这是本序列中唯一一片"零改动面"。

host 单测计数只增不减：kernel 305→**306**（新增页池自测：页对齐/全零/页间独立）、
xtask 167→**176**（扫描器 8 条 + 仓库实测 1 条）、prima-ostd 21、cosir 145、cosir_state 40、
lean_backtrace 2。

## 6. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **306**、prima-ostd 21、xtask **176** |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy / 裸机构建 / 内核零 unsafe） |
| `cargo xtask tcb` | `ratio=7.35%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（35 条事件序列） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |
| 事件流 diff（vs AH） | **逐字节相同** |

## 7. 口径偏差登记

1. **门禁是源码级词法检查，不是语义证明**：它不展开宏（`asm!` 之外的宏若生成 `unsafe` 不会被发现）、
   不覆盖 `build.rs`/proc-macro 产物、不解析 raw string 的全部形态。真正的保证仍来自
   `#![deny(unsafe_code)]` 的编译期强制（门禁第 3 条即为此）；第 4 条是**防回归的护栏**。
2. **TCB 行数是代理指标**（同 §3）：不含 `cosir` 模型与 `xtask` 工具，非语义等价意义上的可信基。
3. safe-by-construction ≠ sound（同 AE–AH）：框架仍以"信任调用方"为契约，本片未改变该口径。

## 8. 遗留与后续

- 五片序列（AE–AI）完成：`prima-ostd` 为唯一信任面，`kernel` 零 `unsafe` 且由门禁守住。
- 仍**未**处置的差距项：G4（Miri 类可执行 UB 检查）、G5（调度/分配策略注入边界）——按差距报告
  属"需实验判决 / 后续工作"，不在本序列范围。
- 本片不勾选任何阶段状态；闭合判断交复核者。
