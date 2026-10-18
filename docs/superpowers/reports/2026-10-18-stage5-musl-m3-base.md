# 阶段 5 · musl M3：SSE/FPU 契约与静态 musl 基座

**日期**：2026-10-18

**性质**：M3 完整切片（含 M3b 续片）。启用 x87/SSE 与 per-task `FXSAVE/FXRSTOR`，从
锁定快照构建 Prima 的静态 `libc.a` 与 sysroot，用 Prima crt 接 `__libc_start_main`，
把 C 探针改为链接 musl，并以用户态匿名 arena 支撑 `malloc/free/realloc`。**未移植
线程、信号、完整 auxv/任意 argv、stdio 输出路径、取消点 musl 侧替换与动态链接。**
S5-D*/S5-X* 仍未勾选，profile v0 未修改。

**上位计划**：[musl 移植计划](../plans/2026-10-18-musl-port-plan.md)；
**前序**：[M0 盘点](2026-10-18-stage5-musl-m0-inventory.md)、
[M2 边界](2026-10-18-stage5-musl-m2-boundary.md)。

## 1. 内核：SSE/FPU 契约

musl 的 x86_64 C 闭包需要 SSE（`vfprintf` 的 `movaps` 等），而 M2 的
`-mgeneral-regs-only` 只能约束探针/适配层 TU，不能约束预编译的 `libc.a`。本片裁决为
启用 SSE，而不是继续限制通用寄存器。

- `prima-ostd::arch` 新增 `read_cr4`/`write_cr4`/`write_cr0`/`fxsave`/`fxrstor`。
- `kernel::cpu::enable_sse`：清 `CR0.EM/TS`，置 `CR4.OSFXSR/OSXMMEXCPT`，在进入
  CPL3 前调用一次；纯掩码函数可 host 单测。
- `kernel::sched`：每 Task 512 B、16 字节对齐的 FXSAVE 区域；四个切换入口
  （`switch_to`/`block_to`/`exit_to`/`fault_to`）在 `set_current` 之前保存当前任务，
  `resume` 在装 FS 之后恢复目标任务。默认 x87 控制字 `0x037F`、MXCSR `0x1F80`；
  `clear_task` 复位槽位，id 复用不继承旧状态。
- 内核自身仍以 `x86_64-unknown-none` 的软浮点配置编译，不产生 SSE；用户态状态不会
  被内核污染。

**变异验证**：把 `restore_fpu` 临时改为总是恢复默认状态，QEMU 在 `TTYOUT` 后
panic，双轮验收失败；恢复实现后通过。证明 ring3 的 XMM 值确实依赖 per-task 恢复。

## 2. musl 静态基座

### 2.1 快照与补丁

- 快照仍是 M0 锁定的 musl 1.2.5（commit `0784374d…`），构建只读使用
  `target/musl-1.2.5`。
- `prima-user/musl/build.sh`：复制快照到 `target/musl-prima`，覆盖两个移植文件，
  以 `-fPIC -O2` 配置静态库，安装到 `target/musl-prefix`。缓存戳由快照 commit 与
  补丁内容哈希组成，重复构建直接复用。
- 补丁（以完整替换文件保存，便于重放）：
  - `arch/x86_64/syscall_arch.h`：删除直接 `syscall` 的内联汇编，保留 `__syscall0..6`
    名字并声明为外部符号。musl 的全部调用点因此落到 Prima 适配层；Linux 号不进入内核。
  - `src/thread/x86_64/__set_thread_area.s`：`ARCH_SET_FS` 改为调用适配层
    `__syscall2(158, ARCH_SET_FS, addr)`，由适配层翻译成 `SYS_TASK_SET_FS_BASE`。

### 2.2 Prima crt

新增 `prima-user/c/prima_crt.c` 与 `prima_crt_start.S`：

- `_start` 保存入口五寄存器到 `.bss` 的 `prima_ctx`，把内核初始栈指针交给
  `prima_start_c`；
- `prima_start_c` 直接调用 `__libc_start_main(main, argc, argv, _init, _fini, 0)`。

**刻意不使用 musl 的 `rcrt1.o`**：内核 ELF 装载器已经消费全部 `R_X86_64_RELATIVE`，
`rcrt1` 的 `_dlstart_c` 会再次应用并破坏 GOT/数据段。

### 2.3 适配层边界收紧

`prima-user/c/prima_syscall.c` 不再定义 `write/read/writev/close/ioctl/_exit/exit`。
这些符号与 musl 同名；若由适配层提供，链接器会让 musl 的 `exit`（含 stdio 清理）
被替换成薄包装，掩盖真实退出语义。现在它们由 `libc.a` 提供，并最终经
`__syscallN` 回到适配层。`arch_prctl`/`set_tid_address` 不是 musl 的公开函数符号，
继续由适配层提供。

### 2.4 容量调整

musl 探针的代码段 10 页、可写 `PT_LOAD` 2 个、入口栈 8 页，使创建期逻辑映射条目
越过原 144 上限。`MAX_VSPACE_ENTRIES` 144 → 192；C 入口栈 1 → 8 页
（`-` 探针仍是无 libc 的 4 页栈时代；musl 启动 + `snprintf` 需要更多）。
两者都是真实增长，未放宽任何不变量检查；容量水位随本片重测。

## 3. 探针

`stage5_probe.c` 继续作为两端同源的 `main`，Prima 侧链接 musl：

- 既有 h/i/xyz 字节流与退出码 37 逐字节不变；`CIO|task=17`、`TTYOUT`、
  `EVENT_STAGE5_CAPACITY` 的 `[17, 0x5a, 37, 1, 1]` 与 IPC 计数均未变。
- 新增（仅 Prima，`musl_runtime_probe`，不产生串口输出）：
  - `snprintf("%d-%s-%x", 42, "musl", 0xabc)` 输出 `42-musl-abc`；
  - `getenv("PRIMA_ABI") == "1"`；
  - musl 的 `write(99, …)` 返回 `-1` 且 `errno == EBADF`（原始负 errno 经
    musl 的 `__syscall_ret` 转成 errno）。
- SSE 往返断言复用 h/i 两次写触发的协作切换：写走适配层 `__syscallN`，不经过
  libc，故 XMM 不会被 libc 自身使用干扰。
- **FS 实验（`arch_prctl(SET_FS)` + `%fs:0`）在 musl 构建中跳过**：它会覆盖 musl
  的 TLS 基址，后续 musl 代码（errno/stdio）会访问错误 TLS。M2 的无 libc 构建仍保留
  该实验；M3 的 TLS 证据来自 musl 自身启动（`__set_thread_area` 成功即不崩溃）。

## 4. 证据

```
cargo test --workspace
  cosir 146, cosir-state 42, lean-backtrace 3, kernel 402,
  prima-initgen 2, prima-ostd 21, prima-runtime 1, xtask 200 all PASS

cargo xtask lint
  PASS fmt --all --check
  PASS clippy -D warnings
  PASS kernel builds under deny(unsafe_code) for x86_64-unknown-none
  PASS kernel kernel/src has zero `unsafe` (27 files scanned)

cargo xtask test
  PASS stage5 C probe is static PIE with R_X86_64_RELATIVE only
  PASS build kernel
  PASS image bios
  PASS image uefi
  PASS linux stage5 C main exits 37/2/3 and streams h/i plus 5-byte xyz via write(1/2)
  PASS qemu bios run=1
  PASS qemu bios run=2
  PASS stage1 boot slice
```

`xtask` 在构建 C 探针前运行 musl 构建脚本；首次构建约一分钟，之后按哈希缓存。
探针 ELF 仍由构建期校验为 `ET_DYN`、无 `PT_INTERP`、仅 `R_X86_64_RELATIVE`。

**诊断过程中的变异与边界**（用于避免把巧合当证据）：

- 显式 `SYS_YIELD` 放在字节流之前会改变启动时序，使 driver 的隔离故障在无 Ready
  后继时触发 panic；SSE 断言最终复用 h/i 的写让出，而不是额外插入裸 yield。
- 退出路径发送退出消息后**不**插入 yield：自毁前让出会破坏 M2 已见证的消息先取走
  顺序；恢复原文后通过。
- 入口栈从 4 页增至 8 页后，创建期 vspace 条目超限，必须同时扩容到 192；只加栈或
  只加 vspace 都会失败。

## 4.1 M3b：mallocng 的匿名内存（2026-10-18 续片）

musl 的 `malloc` 不直接调用 `mmap`，而是由 mallocng 先 `brk` 取元数据区、再对
分组调用匿名 `mmap`，`free` 对独立映射调用 `munmap`，`realloc` 优先尝试 `mremap`。
因此 T-MUSL-05 需要的是完整的这几个系统边界，而不是一个 `malloc` 符号。

### 4.1.1 实现

- **服务管理器预映射**：创建 C 任务时，栈段从 8 页扩为 **72 页**——前 8 页仍是入口栈，
  后 64 页作为固定 VA `C_HEAP_VA = 0x0000_0C00_0000_0000` 的匿名 arena，以 `RW`
  映射进子任务根。它与栈**同一次 `memseg_create`**，不增加 Segment 槽位或 audit
  事件；C 探针运行期的容量快照是 segment 41/48、audit 757/768。
- **适配层**（`prima_syscall.c`）：
  - `brk`：arena 首 16 页；`brk(0)` 返回当前断点，`brk(new)` 只在区间内前进，
    超出时按 Linux 约定返回当前断点，让 mallocng 永久转用 `mmap`；
  - 匿名 `mmap`：只接受 `MAP_PRIVATE|MAP_ANONYMOUS`、`fd=-1`、`offset=0`，从arena
    的 mmap 区 bump 分配；单次上限 48 页；`MAP_FIXED` 只为 brk guard 页返回原地址；
  - `mprotect` 返回成功但不改变权限（匿名页一律 `RW`）；
  - `munmap` 是空操作（bump 区不回收），`mremap` 保持 `ENOSYS`，`realloc` 走
    malloc + copy + free 回退。
- **内核容量**：
  - `MAX_VSPACE_ENTRIES` 192 → **256**：`memseg_map` 逐页记逻辑条目，arena 一次
    贡献 64 条；
  - `MAX_FRAME_OWNERS` 192 → **256**：72 页栈+arena 合计需要 72 个逐帧归属项，旧
    上限在 `allocate_for_segment` 处以 `NoMemory` 失败；
  - 归属表从 `FrameRegistry` **移入 `OWNER_ARENA`**：256 项内联会让注册表按值大小
    超过既有的 32 KiB 预算；与 vspace/audit 使用同一 arena 模式，结构体只留区段句柄。
    该重构不改变 W6 的逐帧守恒/不交判定，也不能通过放宽预算测试来替代。

### 4.1.2 探针断言

`musl_heap_probe` 在既有 `snprintf`/`getenv`/errno 之后运行，不产生串口输出：

- `malloc(64)`：非空、16 字节对齐、写入/读回 64 字节后 `free`；
- `malloc(4096)`：首页与末页可写；
- `malloc(140000)`：超过 `MMAP_THRESHOLD = 131052`，走独立匿名 `mmap`；写入首末
  字节后 `free`（触发 `munmap`）；
- `malloc(32)` + `realloc(128)`：覆盖 `mremap` 返回 `ENOSYS` 后的复制回退。

任一失败会以不同退出码上报（31 = 首个 malloc，32–39 = 后续阶段），而不是笼统的
退出 10。

### 4.1.3 证据

```
cargo test --workspace   15 组 test result: ok（kernel 402 通过）
cargo xtask lint         fmt / clippy -D warnings / 裸机 deny(unsafe_code) / 零 unsafe 全 PASS
cargo xtask test         8 项全 PASS
```

- `EVENT_STAGE5_CAPACITY` 仍为 `[17, 0x5a, 37, 1, 1]`：C 探针在 malloc 探针之后
  以退出码 37 结束；
- 串口既有见证逐字节未变：`CIO|task=17;fd=1;ch=104;fd=2;ch=105`、`TTYOUT`、
  h/i 与 5 字节 `xyz`；
- `CAPACITY` 行仍为 `segment_slots=41;segment_limit=48;audit_used=757;audit_limit=768`；
- `state_audit_ready` 仍为 `used=757;cap=768`；
- `frames_ready` 的 `image` 随内核重构变化（1158），free 相应变化；这是真实帧数，
  事件结构与不变量检查未变。

### 4.1.4 已知边界（不宣称通用 mmap）

- arena 每个 C 任务 64 页（16 页 brk + 48 页 mmap），由服务管理器在任务创建时
  一次映射；只有 `c_stack` 路径的 C 任务具备该映射；
- 无逐映射回收：`munmap` 空操作；arena 用尽后 `mmap` 返回 `ENOMEM`；
- 无权限变更：`mprotect` 不改变 PTE 权限；`MAP_FIXED` 不重映射；
- 无文件映射、共享映射、`mremap`、超过 48 页的单次匿名映射；
- 该实现是 mallocng 的**用户态适配**，不是对任意 POSIX 程序的 `mmap` 承诺。

## 5. 测试 ID 状态

| ID | 状态 | 证据 |
|---|---|---|
| T-MUSL-00 | **通过（本片重放）** | 快照 commit 与哈希未变；补丁按内容哈希重放 |
| T-MUSL-01 | **部分通过** | 适配层调用集经 musl 内部 `__syscallN` 使用；未知号/非法 fd/原始负 errno 同源断言仍在 |
| T-MUSL-02 | **通过** | FS 由 `__set_thread_area` 驱动；per-task FPU 契约新增 host 单测 + QEMU 变异 RED |
| T-MUSL-03 | **通过** | 探针为静态 PIE、仅 RELATIVE；Prima crt 接 `__libc_start_main`；musl `libc.a` 链接 |
| T-MUSL-04 | **部分通过** | `snprintf` 格式化、`getenv`、errno 语义；stdio 输出路径（`printf`→`writev`）未直接断言 |
| T-MUSL-05 | **通过（M3b）** | `malloc/free/realloc` 经匿名 arena 完成；含 140000 字节独立 mmap 与 `munmap` 路径 |
| T-MUSL-06–09 | 未执行 | 依赖线程/信号/fork/网络或动态链接 |

## 6. 已知边界（不得据此宣称支持）

- **匿名内存是用户态 arena**：64 页、无回收、无权限变更、无文件/共享映射，
  单次上限 48 页；见 §4.1.4。`malloc/free` 通过不等于通用 `mmap` 支持。
- **stdio 输出路径未直接测**：`snprintf` 覆盖格式化引擎，但没有用 `printf`+`fflush`
  走 `writev`；既有 h/i/xyz 仍走适配层的原生 fd 写。
- **无完整 auxv/任意 argv**：`AT_PAGESZ`/`AT_NULL` 与固定 argv 不变；musl 的
  `__init_tls` 走 `builtin_tls`。
- **TLS 只由启动见证**：musl 构建跳过 `arch_prctl` FS 实验，`%fs:0` 的 per-task
  恢复证据仍来自 M2 的无 libc 构建。
- **无 SSE 异常处理**：置了 `CR4.OSXMMEXCPT` 但没有 `#XF` 处理器；SSE 异常会走既有
  故障路径。FU 状态用 FXSAVE（无 AVX/XSAVE）。
- **无取消点**：musl `syscall_cp.s` 的直接 `syscall` 未替换；echo 路径不走取消点。
- **版本与缓存**：musl 构建依赖 `target/musl-1.2.5` 快照存在；缺失时构建失败并提示
  锁文件。

## 7. 裁决

- **M3-D1**：启用 SSE（清 `CR0.EM/TS`、置 `CR4.OSFXSR/OSXMMEXCPT`），不再要求整个
  用户态只用通用寄存器。代价：多一份 per-task FPU 状态与切换开销；内核保持软浮点，
  不新增内核 FPU 使用。
- **M3-D2**：per-task 状态用 `FXSAVE/FXRSTOR`（512 B），不启用 XSAVE/AVX。代价：
  AVX 寄存器不在上下文内；需要时再裁决 XSAVE。
- **M3-D3**：每次切换保存当前任务、恢复目标任务，不做延迟保存。代价：每次切换两条
  指令；换取确定性。
- **M3-D4**：Prima crt 直接调用 `__libc_start_main`，跳过 `rcrt1` 自重定位。代价：
  重定位完全由内核装载器负责；内核只支持 `R_X86_64_RELATIVE` 的静态 PIE。
- **M3-D5**：适配层不定义 libc 同名符号，让 musl 的实现与语义获胜。代价：无 libc
  的旧探针构建若需要这些包装，必须单独提供；当前构建已统一走 musl。
- **M3-D6**：C 入口栈 8 页、`MAX_VSPACE_ENTRIES=192`。代价：每个 C 探针多占 7 页栈
  与映射条目；容量水位已重测，其他服务不受影响。
- **M3-D7**：musl 构建跳过 FS 实验，TLS 由启动成功见证。代价：musl 下没有独立的
  `%fs:0` per-task 恢复证据；M2 的无 libc 证据仍有效。
- **M3-D8**：musl 构建使用 `-fPIC -O2` 静态库；探针/适配层 TU 仍加
  `-mgeneral-regs-only`，避免编译器自身的 XMM 使用干扰 SSE 断言。代价：musl 与
  TU 的编译选项不同，需在构建脚本中分别维护。
- **M3b-D1**：匿名内存由服务管理器在创建 C 任务时**预映射**（栈+arena 同段），
  适配层只做 bump 分配。代价：该 arena 只对 `c_stack` 任务存在；C 程序不能自行
  扩大匿名内存。
- **M3b-D2**：不引入新的内核匿名映射 syscall，复用 `memseg_create/map` 语义。
  代价：`mmap` 的保护/回收语义是适配层的 partial 实现，必须逐项登记。
- **M3b-D3**：`MAX_FRAME_OWNERS` 提到 256 并把归属表移入 `OWNER_ARENA`，而不是
  放宽 32 KiB 注册表预算。代价：普通内存多一处静态 arena；换来既有不变量测试
  仍然是硬门槛。
- **M3b-D4**：`MAX_VSPACE_ENTRIES` 提到 256。代价：vspace arena 段内多 64 条
  容量；这是真实增长，不是放宽重叠/权限检查。
