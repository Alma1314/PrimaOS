# 切片 AG 设计：内核全局状态 safe 化（SPEC-078）

> 文档类型：真实内核实现设计（framekernel 分层序列第 3 片）
>
> 日期：2026-09-25
>
> 状态：已获用户确认（2026-09-25，方案批准：独立底层 crate / 完整序列 / 纳入 G3）
>
> 前置：AE（SPEC-076）建 `prima-ostd::arch`/`sync`；AF（SPEC-077）建 `mm` 并把裸内存访问归零。

## 1. 目标与范围

把内核的 **38 处 `static mut`** 收敛为框架 `prima-ostd::sync` 的安全封装（`StaticCell<T>` / `Once<T>`），使访问不再需要 `unsafe`。

**不属于本片**（留 AH）：**被 asm 直接触碰**的静态必须与 asm 同 crate，故以下 7 处留在原处，AH 随裸入口一起下沉：
`idt.rs` 的 `FAULT_FRAME`/`FAULT_ADDR`/`FAULT_ERROR`/`USER_FRAME`/`SYSCALL_STACK`/`USER_RSP`、`syscall.rs` 的 `CONTEXT_BASE`。

## 2. API

沿用 AE 的 `StaticCell<T>`：
- `const fn new(v)`、`with(|&T| R)`、`with_mut(|&mut T| R)`、`as_ptr() -> *mut T`（仅取地址的栈缓冲用）。

**口径**：`StaticCell` 的安全性依赖"单核顺序访问"契约（无并发、不重入同一单元）。本片**不引入写一次类型**（`Once` 保留但本片未用于这些静态）；boot-once 的写一次性仍由既有 `INSTALLED` 标志/调用次序保证，不由类型保证——**该偏差登记**。

## 3. 逐文件改造

| 文件 | 静态 | 形态 |
|---|---|---|
| `sched.rs` | `FRAMES`/`ROOTS`/`KSTACK_TOPS`/`SCRATCH`/`KERNEL_RESUME_FRAMES`/`DEFERRED_FRAMES`/`DEFERRED_COUNT` | `StaticCell`，访问改闭包 |
| `frames.rs` | `REGISTRY`/`INSTALLED` | `StaticCell`；新增 `with_registry(f)` 辅助返回 `Result<_, FrameError>`，10 个公开访问器改闭包 |
| `gdt.rs` | `GDT_ITEM`/`TSS_ITEM` | `StaticCell` |
| `idt.rs` | `IDT_ITEM` | `StaticCell`（其余 6 处留 AH） |
| `cow.rs` | `COW_CTX` | `StaticCell<Option<Ctx>>` |
| `user.rs` | `USER_CTX`/`KERNEL_ROOT`/`KERNEL_IMAGE`/`KSTACK_TASKS` | `StaticCell`；`KSTACK_TASKS` 用 `as_ptr()` |
| `syscall.rs` | `MT`/`MT_SNAPSHOT`/`EXC_WAIT`/`IPC_PATHS`/`TASK_STATE`/`CALLS`/`FAST` | `StaticCell`（`CONTEXT_BASE` 留 AH） |
| `bootstrap.rs` | `BOOTSTRAP_STATE`/`AUDIT_ARENA`/`FAULT_WITNESS`/`RESUME_WITNESS`/`RECLAIM_WITNESS`/`RESUME_FRAME_BUFFER` | `StaticCell` |
| `resume.rs`(bin) | `RESUME_STACK` | `StaticCell` + `as_ptr()` |

## 4. 收敛目标

AG 后**可移除模块级 `#[allow(unsafe_code)]`** 的模块（其非测试 unsafe 归零）：`bootstrap`、`cow`、`frames`、`user`。**保留 allow**：`exc`（裸 `resume`）、`gdt`（asm `lgdt`/`ltr`）、`idt`（裸入口 + asm 静态）、`preempt`（裸 `timer_entry`）、`sched`（调用 `exc::resume` 这个 `unsafe fn`）、`syscall`（asm 静态 `CONTEXT_BASE` + `unsafe extern "C"` 处理函数）——由 AH 处置。
模块级 allow 由 10 降至 **6**（lib）/ 总计 11→7。

**实施结果（2026-09-25）**：实际与上表一致。`static mut` 38→**5**（余 `idt` 的 `USER_FRAME`/`FAULT_FRAME`/`SYSCALL_STACK`/`USER_RSP` 与 `syscall` 的 `CONTEXT_BASE`，均 asm 直接触碰，留 AH）；lib allow 10→6、bin 1 不变（`resume` 仍含裸 `resume_landing`）。含两处**失败反例**（脚本化变换的括号 bug、闭包化对 `return`/`?` 的语义陷阱），详见报告 §4。

## 5. 回归面

预期**事件序列与 seq 不变**；仅可能因镜像布局变化扰动 `bootstrap_ready.image_len` 与 `preempt_ready` 栈地址（按维护规则 5 登记）。以 AE/AF 同样的方法（HEAD worktree + 上一片终点抓取）逐条 diff。host 单测计数不得下滑。

**风险提示（来自 AF 反例）**：闭包式改写改变借用结构，务必逐点核对（尤其是 `SCRATCH` 传给 `exc::resume` 的引用、`frames::global()` 的指针返回改闭包）；此类语义错误只有 QEMU 端到端能暴露。

## 6. 验证

`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`、`cargo xtask test`、`cargo xtask compat`、`cargo build -p kernel --target x86_64-unknown-none`；事件流逐条 diff。
