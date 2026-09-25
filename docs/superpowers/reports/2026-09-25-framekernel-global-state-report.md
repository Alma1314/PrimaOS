# 切片 AG 报告：内核全局状态 safe 化（SPEC-078）

> 文档类型：真实内核实现报告（framekernel 分层序列第 3 片）
>
> 日期：2026-09-25
>
> 设计：`docs/superpowers/specs/2026-09-25-framekernel-global-state-design.md`

## 1. 交付物

把内核的 `static mut` 收敛为 `prima_ostd::sync::StaticCell<T>`（闭包式安全读写 + `as_ptr()` 取地址）。逐文件：

| 文件 | 转换 |
|---|---|
| `sched.rs` | `FRAMES`/`ROOTS`/`KSTACK_TOPS`/`SCRATCH`/`KERNEL_RESUME_FRAMES`/`DEFERRED_FRAMES`/`DEFERRED_COUNT` |
| `frames.rs` | `REGISTRY`/`INSTALLED`；新增 `with_registry(f)` 辅助（未安装返回 `NotInstalled`），12 个访问器改闭包 |
| `gdt.rs` | `GDT_ITEM`/`TSS_ITEM` |
| `idt.rs` | `IDT_ITEM`（其余 4 处 asm 静态留 AH） |
| `cow.rs` | `COW_CTX` |
| `user.rs` | `USER_CTX`/`KERNEL_ROOT`/`KERNEL_IMAGE`/`KSTACK_TASK1`/`KSTACK_TASK2`/`KSTACK_TASKS`（栈缓冲用 `as_ptr()`） |
| `syscall.rs` | `MT`/`MT_SNAPSHOT`/`EXC_WAIT`/`IPC_PATHS`/`TASK_STATE`/`CALLS`/`FAST`（`CONTEXT_BASE` 留 AH） |
| `bootstrap.rs` | `BOOTSTRAP_STATE`（44 处访问）/`AUDIT_ARENA`/`FAULT_WITNESS`/`RESUME_WITNESS`/`RECLAIM_WITNESS`/`RESUME_FRAME_BUFFER`（`as_ptr()`） |
| `resume.rs`(bin) | `RESUME_STACK`（`as_ptr()`） |

框架侧为 AE 的 `StaticCell` 增补 `as_ptr()`（仅取地址、不解引用，供内核栈/恢复栈缓冲）。

## 2. 收敛进度

| 指标 | AF 后 | AG 后 |
|---|---|---|
| `static mut`（kernel src） | 38 | **5**（全部为 asm 直接触碰者，留 AH：`idt` 的 `USER_FRAME`/`FAULT_FRAME`/`SYSCALL_STACK`/`USER_RSP`、`syscall` 的 `CONTEXT_BASE`） |
| 模块级 `#[allow(unsafe_code)]` | lib 10 + bin 1 | **lib 6 + bin 1** |
| 新转零 unsafe 的模块 | — | `bootstrap`（0）、`cow`、`user`、`frames` |
| 仍保留 allow（留 AH） | — | lib：`exc`（裸 `resume`）、`gdt`（`lgdt`/`ltr`）、`idt`（裸入口）、`preempt`（裸 `timer_entry`）、`sched`（调用 `exc::resume` 这个 `unsafe fn`）、`syscall`（asm 静态 + `unsafe extern "C"` 处理函数）；bin：`resume`（裸 `resume_landing`） |

## 3. 回归面实测

方法同 AE/AF：与 AF 终点逐条 diff。**事件序列与 seq 不变（35 条）**，**5 条 data 变化**：

| 事件(seq) | AF | AG | 变化 |
|---|---|---|---|
| `memory_ready` (3) | `usable=30646;image=827;kend=0x133b000` | `usable=30595;image=878;kend=0x136e000` | `usable` −51、`image` +51、`kend` +51 帧 |
| `bootstrap_ready` (9) | `image_len=3383632` | `image_len=3593304` | **+209,672 B**（跨 51 帧）→ `image_frames` 827→878 |
| `preempt_ready` (25) | `rsp1=0x100000900d8`… | `rsp1=0x100000941e8`… | 启动栈地址后移 +0x4110 |
| `frames_ready` (29) | `free=30536;image=827` | `free=30485;image=878` | `free` −51、`image` +51；`universe=31473`/`segment=19`/`table=91` **不变** |
| `self_root_ready` (30) | `pages=173` | `pages=177` | 镜像页数 +4 |

其余 **30 条逐字段不变**。确定性：同镜像连跑两次事件流逐字节相同。

**根因**：`image_len` 上报的是整份 ELF 文件长度（debug 构建含 `.debug_*` 段）。闭包/单态化使 debug 段增大约 205 KB，直接体现为镜像帧数 +51。**无语义变化**，按维护规则 5 登记。

## 4. 实施中的失败与反例（保留）

1. **脚本化变换的括号 bug（真实失败，已修）**：本片有 44 处高度同形的 `unsafe { let slot = addr_of[mut]!(BOOTSTRAP_STATE); (*slot)... }`。我以受控脚本批量改写时，因实现缺陷（已在输出列表中的 `unsafe {` 行未删除）**留下 43 行孤立 `unsafe {`**，文件一度不可编译。定位后以「删除紧邻 `BOOTSTRAP_STATE.with*` 的孤立 `unsafe {` 行」修复。**教训登记**：批量文本变换对本项目（严格证据 + 大体量）风险高，事后必须编译 + 事件流双重校验；本次两轮校验均暴露问题（见下条）。
2. **闭包化对 `return`/`?` 的语义陷阱**：`kernel_exc_resume` 与 `record_reclaim_witness` 的块在 `None` 支用 `return Err(...)`。若直接包进闭包，`return` 只退出闭包、外层语句吞掉 `Err` → **本应失败却继续执行**。已改为闭包返回 `Result` + 外层 `?`（并把内层方法的 `?` 移出）。**该陷阱不改语义时不会编译报错，只有 QEMU 端到端能暴露**。
3. 三处块头为 `let x = unsafe {`（非纯 `unsafe {`）的站点脚本未匹配，手工改写。

## 5. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 305、prima-ostd 11、xtask 167 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 零告警 |
| `cargo fmt --all --check` | 无差异 |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（35 条事件序列） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |
| `cargo build -p kernel --target x86_64-unknown-none` | 通过 |

## 6. 口径偏差登记

1. `StaticCell` 的安全性依赖「单核顺序访问、不重入同一单元」文档契约；**未引入写一次类型**（`Once` 在框架中保留但本片未用于这些静态），boot-once 的写一次性仍由既有 `INSTALLED` 标志/调用次序保证，**非类型保证**。
2. safe-by-construction ≠ sound（同 AE/AF）。
3. 本片使 debug 镜像增大约 205 KB（闭包单态化 + debug 段），属工具链产物、非运行时膨胀；已按维护规则 5 登记。

## 7. 遗留与后续

- **AH（SPEC-079）**：5 处 asm 静态 + 裸入口 + IDT/GDT 下沉、回调反转；届时可移除 `exc`/`gdt`/`idt`/`preempt`/`sched`/`syscall`/bin `resume` 的 allow。
- **AI（SPEC-080）**：删全部 `#[allow]`、lint/零-unsafe 门禁、TCB 量化指标（G3）。
- 本片不勾选任何阶段状态；闭合判断交复核者。
