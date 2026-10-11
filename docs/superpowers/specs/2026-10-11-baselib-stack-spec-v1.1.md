# RuxOS 基础库栈（基座库）技术规格书 v1.1

**文档版本**: v1.1（含阶段四走查 + 阶段五扫雷修订，修订清单见 `walkthrough.md`（同目录） §3）
**创建日期**: 2026-10-11
**项目代号**: `ruxos`（占位，待决策 D-01）
**状态**: 7 项 RED 已通过 v1.1 修订闭环；**待你确认 D-01/02/06/09/13/18 共 6 项 🔴 后方可进入开发**
**适用范围**: RuxOS 微内核之上的三层基础库 —— 共享 crate 集合 / 自家 libc（C ABI）/ Rust core+std 移植
**上游依赖文档**: RuxOS 微内核规格（另册，本册只消费其 ABI 承诺）

> **本文档是本子系统开发的唯一真相源。** 任何 syscall 号、结构体布局、符号名的变更，必须先改本文档 §4，再改代码。

---

## 0. 已锁定决策（用户确认，不可自行更改）

| # | 决策 | 内容 | 影响面 |
|---|---|---|---|
| **K-1** | **ABI 路线 = 混合** | 常规调用走 Linux 风格（`rax`=号，参数 `rdi rsi rdx r10 r8 r9`）；热路径 `ipc_call/ipc_reply/map/page_loan` 走 L4 风格 label + 5 个 message register | 桩生成器必须双模板；musl 适配只用常规路；Rust 服务器可只用热路径 |
| **K-2** | **libc 路线 = musl 移植 + 自研边界** | musl 为 body，只替换 `syscall_arch.h`、`src/thread/*`、`errno/location`、`ld.so` 后端、POSIX 转发层 | 见 §8.2 移植改造清单 |
| **K-3** | **V1 兼容承诺 = 仅自家 Rust/C 程序** | 不承诺 POSIX 符号覆盖率，不承诺 Linux 二进制兼容 | 但 §4.11 persona 字段 + §4.12 符号版本从第一天存在，否则每次加兼容点都要破坏 ABI |

**K-2 与 K-3 的张力处理（本册最重要的架构约束）**：
选择 musl 作 body 但不承诺 POSIX 覆盖，唯一自洽的解释是 —— **V1 交付的是"能用的最小集合"，不是"POSIX 达标"**。因此：
1. V1 只保证 §5 模块清单里的符号可用，musl 其余符号**照常编译进库但不测试、不承诺**（编译通过 ≠ 行为正确）；
2. 所有未承诺行为在 `docs/compat-state.md` 逐条标注 `KNOWN-PARTIAL`，禁止对外宣称 POSIX 兼容；
3. §4.11/§4.12 的预留位是**强制项**，属于 P0，不是"以后再说"。

---

## 1. 项目概述与目标

### 1.1 背景与定位

RuxOS 是 capability-based 微内核。本册解决的是**"内核之上、应用之下"的基础库栈**：让 C 程序能编译链接运行、让自家 Rust 服务器能安全复用 ABI、让 Rust `std` 能为目标平台产出可执行文件。

交付物不是"一个库"，而是**一套分层 + 一条代码生成链 + 一份 sysroot 产物**。

### 1.2 V1 量化目标（验收口径见 §11）

| 指标 | V1 目标 |
|---|---|
| 能编译运行的程序 | 自家 C/Rust 程序 + busybox 静态（作为兼容性探针，**不对外承诺**） |
| 裸 syscall 往返（QEMU `-cpu max`） | ≤ 350 ns |
| 小消息 IPC 往返（热路径） | ≤ 250 ns（含直接 IPC 快路径） |
| `ipc` crate 零依赖体积 | `abi` crate 编译产物 ≤ 2 个 rlib，无 `alloc` 依赖 |
| ABI 一致性 | `cargo gen-abi` 重生成后与仓库内 `include/ruxos/*.h` **零 diff**（CI 强制） |
| Rust 目标 | `-Zbuild-std` 出 `core` + `std`，`std` 单元测试通过 ≥ 60%（§7.3） |
| 崩溃隔离 | 任一用户态服务器崩溃，内核与其余服务器不受影响（§14 影响矩阵） |

### 1.3 角色表

| 角色 | `role` 值 | 特征 | 核心需求 |
|---|---|---|---|
| 内核开发者 | `kernel-dev` | 写内核本体，熟悉 Rust + x86_64 | ABI 类型不漂移；内核不依赖 libc |
| 服务器开发者 | `server-dev` | 写用户态 Rust 服务（rootfs/itss/net…） | 只用 `abi+ipc+server`，不背 libc 的锁与缓冲 |
| libc/工具链维护者 | `libc-maint` | 移植 musl、维护 ld.so 与 sysroot | 边界清晰：只改 5 个替换点，不 fork 全库 |
| 应用作者（外部） | `app-dev` | 拿 sysroot 交叉编译自家 C/Rust 程序 | 一条 `make app` 命令 + 明确"哪些符号可用" |
| 评审/验证 | `verifier` | 跑测试矩阵、性能回归、崩溃注入 | 测试全在 QEMU + 宿主双跑，有可复现的失败产物 |

### 1.4 V1 功能范围（P0/P1/P2）

**P0（V1 必须，缺一不上线）**
- `abi` crate + IDL 生成器（Rust ↔ C 双向，CI 零 diff）
- syscall 桩层（双模板：常规 / 热路径）
- `ipc` crate（路由、打包、批处理、multi-wait 封装）
- crt 家族 + `__libc_start_main` + ABI 版本握手
- musl 移植的 5 个替换点（§8.2）+ 最小 stdio
- `ld.so` 最小闭环（`PT_LOAD` + RELATIVE/GLOB_DAT/JUMP_SLOT + initial-exec TLS）
- `fd ↔ capability` 映射层（含 CLOEXEC 归属规则）
- 单线程假 `pthread`（为后续真多线程保留 ABI）+ TLS 落地
- Rust target JSON + `core` + `-Zbuild-std`
- persona 字段 + 符号版本脚本 + `compat-state.md` 机制
- **信号 ABI（§4.5.1 + `0x03A-0x03D`）** —— `abort`/断言/`SIGSEGV` 的地基，不可推迟到 P1
- **`fork`(单线程)/`execve`/`wait4` 通路（§5.11）** 与 **tty 通路（§5.12）**
- **符号↔syscall 追溯表 `symbol-trace.md`（§4.13）** 及其 CI 校验
- **ABI 版本握手 + `NT_RUXOS_ABI`（§4.15）**
- log server 客户端 + 崩溃上下文捕获

**P1（V1 应有，可降级实现）**
- 真多线程（内核 `Thread` + futex）
- signal 的**扩展能力**：`sigaltstack`、嵌套屏蔽完整语义、实时信号 `SIGRT*`（基础 `sigaction/kill/sigreturn` 已升 P0）
- `dlopen`/`dlmopen`、`dl_iterate_phdr`（后者是 unwinding 前置，若真多线程顺延则升为 P0）
- `libm`、`libc++`/`libstdc++` 链接
- Rust `std` 后端全量（`fs/net/thread/time/env`）

**P2（明确推到 V2，V1 不碰）**
- POSIX 符号覆盖率承诺、LTP/PISA 达标
- Linux 二进制兼容 persona 实现（仅保留 §4.11 接口位）
- 宽字符 / locale / iconv / `wchar_t` API
- 形式化验证、CHERI/riscv 目标、aarch64 移植
- 静态 PIE 之外的高级链接特性（`DT_RELR`、guard、IFUNC）

### 1.5 明确非目标

不做 glibc；不做网络 TLS 库；不做包管理器；不在 libc 里实现路径解析（属 VFS server）；不承诺 ABI 之外的任何 C 语言扩展。

---

## 2. 技术架构

### 2.1 技术栈（全部锁定版本）

| 层 | 组件 | 版本 | 用途 |
|---|---|---|---|
| 语言 | Rust | `nightly-2026-09-15`（`-Zbuild-std`, `asm`） | 内核 + crate + std 移植 |
| C | clang | `19.1.7` | 编 musl/libc（不用 gcc，理由见 §2.2） |
| 汇编 | GNU as | `2.42` | 桩模板、crt |
| 链接 | `ld.lld` / GNU `ld` | `19.1.7` / `2.42` | 内核用 lld；ld.so 用 GNU ld |
| C 库 body | musl | `1.2.5`（vendor 快照，见 §15） | libc 主体 |
| 加载 | limine | `9.2.x`（REV8+） | 提供 `bootmap`/`hh`/模块 |
| 模拟器 | QEMU | `9.2.1`（`-machine q35 -cpu max`） | 全量测试 |
| 调试 | gdb | `15.2` + QEMU `-s -S` | 断点、coredump 分析 |
| 探针 | busybox | `1.36.1`（**仅测试用，不进承诺**） | 兼容性探针 |
| 构建 | cargo + `xtask` + make | 仓库内 | 三阶段 sysroot |
| CI | GitHub Actions 自建 runner | ubuntu-24.04 | 全量矩阵 ≤ 25 min |

### 2.2 选型理由（关键三条）

1. **clang 而非 gcc 编 libc**：musl 只需 C11；clang 的 `-target` + `--sysroot` 对自定义 triple 支持远好于为 gcc 打 cross 补丁（后者要重编 binutils 配置，且现代 C++ 工具链普遍不再保证 gcc 交叉 sysroot 可复现）。
2. **limine 而非 GRUB2**：limine 直接给 `memory_map` + `kernel_file_map` + HHDM 请求，省掉自己写 MB2 解析层；这是微内核早期最没价值的体力活。
3. **musl 而非 glibc/newlib**：musl 内部接口一致（`__syscall` 单点、`errno` 单点、无 IFUNC、无符号版本包袱），替换点只有 5 个；glibc 的适配成本是它的 5~10 倍，newlib 无 TLS 且 POSIX 面窄。

### 2.3 架构图（运行时双通路）

```
              C/C++ 应用                自家 Rust 服务器
                   │                          │
            ┌──────▼──────┐            ┌──────▼──────┐
            │ libc (musl  │            │ server crate│  ← 不链接 libc
            │  body + 自家│            │  (main loop,│
            │  边界层)    │            │   captable) │
            └──────┬──────┘            └──────┬──────┘
                   │                          │
             L3 桩层(双模板)◄──── 共用 ────►L1 ipc crate
                   │                          │
            ┌──────┴──────────────────────────┴──────┐
            │  L2  abi crate / gen-abi  ← 唯一真相源 │
            └──────────────────┬─────────────────────┘
                               │  陷入 (syscall / label)
            ┌──────────────────▼─────────────────────┐
            │  L0  RuxOS 微内核：内存·调度·IPC·能力·IRQ│
            └──────────────────────────────────────────┘
```

**铁律**：L2 不依赖任何层；L1 只依赖 L2；L3 只依赖 L1/L2；libc 高层只依赖 L3~L5；**内核与 libc 都指向 L2，但互不指向对方**。

### 2.4 依赖关系（构建时 vs 运行时）

构建时（先有编译器 → 有 sysroot → 有程序）：
```
host rustc/clang → abi → gen-abi(头文件) → libsys-rs → ipc → server
                                             ↓
              stage1: crt1 + 桩 + 裸 hello  ──┴→ stage2: musl body+边界 → busybox 探针
                                             → stage3: ld.so + 全库 + Rust std
```
运行时启动顺序：`limine → 内核 → root server（持有 Untyped 根）→ log / itss / rootfs → shell`

### 2.5 有状态/无状态标注

| 组件 | 状态性 | 说明 |
|---|---|---|
| `abi`/`gen-abi` | 无 | 纯函数，可并行生成 |
| `ipc` 客户端 | 无全局态 | 状态只在 TCB 里（端点缓存） |
| libc dtable | **进程内**有状态 | 必须可 `fork` 复制 + 锁顺序 |
| malloc arena | 进程内有状态 | 与 `dlopen`/`fork` 三方锁序定义（§6.2.4） |
| `ld.so` 加载表 | 进程内有状态 | `dlopen` 期间持锁，禁止回调进 malloc |
| log server | 全局有状态 | **V1 单点**，降级路径见 §14.5 |

### 2.6 架构原则（违反即打回 PR）

1. **机制在内核，策略在库**：内核里出现 `O_CLOEXEC` 判断 = 违规。
2. **服务器不链接 libc**：链接 libc 的服务器必须写 `MIGRATION: <理由>` 注释并经评审。
3. **类型只来自 IDL**：手写跨语言结构体 = 违规。
4. **`__libc_start_main` 之前禁止 syscall 之外的一切外部调用与 malloc**（§12 红线）。
5. **ABI 变更走"加号不减号"**：号段空洞永不复用，字段只追加。

---

## 3. 权限体系（capability 模型与访问边界）

本节替代模板中的"角色权限矩阵"——OS 基座库的权限对象是**进程/能力**，不是人类角色。

### 3.1 能力类型与权限位

```
Object 类型（ObjType, u16）：
 0x00 Untyped    0x01 Endpoint  0x02 Notification 0x03 Reply
 0x04 Page       0x05 Frame     0x06 AsidPool     0x07 AS
 0x08 Task       0x09 Thread    0x0A IrqHandler   0x0S SchedContext
 0x0C Domain     0x0D Session   0x0E LogHandle    0x0F Timer

Rights（u16 bitmask）：
 bit0 READ  bit1 WRITE  bit2 EXEC  bit3 MAP  bit4 DERIVE  bit5 CALL
 bit6 SIGNAL bit7 CTRL  bit8 UNMAP bit9 RECALL bit10 SET_PRIORITY
 默认：Untyped=DERIVE；Endpoint=CALL；Page=READ|WRITE|MAP；Thread=CTRL|SIGNAL
```

### 3.2 handle 与 cap 表

```rust
#[repr(C)] #[derive(Clone, Copy, Debug, Zeroable, Pod)]
pub struct Handle { pub idx: u32, pub gen: u32 }   // gen = 代数，防 ABA
```
- 每地址空间一张 cap 表，16 MiB 预留，1024 槽起、翻倍增长。
- `idx` 是槽位下标；`gen` 每次槽位复用时 +1；内核查表时比对 `gen`，不匹配 → **`ECAPSTALE`**（不是 `ECAPBAD`），这是句柄泄漏/UAF 类 bug 的唯一可诊断信号。
- cap 表本身是内核对象，libc 侧不可直接读写；`dup` 走 `sys_cap_derive`。

### 3.3 派生链与生命周期

```
Root Untyped (root server, 全物理内存)
  └─ Untyped(2^n) ─ Untyped(小) ─ Object  ← sys_untyped_decant
       回收：Object 无引用 → 槽位 free → gen++ → 内存回 Untyped（sys_untyped_reclaim）
```

**能力无法"撤销"**：派生出的子能力不可回收除非全部 free。因此 §14 的崩溃恢复只能靠"重启服务器重建 session"，不能指望 revoke（设计后果必须写进各 server 实现）。

### 3.4 权限校验伪代码（内核侧，供库侧理解语义）

```
fn sys_xxx(h: Handle, rights: Rights) -> Result {
    let slot = cap_table(current.as).get(h.idx)?;      // OOB → ECAPBAD
    if slot.gen != h.gen { return Err(ECAPSTALE); }
    if !slot.rights.contains(rights) { return Err(EACCESS); }  // 不泄露对象存在性
    ...
}
```
**规则**：无权限与不存在返回不同错误码（`EACCESS` / `ECAPBAD`），但**消息长度、时延必须恒定**，避免通过时序探测他进程能力。

### 3.5 敏感操作清单（必须 `CTRL`/`SIGNAL` 独立权限位）

`sys_thread_ctrl(suspend/resume/set_priority)`、`sys_irq_ctrl`、`sys_untyped_reclaim`、`sys_as_map(到他人 AS)`、`sys_domain_enter(Linux persona)`。

### 3.6 跨域防护

- `ECAPFORBID`：能力属于其他 AS，不可向本 AS 注入（libc 侧 `fd` 导入必须经 itss 代理）。
- `EDEADJK`：内核检测环等待 → 返回而不是死锁；libc 必须原样上抛，**禁止重试**。

---

## 4. ABI 规范（核心章节）

### 4.1 陷入寄存器约定（常规路 = Linux 风格）

| 项 | 规定 |
|---|---|
| 陷入指令 | `syscall`（x86_64）。内核入口校验 `IA32_EFER.SCE`，用户态 `rcx`/`r11` 视为已破坏 |
| 调用号 | `rax`（零扩展，`eax` 写入即清零高位） |
| 参数 | `rdi rsi rdx r10 r8 r9`（**libc 桩负责把 SysV 的 `rcx` 参数搬到 `r10`**） |
| 返回值 | `rax`；`rax ∈ [-1, -2047]` = `-errno`；`rax ≤ -2048` = 内核扩展码 |
| 破坏寄存器 | `rax rcx r10 r11 rdi rsi rdx r8 r9` |
| 保存寄存器 | `rbx rbp rsp r12 r13 r14 r15`，**方向标志 DF 必须为 0**（libc 入口 `cld`） |
| 栈 | 调用前 16 字节对齐；返回不消费栈 |
| 7 参以上 | 第 7 参起经 `rsi` 传结构体指针（`struct syscall_extra`） |

### 4.2 热路径（L4 风格 message register）

仅用于 `SYS_IPC_*`、`SYS_MAP`、`SYS_PAGE_LOAN`、`SYS_UNTYPED_DECANT`。

```
rax = 调用号            ← 仍是常规号，保证同一入口
rdi = label  (u64)
rsi rdx r10 r8 r9 = mr[0..4]  ← 消息负载，内核不解引用，纯值传递
返回：rax = 错误码/0；rsi..r9 = 回复 mr
```

**label 位域（v1 冻结）**
```
 bit 0..11  endpoint : u16   端点槽位下标（0 = 本线程 reply 端口）
 bit 12..15 method   : u4    服务方法号
 bit 16..23  mrlen   : u8    mr 有效字节数 ≤ 40（0 = 纯 call）
 bit 24..39  badge   : u16   服务端返回时回填的标识
 bit 40      direct  : bool  请求走直接 IPC（失败自动降级为普通，语义不变）
 bit 41      nonblock: bool  等价 sys_nbcall
 bit 42..63  reserved: 必须为 0，非 0 → EBADLABEL
```
> `reserved` 必须为 0 且内核校验：这是 v1 冻结位域能长期演进的唯一保障。

### 4.3 错误码设计

**内核码空间**：`-1..-132` 直接采用 Linux errno 数值（不重排，理由见待决策 D-07）；`-1024..-1055` 为内核扩展：

| 名称 | 值 | 语义 | 出现场景 |
|---|---|---|---|
| `ECAPBAD` | -1024 | 槽位无效 | handle 越界/未分配 |
| `ECAPSTALE` | -1025 | 代数不匹配 | UAF/句柄泄漏诊断锚点 |
| `ECAPFORBID` | -1026 | 跨 AS 注入被拒 | 安全 |
| `EUNTEXHAUST` | -1027 | Untyped 余量不足 | 内存耗尽前兆 |
| `EIPCTIMEOUT` | -1028 | 等待超时 | 带 timeout 的 call |
| `EIPCNOCON` | -1029 | 端点无接收方且非阻塞 | 服务未起 |
| `EIPCCLOSED` | -1030 | 端点已关闭 | 服务崩溃/重启 |
| `EIPCDEADLK` | -1031 | 环等待 | 禁止重试 |
| `EIPCFLABEL` | -1032 | label 非法/reserved 非 0 | 编程错误 |
| `EALIGN` | -1033 | 对齐不足 | map/pool |
| `ERANGE2` | -1034 | 参数超范围（非 Linux ERANGE 语境） | decant 阶数 |
| `EUNIMPLEMENTED` | -1035 | 明确未实现（≠ENOSYS） | P1/P2 占位 |
| `EABIMISMATCH` | -1036 | ABI/persona 不匹配 | 加载期 |
| `EQUOTA` | -1037 | 配额（cap 槽/fd/内存）超限 | 资源控制 |
| `EPAGENEEDBACK` | -1038 | page loan 未归还 | 内核不变量 |
| `ENOTSLOT` | -1039 | 无空闲槽 | 需回收 |
| `EBUSYCORE` | -1040 | 目标核不可调度 | 亲和性 |
| `ESTALEOBJ` | -1041 | 对象处于回收中 | 重试一次可 |
| `EDEADLINE` | -1042 | deadline 已过 | 实时 |
| `ENOSIGNALSRC` | -1043 | 信号源鉴权失败 | 安全 |
| `ESECURITY` | -1044 | 通用安全拒绝 | 不透明 |

**libc 翻译**：`-1..-132` 直接取负即 `errno`；扩展码映射到 §4.3.1 表（`EIPC*→EPIPE/ETIMEDOUT` 等有损，**必须在 `compat-state.md` 标注为有损映射**），且提供 `int ruxos_errno(void)` 返回原始码供自家程序精确判断。

### 4.4 syscall 号表（v1 冻结，空洞永不复用）

**段 0x00 通用**
| 号 | 名称 | 参数（常规） | 语义 / 错误 |
|---|---|---|---|
| 0x000 | `sys_nop` | — | 时延基线测量，恒返回 0 |
| 0x001 | `sys_abi_version` | — | 返回 `ABI_VERSION: u32`；libc 启动握手 |
| 0x002 | `sys_features` | `mask(u64)` | 特性位图：**bit0** `multi_wait`、**bit1** `direct_ipc`、**bit2** `page_loan`、**bit3** `futex`、**bit4** `signals`、**bit5** `console_direct`、**bit6** `as_derive_cow`、**bit7** `pool_ctrl`；未置位特性的调用必返回 `ENOSYS`，libc 据此选退化路径（Y-3/Y-4） |
| 0x003 | `sys_get_errno_raw` | — | 调试用，返回本线程上次内核码 |
| 0x004 | `sys_log_write` | `level(u32) buf(ptr) len(u64)` | 早期日志后门（启动后走 log server） |
| 0x005 | `sys_persona_get` | `out(ptr)` | 本进程 persona |
| 0x006 | `sys_cpu_id` | — | 当前 CPU 号 |

**段 0x10 虚拟内存 / 物理内存**
| 号 | 名称 | 参数 | 语义 |
|---|---|---|---|
| 0x010 | `sys_map` | **热路径** `label`+mr：`src(Page), dst(AS), vaddr, perms` | 映射；`EPAGENEEDBACK` 语义 |
| 0x011 | `sys_unmap` | `as(u64) vaddr(u64) len(u64)` | 解除映射，返回实际解除字节 |
| 0x012 | `sys_protect` | `vaddr len prot` | 改权限 |
| 0x013 | `sys_msync` | `vaddr len flags` | 文件映射回写（转发 rootfs） |
| 0x014 | `sys_madvise` | `vaddr len advice` | `MADV_DONTNEED/FREE/FAULT` |
| 0x015 | `sys_page_loan` | **热路径**：`src, dst_ep, count, flags` | IPC 页借出，见 §6.3.2 |
| 0x016 | `sys_page_reclaim` | `pages(ptr) cnt` | 内核主动回收 |
| 0x017 | `sys_madv_reserve` | `vaddr len` | 预约地址不占物理 |
| 0x018 | `sys_mremap` | `old new len flags` | 返回新地址 |
| 0x019 | `sys_brk` | `addr(u64)` | 堆边界；返回当前 brk。实现：libc 维护 + 必要时 `sys_map` 取池页（`RED-1`） |
| 0x01A | `sys_as_derive` | `src(u64) flags(u64) out(ptr)` | 派生子地址空间，`flags=AS_DERIVE_COW` 供 `fork` 用（`RED-2`） |
| 0x01B | `sys_pool_ctrl` | `pool(u64) CMD_FILL/DRAIN cnt` | anonymous 页池控制（§4.14） |

**段 0x20 能力对象**
| 号 | 名称 | 参数 |
|---|---|---|
| 0x020 | `sys_untyped_decant` | **热路径**：`uobj, type, size_bits, align_bits, out_cap` |
| 0x021 | `sys_untyped_reclaim` | `cap(u64)` |
| 0x022 | `sys_cap_derive` | `src(u64) rights(u16) out(ptr)` |
| 0x023 | `sys_cap_destroy` | `cap(u64)` |
| 0x024 | `sys_cap_retain` | `cap(u64) cnt(u32)` |
| 0x025 | `sys_cap_lookup` | `handle(u64) out(obj_info ptr)` |
| 0x026 | `sys_cap_transfer` | `cap, target_thread` （注入他人须 `CTRL`） |
| 0x027 | `sys_ep_ctrl` | `cap, CMD_SHUTDOWN/CLOSE/GETSTATE` |
| 0x028 | `sys_notif_bind` | `notif(u64) ep(u64) mask(u64)` |

**段 0x30 进程/线程/调度**
| 号 | 名称 | 参数 |
|---|---|---|
| 0x030 | `sys_task_create` | `as(u64) out(ptr)` |
| 0x031 | `sys_thread_create` | `stack_top, ctx, sched, out` |
| 0x032 | `sys_thread_exit` | `code(u64)` |
| 0x033 | `sys_thread_wait` | `tid(u64) out(status ptr)` |
| 0x034 | `sys_thread_yield` | `flags` |
| 0x035 | `sys_thread_ctrl` | `tid, CMD_SUSPEND/RESUME/GETREGS/SETREGS` |
| 0x036 | `sys_sched_ctrl` | `tid, prio, class, quantum` |
| 0x037 | `sys_sched_affinity` | `tid, cpumask` |
| 0x038 | `sys_futex` | `addr op val timeout` （P1，见 D-02） |
| 0x039 | `sys_wait_multi` | `eps(ptr) n out(ptr) timeout` （P1） |
| 0x03A | `sys_signal_set` | `sig(u32) action(ptr) old(ptr)` — 登记 handler 与 mask（`RED-3`） |
| 0x03B | `sys_signal_send` | `tid(u64) sig(u32)` — 需目标 `SIGNAL` 权限，否则 `ENOSIGNALSRC` |
| 0x03C | `sys_sigreturn` | `uc(ptr)` — 恢复 trap frame |
| 0x03D | `sys_signal_pending` | `out(ptr)` — 返回 pending 位图 |

**段 0x40 IPC 热路径（本册最核心）**
| 号 | 名称 | 语义 |
|---|---|---|
| 0x080 | `sys_ipc_call` | `label`+mr → 阻塞直到对端 accept；回复经 mr 返回 |
| 0x081 | `sys_ipc_reply` | 对当前挂起 sender 回复（`endpoint=0`） |
| 0x082 | `sys_ipc_wait` | 取一条入站（返回 caller badge/mr） |
| 0x083 | `sys_ipc_nbcall` | 非阻塞，无人接收 → `EIPCNOCON` |
| 0x084 | `sys_ipc_notify` | 向 Notification 置位，永不阻塞 |
| 0x085 | `sys_ipc_wait_notify` | 等待 Notification 位，`bitset out` |
| 0x086 | `sys_ipc_poll` | 查询端点状态不消费 |
| 0x087 | `sys_ipc_recv_from` | 接受并返回 caller AS/thread（鉴权用） |

**段 0x50 中断/时间**
| 号 | 名称 | 参数 |
|---|---|---|
| 0x090 | `sys_irq_get_cap` | `irq(u32) out` |
| 0x091 | `sys_irq_wait` | `cap, timeout` |
| 0x092 | `sys_irq_ack` | `cap, eoi_mode` |
| 0x093 | `sys_irq_mask` | `cap, mask` |
| 0x094 | `sys_clock_gettime` | `clock_id(u32) out(timespec ptr)` |
| 0x095 | `sys_nanosleep` | `req(u64) rem(ptr)` |

**段 0x60 转发/杂项**
| 号 | 名称 | 参数 |
|---|---|---|
| 0x0A0 | `sys_info` | `what(u32) out(ptr) len` — 页大小/内存/CPU 数 |
| 0x0A1 | `sys_getrandom` | `buf(u64) len(u64) flags(u64)` — V1 仅非阻塞：熵不足返回 `EAGAIN`，**绝不返回 0 字节当成功**；阻塞版属 P1（`GRND_BLOCKING`，Y-2） |
| 0x0A4 | `sys_console_write` | `buf(ptr) len(u64)` — boot 期直通串口/UART，`bit5 console_direct` 控制；启动后仅 itss/tty 持有（RED-7） |
| 0x0A5 | `sys_crash_ack` | `seq(u64)` — crash server 确认已捕获（§5.9） |
| 0x0A2 | `sys_debug_break` | `code` — 触发内核断点 |
| 0x0A3 | `sys_domain_enter` | `persona(u32)` — V2 Linux 兼容的入口位（V1 返回 `EABIMISMATCH`） |

**未分配（保留池）**：`0x007-0x00F`、`0x01C-0x01F`、`0x029-0x02F`、`0x03E-0x07F`、`0x088-0x08F`、`0x096-0x09F`、`0x0A6-0x0FF`。**已占用号永不复用**；废弃调用保留号并返回 `ENOSYS` 占位（§12 红线 3）。

### 4.5 核心结构体（IDL 摘录，完整见 `abi/idl/*.idl`）

```idl
struct Msg {                 // 常规路的可选长消息（热路径不用）
  u64 label;
  u64 mr[5];
  u64   buf_ptr;   // 0 = 无长消息
  u32   buf_len;   // 上限 = 4 KiB（一页），超过须用 page_loan
  u32   _pad0;
}

struct ObjInfo { Handle h; u16 type; u16 rights; u32 refs; u64 extra; }

struct BootInfo {            // 由 root server 转发的启动信息（非内核直传）
  u64 magic;                 // 0x5255584F424F4F51 "RUXOBOOT"
  u32 abi_version; u32 _pad;
  u64 phys_base; u64 phys_size; u64 hhdm_base;
  u64 modules[8]; u64 module_count;
  Handle root_untyped; Handle log_endpoint; Handle itss_endpoint;
}

struct Auxv { i64 type; u64 val; }   // 结尾 AT_NULL

struct CrashInfo {
  u64 fault_vaddr; u64 rip; u32 trap_no; u32 err_code;
  u64 regs[16]; Handle thread; u64 crash_seq;
}
```

```rust
// Rust 侧（生成物，节选）
#[repr(C)] #[derive(Clone, Copy)] pub struct Msg { pub label: u64, pub mr: [u64; 5], pub buf_ptr: u64, pub buf_len: u32, pub _pad0: u32 }
```

**规则**：所有跨语言结构体 `#[repr(C)]`，尾部 `_padN: u32` 为 v1 预留；`static_assert(sizeof(Msg)==64)` 在生成的 C 头里必须存在（`label 8 + mr 40 + buf_ptr 8 + buf_len 4 + _pad 4 = 64`）。

### 4.5.1 信号相关结构（`RED-3` 新增，随 v1 一起冻结）

```idl
struct Sigaction {
  u64 handler;        // 0 = SIG_DFL, 1 = SIG_IGN, 否则用户函数
  u64 flags;          // SA_RESTART|SA_SIGINFO|SA_ONSTACK|SA_NODEFER
  u64 restorer;       // 指向 sigreturntramp（libc 提供，见下）
  u64 mask;           // 64-bit 位图
}

struct Siginfo { u32 signo; u32 code; u32 _pad; u64 fault_addr; u64 uid; u64 timer_val; }

struct Sigcontext {          // 内核在构造 handler 栈帧时写入，libc sigreturn 原样读回
  u64 regs[16];              // rax rbx rcx rdx rsi rdi rbp r8..r15 rsp rip rflags
  u64 retaddr;               // = handler 返回后跳转 → restorer
  u64 fpstate_ptr;           // 对齐 64，指向保存的 x87/SSE
  u64 old_mask;
  u64 krn_cookie;            // 内核校验用，非 0 且匹配才允许 sigreturn
}
```

```c
_Static_assert(sizeof(Sigcontext) == 16*8 + 4*8, "sigcontext layout frozen");
```

规则：
1. 内核只在**返回用户态**时投递 pending，不打断内核态执行；投递 = 在目标栈上构造 `Sigcontext` 并把 `rip` 改为 handler。
2. `restorer` 必须是 libc 提供的固定桩（`sigreturntramp`），**不接受应用自带**，否则 `sigreturn` 无法校验 `krn_cookie`。
3. 嵌套信号：`mask` 自动屏蔽同号（除 `SA_NODEFER`），alt-stack（`sigaltstack`）属 P1。
4. `SIGSEGV/SIGBUS/SIGFPE/SIGILL` 由内核 trap 直接映射（`trap_no` → `signo` 表进 IDL，不得在 libc 里硬编码）。
5. 无 handler 且 `SIG_DFL` = 终止：内核发 itss，libc 不参与 —— 保证 `abort()` 行为可预测。

### 4.6 IPC 消息与能力传递


- **mr 里只能放值，不能放指针**。指针一律经 `buf_ptr`（同 AS 有效）或 `page_loan`。
- **传递能力**：`sys_cap_transfer` 显式传给"目标线程"，接收方在下次 `ipc_wait` 的返回 mr 里得到新 handle（`badge` 位标 `CAP_IN`）。不隐式随消息传（避免生命周期歧义）。
- 消息最大 `mr=40B` + `buf=4KiB`；更大走 `page_loan`。**这是硬上限，写进 §12 红线。**

### 4.7 bootinfo / auxv

```
AT_PHDR=1  AT_PHENT=2  AT_PHNUM=3  AT_PAGESZ=4  AT_ENTRY=5  AT_BASE=6 (ld.so)
AT_HWCAP=7  AT_RANDOM=8(16B 熵指针)  AT_EXECFN=9
AT_RUX_CAPTAB=16  AT_RUX_ABI=17  AT_RUX_PERSONA=18  AT_RUX_EP_ROOT=19
AT_NULL=0
```
`argv/envp/auxv` 布局：栈顶 `argc` → `argv[]` → `envp[]` → `auxv[]` → `NULL`，与 SysV 一致（musl 无需改）。

### 4.8 ELF / TLS / 链接约定

| 项 | 规定 |
|---|---|
| ELF | x86_64 ELF64，`e_type=ET_DYN`（PIE）为默认；静态可执行也走 `ld.so` 时保留 `PT_INTERP` |
| TLS 模型 | **只承诺 `Initial-Exec`**（`-ftls-model=initial-exec` 编译 libc 内部符号）；应用用 `Local-Exec`；`Global-Dynamic` 属 P1 |
| TLS 块布局 | musl 风格：`fs` → TLS block 起始，`TCB` 在 `fs - sizeof(TCB)` |
| 栈对齐 | 16B；`_start` 处 `rsp` 按 SysV 语义 |
| 重定位支持（V1） | `NONE, 64, COPY, GLOB_DAT, JUMP_SLOT, RELATIVE, TLSDESC(可退化), DT_REL/RELA, DT_INIT_ARRAY/FINI_ARRAY` |
| 不支持（V1） | `IRELATIVE`（libc 构建期禁用 ifunc）、`DT_RELR`、`PLT` 惰性以外的优化 |
| 页大小 | 4096（`sys_info` 可查询，代码禁止硬编码） |
| redzone | 128B，可关闭（内核 `EFNOREDZONE`） |

### 4.9 符号与版本脚本（P0，第一天启用）

```
libc.map:
RUX_1.0 {
  global:
    read; write; open; close; lseek; mmap; munmap; prctl;
    printf; fprintf; snprintf; malloc; free; calloc; realloc;
    __ruxos_call; __ruxos_call_mr; __ruxos_call_label;
  local: *;                    // 一切内部符号默认隐藏
};
```
- 编译默认 `-fvisibility=hidden`，公开符号逐个 `__ruxos_public` 标注。
- 内部跨 .so 调用一律用 `GLIBC_PRIVATE` 兼容别名（musl 不需要，自研部分若被 `ld.so` 用到则加 `hidden` 版本符号）。
- **`-Wl,--version-script=libc.map` 写进构建，不可选。**

### 4.10 数据表示契约

| 项 | 规定 | 理由 |
|---|---|---|
| `off_t/ino_t/nlink_t/dev_t` | `i64/u64/u64/u64` | 64 位一律，无 32 位变体 |
| `time_t/clock_t/suseconds_t` | `i64` | 2038 无关 |
| `long` | 8 字节（LP64） | 冻结进 `abi::typesize` 静态断言 |
| `char` 符号性 | **signed**（`-fsigned-char`） | 全局强制，构建系统统一注入 |
| `wchar_t` | `i32`，但 API 属 P2 | |
| `size_t` | `u64`，`ssize_t=i64` | |
| `pid/tid` | `u32`，0 非法 | |
| 字节序 | 小端 | |
| 结构体尾部 | `u32 _padN` + `static_assert(size)` | 只追加不改布局 |

### 4.11 persona（兼容演进预留位，P0）

```rust
#[repr(u32)] pub enum Persona { Native = 0, Posix = 1, Linux = 2 } // Linux = V2 预留
```
- persona 存在于 **AS 对象**（`sys_task_create` 参数）与 `AT_RUX_PERSONA`。
- libc 启动时校验：`Native` → 正常；`Posix` → 打开承诺检查表；`Linux` → 当前返回 `EABIMISMATCH` 并终止。
- V1 唯一允许的值是 `Native`/`Posix`；`Linux` 位存在即保证 V2 加兼容层时**不改 ABI**。

### 4.12 兼容性状态记录机制（K-3 的护栏，P0）

`docs/compat-state.md` 每行：`符号 | 模块 | 状态(OK/PARTIAL/UNTEST) | 已知偏差 | 探针用例`。
- CI 校验：表内 `OK` 项必须有对应测试；`PARTIAL/UNTEST` 必须有偏差说明。
- 任何 PR 新增公开符号必须同时更新此表，否则 CI 拒绝。
- 对外文档只允许引用本表 `OK` 行。**这是"不承诺 POSIX 但将来能升级承诺"的唯一路径。**

### 4.13 符号 ↔ syscall 追溯表（P0 交付物，`RED-1` 修复）

规格里"libc 有某符号"与"内核有某原语"**必须逐条对上**，否则实现期才发现缺号。交付物：
`abi/idl/trace.idl` → 生成 `docs/symbol-trace.md`，每行 `公开符号 | 常规桩 | 热路径 | 依赖的 server 方法 | 失败错误集 | compat-state 状态`。

CI 规则（P0）：
- 每条公开符号必须有非空 trace 行；trace 行引用的 syscall 号必须存在于 §4.4；
- 每条 trace 行必须声明"失败错误集"，且其中每个错误码都在 §4.3 定义（防发明错误码）；
- 内核暴露但无任何符号引用的 syscall 必须在 `abi-changelog.md` 注明"预留"。

已补齐的对齐项：

| 符号 | 原缺口 | 处理 |
|---|---|---|
| `brk` | 无内核原语 | 新增 `0x019 sys_brk`（libc 维护边界 + 池取页） |
| `fork` | 无 AS 派生原语 | 新增 `0x01A sys_as_derive(AS_DERIVE_COW)`（§5.11） |
| `mlock/munlock` | 微内核无交换语义 | 返回 `0`（no-op）+ 登记 `PARTIAL`，**不返回 ENOSYS**（大量程序会因 `mlock` 失败而放弃运行） |
| `sigaction/kill/sigreturn` | 整块缺失 | 新增 `0x03A-0x03D` + §4.5.1 |
| `abort` | 未指定实现路径 | `abort() → sys_signal_send(self, SIGABRT) → 内核 → itss 终止` |
| `poll/select` | 依赖 P1 `wait_multi` | V1 退化为 `nanosleep` 轮询，登记 `PARTIAL`（Y-3） |
| `printf/串口` | 无 tty 后端 | 新增 `0x0A4 sys_console_write`（§5.12，RED-7） |
| `munmap/msync/madvise` | 号已存在但 flag 未定义 | §4.14 补 flag 位 |

### 4.14 anonymous 映射与 `mmap` flag 语义（`RED-5` 修复）

微内核下内核只认"页能力"，不认 `MAP_ANONYMOUS`。因此引入**页池对象**：

```
mem_pool（每 AS 一个，由 libc 从 Untyped decant 而来）
  ├─ 预填充：sys_pool_ctrl(CMD_FILL, cnt)   // 内核从 Untyped 造 cnt 个 Page
  └─ sys_map(src_cap = 0)  → 含义"从本 AS 池取 1..N 页"，池空 → EUNTEXHAUST
```

`mmap` flag 位冻结（值与 Linux 不同，**只在本表定义，禁止应用假设 Linux 数值**）：

| flag | 值 | 语义 | V1 |
|---|---|---|---|
| `MAP_FIXED` | 0x01 | 强制覆盖；与已映射区重叠 → 先 `sys_unmap` | ✅ |
| `MAP_ANONYMOUS` | 0x02 | 走 `mem_pool` | ✅ |
| `MAP_SHARED` | 0x04 | 需 `fd` 对应对象的 `MAP` 权限；`msync` 生效 | ✅ |
| `MAP_PRIVATE` | 0x08 | COW 视图，靠 `sys_as_derive`/内核 fault 复制 | 只读映射 ✅ / 写 COW P1 |
| `MAP_NORESERVE` | 0x10 | 记账不占池（首次 fault 才取页） | ✅ |
| `MAP_GROWSDOWN` | 0x20 | 仅栈区允许，其余 `EINVAL` | ✅ |
| `MAP_UNINIT` | 0x40 | **不保证清零**（性能路径，内部用；公开 `mmap` 默认清零） | 内部 |
| `MAP_POPULATE` | 0x80 | 同步取页，池不足 → `ENOMEM` | P1 |

`prot` 位：`PROT_NONE/READ/WRITE/EXEC`，`EXEC` 与 `WRITE` **不可同时置位**（W^X 强制，违反 → `EACCESS`）。这条是内核约束，写进 trace 表以免 libc 偷偷绕过。

### 4.15 ABI 版本与 `NT_RUXOS_ABI` 握手（`RED-6` 修复）

```
ABI_VERSION = (major << 16) | (minor << 8) | patch      // u32
major: 破坏性（号/布局/label 位域/persona 枚举变更）
minor: 追加（新号、新 flag 位、新结构尾部 _pad 转实字段）
patch: 无 ABI 影响
```

| 载体 | 位置 | 校验者 | 时机 |
|---|---|---|---|
| 动态 | ELF note `NT_RUXOS_ABI`（`desc = ABI_VERSION`，每个 so 必带） | `ld.so` | 加载每个对象前 |
| 静态 | `crt1` 内嵌常量 + `sys_abi_version()` | `__libc_start_main` | 进程启动第一件事 |

规则：
1. **major 必须完全相等**，不等 → `EABIMISMATCH`，`ld.so` 拒绝加载并打印 `expected 0x00030000, got 0x00020105`。
2. minor：内核 ≥ 对象允许（向前兼容新号），对象 > 内核时，仅当该对象实际引用了高 minor 号才拒绝（`sys_features` 辅助判定）。
3. **禁止"尽量跑"降级**：任何 mismatch 都是终止，不是警告。
4. `patch` 不参与判定。
5. note 缺失（第三方 so）→ 按 `major` 相等假设 + 写 `compat-state.md` 的 `UNTEST` 行。


---

## 5. 功能模块详细设计

每个模块按统一格式：职责 / 输入输出 / 规则与边界 / 异常路径 / 测试 / 幂等与并发。

### 5.1 M01 `abi` crate + IDL 生成器

**职责**：唯一真相源；产出 Rust 定义、C 头、文档表、测试夹具四份同源物。

**目录**
```
abi/idl/{syscall,label,err,struct,obj,signal,pool,trace}.idl
abi/src/{lib,syscall,types,msg,obj}.rs   // 由 idl 生成，禁手改（顶部 GENERATED 标记）
gen-abi/src/{main,emit_rust,emit_c,emit_doc,error}.rs   // xtask gen-abi
docs/symbol-trace.md                      // 生成物（§4.13）
include/ruxos/{abi.h,syscall.h,err.h,types.h}           // 生成物，入版本库
```

**规则**
- IDL 不支持泛型/union（union 手写 `#[repr(C)]` 包装并注明 SAFETY）。
- 每个 syscall 条目含：号、名、参数类型数组、热路径标记、返回语义、错误集、最小 ABI 版本。
- 生成器输出必须**字节稳定**（不随 map 遍历顺序变化）→ 所有表按号排序。

**异常路径**：号冲突 → 生成失败；位域超 64 位 → 失败；reserved 位被占用 → 失败。

**测试**：`gen-abi --check`（重生成后 `git diff --exit-code`）；C 头用 `cc -fsyntax-only` 编译；Rust 端 `static_assert` 由 `const _: () = assert!(size_of::<Msg>()==64)`。

**幂等**：生成是纯函数，重复执行结果一致（这是"CI 零 diff"能成立的前提）。

### 5.2 M02 `ipc` crate（用户态 IPC 客户端）

**职责**：把"给某服务发某方法"翻译成 label/mr，处理路由、缓存、批处理、multi-wait。

**核心 API**
```rust
pub struct Router { /* 服务名 → Endpoint 缓存，进程内只读表 */ }
pub fn resolve(name: &str) -> Result<Handle>
pub fn call(ep: Handle, method: u8, args: &MsgArgs) -> Result<Reply>
pub fn call_batch(reqs: &[Req], out: &mut [Reply]) -> usize   // 合并陷入
pub fn wait_any(&[WaitSource], timeout: Option<Deadline>) -> Result<Event>
```

**规则**
- `resolve` 结果缓存于 TCB 局部表，`EIPCCLOSED` 时失效重取（**只重试一次**，防风暴）。
- 服务重启检测：`badge` 高 16 位带 session epoch，epoch 变即重建 session。
- `call_batch` 仅在目标同端点时合并；跨端点退化为多次（语义等价）。
- 端点缓存表大小上限 64 项，超出 LRU 淘汰 → `EQUOTA` 不上抛（内部降级）。

**异常路径**：超时 → `EIPCTIMEOUT` 原样上抛；环等待 → `EIPCDEADLK` **禁止重试**并写 log；`mrlen > 40` → 编程错误，debug 断言 + release 返回 `EINVAL`。

**测试**：与内核无关的纯逻辑在宿主跑（用 §7.2 fake backend）；真 IPC 在 QEMU 跑 `ipc-{call,reply,timeout,epoch}`。

**并发**：`Router` 无锁只读；写路径只在 init 期；多线程共享端点必须各自 `dup`（文档写明，因为内核端点是队列共享的）。

### 5.3 M03 `libsys-rs`（Rust 裸 syscall 层）

给自家 Rust 程序/服务器用，**不含 libc**。提供 `syscall0..6`、`ipc_call_mr`、能力 `Handle` 方法。约束：只用 `asm!` 且 `nomem`/`volatile` 精确；无 `unsafe` 之外的分配；panic 时只能 `sys_log_write`。

### 5.4 M04 桩层生成（C 侧，双模板）

```asm
// 常规模板（生成）
.rux_stub write:
    mov  eax, 0x61            ; 号（示例）
    mov  r10, rcx             ; SysV 第4参 → r10
    syscall
    test rax, rax
    jns  .Lret
    cmp  rax, -2048
    jl   .Lkext               ; 内核扩展码
    neg  eax                  ; 取负存 errno
    mov  DWORD PTR fs:[rax + TLS_ERRNO_OFF] ...
    mov  eax, -1
.Lret: ret
```
**规则**：模板文件 `stubs/{regular,hot}.S.tpl` 由 gen 生成；**禁止手写桩**；每个桩有 `.type foo,@function` + 版本符号。
**异常路径**：内核扩展码走 `__ruxos_map_kerr`（集中映射表，唯一一处），映射失败 → `errno=EUNKNOWNERR` 且保留原始码可查。

### 5.5 M05 musl 移植与边界替换（K-2 落点）

**只改 5 个点（vendor 树用 `ruxos/` 目录覆盖，不改上游文件）**

| # | 替换 | 上游位置 | 自研位置 | 要点 |
|---|---|---|---|---|
| E1 | 陷入原语 | `arch/x86_64/syscall_arch.h` | `ruxos/syscall_arch.h` | 双约定；6 参版本；返回负 errno 语义与 musl 天然一致（musl 就是 `if (uerror(a))`） |
| E2 | errno 位置 | `src/internal/syscall_ret.h` + `__errno_location` | `ruxos/errno.c` | `fs:` 相对 TLS 偏移，不用 `__thread`（启动前可用） |
| E3 | 线程/TLS | `src/thread/*`, `arch/x86_64/syscall_ret` 的 `__clone` | `ruxos/thread/*` | `pthread_create → sys_thread_create`；V1 先单线程假实现（P1 换真） |
| E4 | 动态加载 | `ldso/dynlink.c` | `ruxos/ldso/dynlink.c` | 内核只给 `map`，其余照 musl 结构改；删掉 Linux 专有 `getauxval(AT_SYSINFO)` 假设 |
| E5 | 转发后端 | `src/stat/* src/fcntl/* src/unistd/*` 中依赖 procfs/ioctl 的 | `ruxos/backend/*` | 一律改走 rootfs/itss session；无对应语义的返回 `ENOSYS` 并登记 `compat-state.md` |

**移植规则**
- musl 版本快照 `1.2.5`，以 git submodule + patch series 维护，**每次上游更新重放补丁并跑探针集**。
- 构建关闭：`WIDE-CHAR`、`ICONV`、`LIBUNWIND`(V1)、`IFUNC`、`DNS`（转发给 net server）。
- 静态优先：sysroot 先出 `libc.a`，再出 `libc.so`（因为 §5.6）。

**异常路径**：任一替换点缺失符号 → 链接期失败（不允许多余 fallback 定义）。

**测试**：musl `make check` 子集（约 200 用例）在 QEMU 跑，通过率记录进 `compat-state.md`（**不作为承诺，作为趋势指标**）。

### 5.6 M06 `ld.so`

**职责**：`_dl_start` → 自身重定位 → 读 `PT_INTERP` → 加载依赖闭包 → 重定位 → 调 `__libc_start_main`。

**规则**
- 搜索路径：`AT_RUX_*` 提供的 `LD_LIBRARY_PATH` 等价 + `/usr/lib/ruxos`；不支持 `$ORIGIN`（P1）。
- 符号查找顺序：global scope BFS，按 DT_NEEDED 展开顺序；支持 `RTLD_GLOBAL/LOCAL`（P1 的 `dlopen`）。
- `dl_iterate_phdr` **必须在 unwinding 前完成**（否则 Rust panic/C++ 异常全挂）。
- **`ld.so` 不得链接 libc**（自带 6 个裸桩）。

**异常路径**：找不到 so → `ENOENT` 且打印搜索过的 3 条路径；ABI 不匹配 → 拒绝加载并 `EABIMISMATCH`；循环依赖 → 打断并警告。

**测试**：`ld-{load,rel,tls,lazy,now,broken,mismatch}` 六组，其中 `broken` 构造 12 种畸形 ELF 全部安全拒绝。

### 5.7 M07 `fd ↔ capability`（dtable）

**职责**：POSIX fd 语义 ↔ 内核能力的映射与生命周期。

```rust
struct FdSlot { handle: Option<Handle>, flags: u32 /*O_CLOEXEC 等*/, offset: u64, cookie: u64 }
struct Dtable { slots: Box<[FdSlot]>, len: u32, max_fd: u32, lock: Lock }
```

**规则（含关键归属决策）**
1. `O_CLOEXEC` 是 **libc 侧 flags**，不进内核 rights（策略/机制分离）。`execve` 由 libc 遍历 `[0, max_fd)` 关闭置位项，因此 `max_fd` 必须 O(1) 维护，禁止扫到 4096。
2. `dup/dup2/dup3` = 槽位复制 + `sys_cap_retain`；**先增引用再占槽**，占槽失败必须回滚引用（否则泄漏）。
3. 扩容：翻倍 + 整体搬迁 + 持锁；扩容失败 → `EMFILE` 且原表不变（强异常保证）。
4. `read/write/lseek` 直通对应 server 端点；libc 不做缓冲（缓冲属 stdio）。
5. `fcntl(F_GETFL/F_SETFL)` 的 flags 存 libc；`F_DUPFD` 走 dup 路径。
6. fd 号从 0 起，`STDIN/STDOUT/STDERR` 在 `_start` 由 itss 注入；缺失则 libc 自行 open `/dev/null`（POSIX 要求，V1 做，成本低）。

**异常路径**：`EBADF`（槽空）、`ECAPSTALE`（内核侧已回收，libc 转 `EBADF` 并 log 原始码）、`EQUOTA`（扩不动）。

**并发/死锁**：**全局锁顺序 `dtable_lock > malloc_lock > ldso_lock > stdio_lock`**，任何反向嵌套 = PR 打回。写进 `docs/lock-order.md`（§21 D-05）。

### 5.8 M08 线程 / TLS / 同步

- V1：单线程假实现（`pthread_create` → `ENOSYS` + 登记 `PARTIAL`），但 **TLS、`__errno_location`、TCB、`atfork` 注册表必须先落地**（后续真多线程不能改 ABI）。
- P1 真多线程：`pthread_create → sys_thread_create`（栈从内核 loan 2 页起步，`GUARD` 页由 `sys_protect` 设 `PROT_NONE`）；`futex → sys_futex`；退出走 `__pthread_exit` + `TLS_Dtor`。
- `pthread_atfork`：三个 handler 表；`fork` 后子进程**必须**按 `stdio → malloc → ldso → dtable` 逆序 reset。

### 5.9 M09 日志 / 崩溃捕获 / 恢复

- `rux_log(level, fmt)`：libc 侧无缓冲，直接 `sys_ipc_notify` 到 log server；`sys_log_write` 仅作 boot 期后门。
- 崩溃：内核捕获 → 发给 itss → itss 转 crash server → 写 `CrashInfo` + 可选寄存器转储 → 按重启策略拉起。
- 服务器重启策略：`rootfs/itss/log` = 立即重启（≤5 次/10s），失败则降级到"只允许串口交互"。
- 客户端侧：`EIPCCLOSED` → 库层自动重建 session（一次），仍失败 → 上抛 `EHOSTDOWN`。

### 5.10 M10 Rust core/std 移植

**target JSON**（`targets/x86_64-ruxos-none.json`）关键字段：
```json
{ "os": "ruxos", "env": "gnu", "abi": "elf",
  "dynamic-linking": true, "has-rpath": true, "has-thread-local": false→true(P1),
  "pre-link-args": {"ld.lld": ["-znoexecstack"]},
  "panic-strategy": "unwind", "target-feature": "-redzone" }
```
**阶段**
- R1：`core` + `-Zbuild-std`，`panic=abort`，静态 hello（不依赖 libc）
- R2：`std` 后端 `sys/pal/ruxos`（抄 `pal/unix`，把 `libc::` 换成 `ruxos-sys`），线程走 `ENOSYS` 桩
- R3：线程/futex 接通，开 `has-thread-local`
- R4：`std` 测试矩阵（§7.3 T-RS-02）+ unwinding

**约束**：`std` 只允许调用 libc 公开符号（不许偷 `GLIBC_PRIVATE`），否则 sysroot 与 std 会互相锁死版本。

### 5.11 M11 fork / exec / 进程生命周期（`RED-2` 修复）

**`fork()`（V1 语义定稿）**

```
单线程调用者：
  1. sys_task_create(as_child)                      → Task
  2. sys_as_derive(src=as, AS_DERIVE_COW)           → 子 AS（页表结构复制 + 写时复制标记）
  3. dtable 快照复制（libc 侧，含 flags/offset/cookie）
  4. sys_thread_create(栈=复制, ctx=与父同 rip, prio 继承) → 子首线程
  5. 返回子 pid（父）/ 0（子）；子进程入口处跑 pthread_atfork 子处理器（§5.8 逆序 reset）
多线程调用者：V1 返回 EAGAIN 并设 errno（**不是 ENOSYS**，程序可据 EAGAIN 走 posix_spawn）
```
> 完整多线程 `fork`（clone 全部线程 + 停止/恢复）属 P2，因为它要求内核 `Thread` 快照语义，风险远超收益。`posix_spawn` 反而是 V1 的主力路径。

**`vfork()`**：V1 用 `posix_spawn` 实现替代，`vfork` 符号存在但 `EUNIMPLEMENTED`（避免"半实现的 vfork"造成的稀有死锁）。

**`execve()`**
```
libc: 打开新 ELF（rootfs）→ itss:proc_exec(path, argv, envp, CloExecFdList)
itss: 新建 Task/AS → 校验 NT_RUXOS_ABI → 通过 ld.so 加载（或在 itss 内代跑 loader）
      → 注入栈(argc/argv/envp/auxv) → 创建首线程 → sys_task_start
      → 旧 Task 终止（继承 pid 由 itss 决定并保证 pid 复用安全：gen 计数）
CLOEXEC 处理在 libc 侧完成遍历（§5.7 规则 1），itss 只接收"要保留的 fd 列表 + 对应 cap"
```
**失败路径**：ELF 校验失败 → `ENOEXEC`；解释器缺失（`#!`）→ itss 二次尝试 `ld.so`/`/bin/sh`（V1 支持一层，嵌套 shebang 拒绝并 `ELOOP`）；ABI mismatch → `EABIMISMATCH`。

**`wait4()`**：itss 维护退出状态队列（`code`, `signo`, `ru`）；被等待者不存在 → `ECHILD`；`WNOHANG` 走 `sys_ipc_nbcall`。

**pid/tid 唯一性**：`pid = (as_idx, gen)` 的低 32 位投影，`gen` 保证 pid 复用不会被误投递信号（`ENOSIGNALSRC`）。

### 5.12 M12 tty / console 通路（`RED-7` 修复）

| 阶段 | 通路 | 说明 |
|---|---|---|
| boot 期（root server 之前） | `sys_console_write`(0x0A4) | 仅内核 + root server 可用；`console_direct` 位控制 |
| V1 运行期 | itss 下挂 **tty server**（`fd 0/1/2` = tty 端点） | `write()` → `ipc_call(tty, M_WRITE)` |
| 行编辑 / termios | V1 提供 `cfmakeraw` + 最小 `tcgetattr/tcsetattr`（`ISIG/ICANON/ECHO` 三位生效，其余 no-op + PARTIAL） | 完整 termios 属 P2 |
| 前台进程组 / `^C` | tty server 检测 `ISIG` → `sys_signal_send(fg_pgid 首线程, SIGINT)` | V1 支持，否则 `Ctrl-C` 无法退出 |

`printf` 路径：`printf → vfprintf → fputs → write(fd=1) → dtable → ipc_call(tty)`。**stdio 缓冲在 V1 对 `fd 1/2` 默认行缓冲、对文件全缓冲**（`_IOLBF/_IOFBF`），`stderr` 无缓冲 —— 这条必须写死，否则崩溃现场日志丢失。



---

## 6. 性能预算与瓶颈分析（逐环节）

### 6.1 时延预算表

> **所有目标数字标注 `PROVISIONAL`**：M2（W3）用 `perf-nop` 实测替换，之后每里程碑复测。
> 数字按 `(env=qemu-9.2.1, cpu=max, smp=4, host=<记录>)` 三元组分别存入 `bench/baseline.json`，**禁止跨环境互比**（Y-1）。

| 操作 | 预算（QEMU max，PROVISIONAL） | 真机目标 | 测量用例 |
|---|---|---|---|
| `sys_nop` | ≤ 120 ns | ≤ 60 ns | `perf-nop` |
| 常规 syscall（含 errno 处理） | ≤ 350 ns | ≤ 180 ns | `perf-call` |
| `ipc_call` 小消息（40B，无直接 IPC） | ≤ 600 ns | ≤ 350 ns | `perf-ipc-rtt` |
| `ipc_call` 直接 IPC 快路径 | ≤ 250 ns | ≤ 150 ns | `perf-ipc-direct` |
| `page_loan` 单页（往返合计） | ≤ 2 µs | ≤ 1.2 µs | `perf-loan` |
| `read()` 命中 rootfs 缓存 | ≤ 1.5 µs | ≤ 900 ns | `perf-read` |
| `read()` 下探驱动 | ≤ 4 µs + 驱动耗时 | — | `perf-read-drv` |
| `pthread_mutex` uncontended | ≤ 25 ns | ≤ 12 ns | `perf-lock` |
| `write()` 到 tty（行缓冲触发） | ≤ 1.2 µs | ≤ 700 ns | `perf-tty` |

**若 M2 实测显示 QEMU 下小消息 IPC 往返 > 800 ns**：判定为内核侧问题（多半是 cap 校验或 TCB 拷贝路径），**基础库侧不得用"增加缓存"来掩盖**，必须回到内核修（D-09 依赖内核排期的原因）。

**回归阈值**：任一用例劣化 >15% → CI 红，且必须给出归因（不允许"先记 TODO"）。

### 6.2 关键瓶颈：一次 POSIX `read()` 要几次陷入？

微内核的典型死法就是"一次 read 三次陷入"。逐环节分析：

| 环节 | 陷入数 | 优化手段 | 是否 V1 做 |
|---|---|---|---|
| 1. fd→cap 查表 | 0 | libc 内存表 | ✅ |
| 2. VFS 权限/名字解析 | 1 | **`fd` 直接绑定到 rootfs 端点，跳过 VFS 中间跳**（解析只在 `open` 时） | ✅ |
| 3. rootfs→驱动（块设备） | 1 | rootfs 侧缓存 + 批量提交（一次 IPC 发 N 个 bio） | 缓存 ✅ / 批量 P1 |
| 4. 结果回传 | 0 拷贝 | `page_loan` 而非 `buf` 拷贝（>1 页时） | ✅ |
| **合计** | V1 目标：**≤2 次陷入** | | |

**结论**：`open` 可以慢（1~3 次），`read/write` 必须 ≤2 次。此结论驱动 §5.7 的"fd 直绑端点"设计（不是纯分层洁癖）。

### 6.3 三条强制优化（P0）

1. **直接 IPC（direct call）**：`label.direct=1` 且接收方就绪时，内核把发送方 TCB 直接改写为接收方继续执行，省一次调度往返。降级透明。
2. **page loan**：跨页数据借出而非拷贝，借出未还内核禁止调度该线程离开（`EPAGENEEDBACK` 是内核不变量违规，出现即 bug 报告级别）。
3. **batched IPC**：`call_batch` 合并同端点请求，一次陷入换 N 次往返。

### 6.4 吞吐/并发承载（OS 语境换算）

| 维度 | V1 承载 | 依据 |
|---|---|---|
| 并发线程 | 256（cap 表 1024 槽起） | 单 AS cap 槽翻倍上限 65536 |
| IPC 吞吐 | ≥ 3M msg/s（小消息，双核 ping-pong） | 250ns 往返 → 单流 4M/s，留 25% 余量 |
| 每服务 session | 128 | itss 表大小 |
| log server | 1M rec/s 内存态，磁盘态 ≤ 50k rec/s | 单点，见 §14.5 |

无"日活/QPS"式估算 —— OS 基座库的容量模型是**每核 IPC 速率 + 每 AS 槽位数 + 内存预算**，三者全部由 §7.3/§6.1 微基准实测得出并写进 `bench/baseline.json`。

---

## 7. 测试策略

### 7.1 测试基础设施验证（第一天，红线）

开发第一周必须先跑通：
1. 宿主 `cargo test -p ipc --features fake-backend`（不依赖内核）
2. QEMU `make run-test TEST=hello-serial`（裸桩 → 串口）
3. CI 能同时跑这两条并产出 `result.xml`

跑不通就不许写功能代码。

### 7.2 fake backend（本册最有价值的测试杠杆）

`ipc`/`libsys-rs` 全部通过 `trait Backend { fn call(...) }` 访问内核。宿主实现 `FakeKernel`（模拟 cap 表、端点队列、超时、错误注入）。收益：§5.2/§5.7/§5.9 的**逻辑**测试全能在宿主跑，迭代快 100 倍；真机只测时序与 ABI 布局。

### 7.3 测试矩阵

| ID | 模块 | 用例 | 环境 | 通过标准 |
|---|---|---|---|---|
| T-ABI-01 | M01 | 重生成零 diff | 宿主 CI | diff 为空 |
| T-ABI-02 | M01 | `static_assert` 尺寸全对 | C+Rust | 编译通过 |
| T-ABI-03 | M01 | 号冲突/位域超限拒绝 | 宿主 | 生成失败且报号 |
| T-ABI-04 | M01 | 生成 C 头被 musl 编译 | 交叉 | 零 warning（`-Werror`） |
| T-STUB-01 | M04 | 每桩寄存器破坏集校验 | QEMU | 调用前后 callee-saved 不变 |
| T-STUB-02 | M04 | DF 标志归零 | QEMU | `std` 前置 `cld` |
| T-STUB-03 | M04 | errno 线程局部独立 | QEMU(2线程) | 不串扰 |
| T-IPC-01 | M02 | call/reply 往返 | 两者 | fake 过 + 真机过 |
| T-IPC-02 | M02 | 超时/无接收方/关闭 | QEMU | 错误码精确 |
| T-IPC-03 | M02 | epoch 变更重建 | 注入重启 | 仅 1 次重试 |
| T-IPC-04 | M02 | `EIPCDEADLK` 不重试 | QEMU | log 出现且无循环 |
| T-IPC-05 | M02 | batch 语义等价 | 两者 | 与逐次调用结果同 |
| T-IPC-06 | M02 | mr>40 拒绝 | 宿主 | debug 断言/release EINVAL |
| T-MUSL-01 | M05 | 5 个替换点无重复定义 | 交叉 | 链接无 `multiple definition` |
| T-MUSL-02 | M05 | musl `make check` 子集 | QEMU | ≥ 上次基线 |
| T-LDSO-01 | M06 | 12 种畸形 ELF 安全拒绝 | QEMU | 无崩溃、错误码正确 |
| T-LDSO-02 | M06 | 同程序 static/dynamic 行为一致 | QEMU | 输出逐字节同 |
| T-FD-01 | M07 | dup/close/retain 引用平衡 | 两者 | 无 `ECAPSTALE`/无泄漏 |
| T-FD-02 | M07 | 扩容失败原子性 | 注入 | 原表完整 |
| T-FD-03 | M07 | CLOEXEC exec 关闭集正确 | QEMU | 与 `compat-state` 表一致 |
| T-FD-04 | M07 | 锁顺序静态检查 | CI 脚本 | 无 `malloc→dtable` 嵌套 |
| T-THR-01 | M08 | TLS/errno 早期可用 | QEMU | `_start` 前不触 malloc |
| T-LOG-01 | M09 | 服务器崩溃→重启→session 重建 | 注入 kill | ≤ 100 ms 恢复 |
| T-RS-01 | M10 | `-Zbuild-std` 出 core hello | QEMU | 串口输出 |
| T-RS-02 | M10 | std 测试矩阵 | QEMU | ≥60%（记录趋势） |
| T-RS-03 | M10 | panic+unwind 可用 | QEMU | 栈回溯正确 |
| T-FORK-01 | M11 | 单线程 `fork`+子进程 `printf`+`wait4` 状态正确 | QEMU | 输出与状态逐次一致，pid 复用不误投递 |
| T-FORK-02 | M11 | 多线程 `fork` → `EAGAIN` | QEMU(P1) | 错误码精确，不死锁 |
| T-EXEC-01 | M11 | `execve` 后 CLOEXEC 关闭集 = 预期表 | QEMU | 与 `compat-state.md` 一致 |
| T-EXEC-02 | M11 | 一层 shebang 可用、嵌套拒绝 | QEMU | `ELOOP` |
| T-SIG-01 | §4.5.1 | `sigaction`+`kill`+`sigreturn` 往返 | QEMU | 寄存器恢复逐字节一致 |
| T-SIG-02 | §4.5.1 | `abort` → `SIGABRT` → itss 终止 | QEMU | 退出状态含 signo |
| T-SIG-03 | §4.5.1 | 非法 `krn_cookie` 的 `sigreturn` 被拒 | QEMU | `EPERM`，不劫持控制流 |
| T-TTY-01 | M12 | `printf` 行缓冲 / `stderr` 无缓冲 | QEMU | 崩溃前最后一行必出现 |
| T-TTY-02 | M12 | `^C` → `SIGINT` 到前台进程 | QEMU | `Ctrl-C` 生效 |
| T-TIME-01 | §3.4 | `EACCESS` vs `ECAPBAD` vs 成功路径时延 | QEMU | 三者中位数差 < 5% |
| T-CAP-01 | §3.1 | 派生 rights 取交集，逐位断言 | 两者 | 16 位全枚举通过 |
| T-STK-01 | §9 | 栈溢出注入命中 guard 页 | QEMU | `SIGSEGV`，非静默 |
| T-HANDLE-01 | §3.2 | 1e6 次 create/destroy，`gen` 唯一 | QEMU | 无 `ECAPSTALE`，无槽泄漏 |
| T-PERF-* | §6 | 微基准 | QEMU | 见回归阈值 |

### 7.4 交叉 ABI 测试 harness（必需）

`tests/abicross/`：只用**裸 syscall** 写的微型 C 程序 + 只用 `libsys-rs` 的 Rust 程序，互相发 IPC。目的：**绕开 libc 验证 libc**，避免"libc 的 bug 掩盖 ABI 的 bug"。

### 7.5 崩溃注入 / 压力

`stress-{ipc,fd,mem,session}`：10 万次循环 + 随机 kill 服务器 + 随机 fd 关闭。通过标准：无 `ECAPSTALE`（除非预期）、无内存增长（RSS 曲线斜率 < 阈值）。

---

## 8. 构建系统与 sysroot

### 8.1 三阶段

```
stage0  host tools          : gen-abi, xtask, fake kernel
stage1  裸 sysroot          : crt1.o crti.o crtn.o libsys-rs libc.a(最小桩+string) 
          → 产出 hello-serial, 桩测试可跑
stage2  musl 完整           : libc.a + libc.so + ld.so + include/
          → busybox 探针、自家 C 程序
stage3  工具链侧            : Rust target + core + std + libm + (P1) libc++
```

### 8.2 关键命令

```bash
make gen-abi                     # 重生成，CI 用 --check
make stage1 && make run-test TEST=hello-serial
make sysroot                     # 全量，输出 build/sysroot/{include,lib}
make bench                       # 微基准 → bench/baseline.json 对比
make probe-busybox               # 兼容性探针（非承诺）
cargo +nightly-2026-09-15 build --target targets/x86_64-ruxos-none.json -Zbuild-std=core
```

### 8.3 交叉工具链参数（冻结）

```
clang  --target=x86_64-ruxos --sysroot=build/sysroot \
       -mno-red-zone -fsigned-char -fPIC -fvisibility=hidden \
       -ftls-model=initial-exec -Werror
ld     --version-script=libc.map -z noexecstack -z relro
```
`.cargo/config.toml` 指定 `linker = "clang"` + `rustflags` 同源，禁止个人本地差异（CI 校验 `--print cfg` 输出）。

---

## 9. 安全设计（具体措施，不写口号）

| 维度 | 措施 | 验证 |
|---|---|---|
| 内存安全 | `abi` 全类型 `Pod/Zeroable`（`zerocopy`），禁止 `transmute`；unsafe 集中于 4 个边界模块且每处 `// SAFETY:` | `cargo clippy -D warnings` + 手写 lint 检查 `transmute` |
| 栈保护 | libc 开 `-fstack-protector-strong`；guard 页由 `sys_protect(PROT_NONE)` | `T-STK-01` 溢出注入须 SIGSEGV 而非静默 |
| RELRO/NX/ASLR | `-z relro -z now`（V1 全量 RELRO，不用 lazy RELRO）；内核 `EFNNX` 强制；ASLR 基址来自 `AT_RANDOM`，V1 只随 **PIE** | `readelf -l` 校验 |
| 能力最小化 | 派生时 rights 取交集；默认无 `CTRL/SIGNAL` | `T-CAP-01` 逐位断言 |
| 时序/信息泄露 | 校验失败路径恒定长度；不返回对象类型给无权限方（§3.4） | 时延测试 `perf-cap-probe` |
| 句柄卫生 | `gen` 代数 + `ECAPSTALE` 计数导出到 `/crash` | `T-HANDLE-01` 循环 create/destroy 1e6 次 gen 唯一 |
| 输入校验 | `ld.so` 对 ELF 做 25 项完整性检查（段重叠、越界 `p_filesz`、`PT_INTERP` 长度、`DT_*` 环） | `T-LDSO-01` |
| 日志脱敏 | log server 只落 `(seq, level, module, code)`，**禁止**落用户缓冲区内容 | lint 检查 + 代码审查项 |
| 供应链 | musl/qemu 版本锁定 + SHA256 校验；vendor 内网镜像 | CI `sha256sum -c` |

---

## 10. 目录结构（文件级）

```
ruxos/
├── AGENTS.md                     # 编码工具入口（技术栈/命令/禁止事项/红线摘要）
├── docs/
│   ├── ruxos-baselib-spec-v1.md  # 本文档
│   ├── compat-state.md           # §4.12 兼容性表（CI 强制）
│   ├── symbol-trace.md           # §4.13 符号↔syscall 追溯（生成物）
│   ├── lock-order.md             # §5.7 锁顺序
│   ├── abi-changelog.md          # 号段/字段追加记录
│   └── walkthrough.md            # 走查 + 扫雷记录（§24/§25 附录 B/C）
├── abi/
│   ├── idl/{syscall,label,err,struct,obj,signal,pool,trace}.idl
│   ├── src/{lib,syscall,types,msg,obj}.rs   # GENERATED
│   └── tests/{layout,roundtrip}.rs
├── gen-abi/src/{main,emit_rust,emit_c,emit_doc,error}.rs
├── crates/
│   ├── ipc/src/{lib,router,marshal,wait,batch,fake}.rs
│   ├── libsys-rs/src/{lib,asm,ipc,cap}.rs
│   └── server/src/{lib,loop,captable,session,crash}.rs
├── libc/
│   ├── musl/                     # vendor 快照 1.2.5（submodule）
│   ├── ruxos/
│   │   ├── syscall_arch.h  errno.c  ldso/dynlink.c
│   │   ├── thread/{pthread,tls,futex}.c
│   │   └── backend/{vfs,proc,net}_client.c
│   ├── crt/{crt1.S,Scrt1.S,crti.S,crtn.S,init.c}
│   ├── stubs/{regular.S.tpl,hot.S.tpl,gen.rs}
│   ├── libc.map  Makefile
├── targets/x86_64-ruxos-none.json
├── rust/                         # std 移植 overlay
│   ├── pal/{mod,init,fs,thread,futex,time,env,net,mm}.rs
│   └── unwinding/README.md
├── tests/
│   ├── abicross/{c_main.c,rust_main.rs,build.rs}
│   ├── qemu/{hello-serial,ipc-*,fd-*,ldso-*,stress-*}
│   └── host/{ipc.rs,fd.rs,ldso_logic.rs}
├── bench/{nop,ipc_rt,ipc_direct,loan,read,lock}.c  baseline.json
├── scripts/{run-qemu.sh,gdb.sh,sha256-check.sh,lint-lock.sh,gen-stubs.sh}
├── xtask/src/{main.rs,gen_abi.rs,sysroot.rs,test.rs,bench.rs,probe.rs}
├── build/sysroot/{include/ruxos/*.h,lib/{crt1.o,libc.a,libc.so,ld-ruxos.so}}
└── .github/workflows/ci.yml
```

---

## 11. 验收标准（V1 出口）

| # | 标准 | 判据 |
|---|---|---|
| 1 | ABI 单一真相 | `gen-abi --check` 零 diff，且 `T-ABI-01..04` 全绿 |
| 2 | 桩正确性 | `T-STUB-01..03` 全绿；callee-saved 校验脚本进 CI |
| 3 | IPC 语义 | `T-IPC-01..06` 全绿；直接 IPC 实测 ≤250ns |
| 4 | 自家 C 程序 | sysroot 下 `clang` 编出的 hello（`printf`→tty）、fd 测试、`fork+exec` 往返、`abort` 信号路径 全部通过；busybox 静态探针仅作信息（判据 #4 的可测量口径 = 上述 4 个自研用例，**不是 busybox**） |
| 5 | 动态链接 | 同程序 static/dynamic 输出逐字节一致（`T-LDSO-02`），且 `NT_RUXOS_ABI` mismatch 用例确实拒绝加载 |
| 6 | 崩溃隔离 | kill 任一 server，其余存活且 `≤100ms` 重建 session；`EACCES`/`ECAPBAD` 时延差 < 5% （`T-TIME-01`） |
| 7 | Rust 目标 | `core` hello + `std` ≥60% 通过 + `T-RS-03` unwinding 可用 |
| 8 | 性能 | `bench/baseline.json` 全部落在 §6.1 预算内（PROVISIONAL 数字在 M2 后转为实测目标） |
| 9 | 兼容状态透明 | `compat-state.md` 覆盖 100% 公开符号，无空说明；**`symbol-trace.md` 覆盖 100% 公开符号且引用号全部存在**（§4.13） |
| 10 | 文档可复现 | 新环境按 §8.2 命令能在一台干净机器上复现 sysroot |

---

## 12. 开发红线（写代码前必读）

1. `__libc_start_main` 之前：禁 malloc、禁 IPC、禁非 `sys_*` 外部调用。
2. 锁顺序固定 `dtable > malloc > ldso > stdio`，反向嵌套 = 打回。
3. syscall 号/结构体布局/persona 枚举 只追加，空洞不复用。
4. 手写跨语言结构体、手写桩 = 打回。
5. 服务器默认不链接 libc。
6. `errno` 不得用 `__thread`（启动期不可用），必须 `fs:` 偏移。
7. 不承诺的符号必须 `UNTEST` 记录，禁止默默可用（`libm` 类"存在但不承诺"同样适用，§13）。
8. 任何性能用例劣化 >15% 且无归因 = 阻塞合入。
9. `page_loan` 未归还（`EPAGENEEDBACK`）视为内核级 bug，立即停线排查，不做 workaround。
10. CI 的 ABI 检查与 `trace-check`（§4.13）不因"改动很小"跳过。
11. **W^X 强制**：`PROT_WRITE|PROT_EXEC` 一律 `EACCESS`（§4.14 / D-18），libc 不得提供绕过入口。
12. **新增公开符号必须先有 trace 行**：`symbol-trace.md` 中无对应 syscall 号 → 不许进 `libc.map`。
13. **禁止"尽量跑"**：ABI mismatch 一律终止加载（§4.15），不允许警告后继续。

---

## 13. API 清单（公开符号子集，完整表见生成物 `abi.h`）

桩层（常规）：`read write open openat close lseek pread pwrite mmap munmap mprotect madvise getpid gettid fork vfork execve exit wait4 nanosleep clock_gettime getrusage fcntl dup dup2 dup3 pipe pipe2 socket send recv poll select unlink rename mkdir rmdir getdents chdir getcwd`

桩层（热路径专用，仅自家 Rust/C 高精度用）：`__ruxos_call_label(label, mr[5]) __ruxos_page_loan __ruxos_decant`

libc 高层：`malloc free calloc realloc posix_memalign printf fprintf sprintf snprintf vprintf fputs fputc fgetc fgets fopen fclose fflush fseek ftell strerror errno errno_location strlen strcpy strncpy strcmp strncmp strchr strrchr strstr memcpy memmove memset memcmp atoi atol strtol strtoll qsort bsearch getenv setenv abort atexit on_exit`

线程（P1）：`pthread_create join detach mutex_* cond_* rwlock_* key_* tls_* sem_* barrier_*`

Rust：`std::{io,fs,net,os,thread,time,env}` 表面 + `ruxos_sys::ipc`

统计口径（`RED-4` 修复 —— 区分"承诺"与"存在"）：

| 口径 | V1 数量 | 说明 |
|---|---|---|
| **承诺符号**（进 `compat-state.md` 且有测试） | ≈ 200（P0）+ 120（P1） | §11 判据 #9 以此为准 |
| **存在符号**（musl 编译进来但未测试） | ≈ 1800 | 一律标 `UNTEST`，**禁止对外宣称可用** |
| `libm` | 全量 `libm.a`（静态，≈ 400 符号） | V1 **不计入承诺**，链接可得但 `UNTEST`；不进 §4.9 版本脚本的 `RUX_1.0` global 段（避免与 libc 符号表混淆） |

> 决策要点：**"存在"不等于"承诺"**。libm/libc++ 这类"搬进来就能链接"的库，如果混进承诺表，V1 出口判据会瞬间失真（要么虚高要么全红）。因此 libm 走独立归档 + `UNTEST` 默认标注，P2 再按探针结果逐批升级。

---

## 14. 架构韧性与降级（微内核版）

### 14.1 单点识别

| 组件 | 是否单点 | 后果 | V2 消除 |
|---|---|---|---|
| 内核 | 是 | 全系统崩溃 | 无法消除，靠 §9 正确性 + 崩溃转储 |
| root server | 是（启动期） | 无法建 session | 启动后即交出能力，运行期不在关键路径 |
| log server | 是 | 日志丢失，不影响执行 | 环形缓冲 + 串口兜底（V1 已做） |
| itss | 是 | 不能 fork/exec，存量进程可跑 | 双 itss 热备（V2） |
| rootfs | 是 | 文件 IO 阻塞 | 客户端只读缓存（V2） |
| net server | 否（多实例可） | 网络中断 | V1 单实例 |

### 14.2 故障影响矩阵（组件故障 → 功能影响）

| 故障 | shell 交互 | 文件 IO | 网络 | 新进程 | 存量线程 |
|---|---|---|---|---|---|
| rootfs 挂 | 降级（tty 走 itss） | **不可用→`EHOSTDOWN`** | 不受影响 | 若 exec 依赖 rootfs 则失败 | 正常 |
| itss 挂 | 无法起新 shell | 正常 | 正常 | **失败** | 正常 |
| log 挂 | 正常 | 正常 | 正常 | 正常 | 正常（仅丢日志） |
| net 挂 | 正常 | 正常 | **不可用** | 正常 | 正常 |
| 某驱动挂 | 正常 | 该设备 `EHOSTDOWN` | 若同栈则不可用 | 正常 | 正常 |

### 14.3 恢复目标

| 对象 | RTO | RPO |
|---|---|---|
| log server | 5 s（重启） | 允许丢内存外全部（≤1s 窗口） |
| rootfs | 100 ms（session 重建，客户端重试 1 次） | 写回缓存丢失 = 数据丢失 → **必须 fsync 语义正确** |
| itss | 1 s | 存量线程状态由内核保持，不丢 |
| 内核崩溃 | 人工重启（无热备） | 全丢 → 因此 V1 内核改动必须过 `stress-*` |

### 14.4 熔断 / 背压

- 客户端对同一 server：**连续 5 次 `EIPCCLOSED` → 打开熔断 100 ms**，期间调用直接失败（`EHOSTDOWN`）不再打 IPC。
- server 侧过载：`EQUOTA` 作为背压信号，libc 不自动退避（避免语义突变），由 `ipc` crate 提供 `RetryPolicy::backoff()` 供应用显式选择。
- 禁止在任何库层对 `EIPCDEADLK`/`EQUOTA` 做静默重试。

### 14.5 降级层级

```
L0 正常
L1 log server 故障 → 串口直写（丢弃结构化）
L2 rootfs 故障 → 只读缓存 + 报 ENOENT 于写路径
L3 itss 故障 → 存量进程可用，禁止起新进程
L4 内核 panic → 崩溃转储 + 重启
```

### 14.6 兜底清单

| 依赖失败 | 兜底 |
|---|---|
| ABI 版本不匹配 | 拒绝加载 + 打印期望/实际（绝不"尽量跑"） |
| 未实现符号 | `ENOSYS` + `compat-state.md` 行号提示 |
| 无熵 | `sys_getrandom` 返回 `ENOSYS`，libc 不返回 0（防静默弱随机） |
| 端点缓存耗尽 | LRU 降级为每次 resolve |
| 栈溢出 | guard 页 SIGSEGV，不自动扩 |

---

## 15. 上游依赖风险清单

| 依赖 | 版本 | 风险 | 缓解 | 备选 |
|---|---|---|---|---|
| musl | 1.2.5 | 上游演进与 patch series 冲突；5 个替换点被上游改动 | submodule + patch 重放脚本 + `T-MUSL-02` 基线 | 自研最小 libc（已评估为 V2 备选） |
| rustc nightly | `2026-09-15` 钉死 | `-Zbuild-std`/`pal` 内部 API 变动破坏 std overlay | 锁 nightly；`T-RS-01..03` 每日跑 | 提交 stage0 上游补丁 / vendor fork `library/std` |
| limine | 9.2.x | boot 结构变更 | 固定 release + SHA256 | GRUB2 multiboot2 |
| QEMU | 9.2.1 | `-cpu max` 行为变化致时延基线漂移 | `bench/baseline.json` 分环境记录；CI 固定镜像 | 真机基准（V2） |
| clang | 19.1.7 | 自定义 triple 默认 spec 缺失 | 全部 `-target` 参数显式化，不靠默认 | 自编 clang 配置 |
| LLVM libunwind | 19.x | 与 `dl_iterate_phdr` 协作缺失 | R4 前保持 `panic=abort` 可用 | 手写 unwind（成本高，不推荐） |

**冷启动/配额类风险在 OS 场景不适用**；对应风险是"CI 里 QEMU 跑不满并发导致单测 25min→超时"，已用"宿主 fake-backend 优先 + QEMU 分片 3 路并行"缓解。

---

## 16. 成本（人力/机器，非云服务）

| 项 | 估算 | 说明 |
|---|---|---|
| 人力 V1（P0+P1） | 约 **26 人周** | abi+生成器 3，ipc 3，桩/crt 2，musl 边界 5，ld.so 4，fd/TLS 3，**信号 2**，**trace/compat 工具 1**，Rust 移植 4，测试基建 3（重叠计）；v1.1 因信号提前 +1 人周缓冲 |
| 日历工期 | **22 周**（§19） | 按 1.2 FTE 并行（M1/M3/M4 三线）折算；单人做则 ~26 周 |
| 单机开发 | 16 核 / 64 GB | QEMU 分片 + 交叉编译 |
| CI | 自建 runner ×3（16 核） | 全量矩阵 ≤25 min |
| 真机基准 | 1 台 x86_64 迷你主机 | 性能预算校准（V1 末） |
| 外部服务费用 | 0 | 无第三方 SaaS |

---

## 17. CI/CD

```
lint → gen-abi --check → trace-check(§4.13) → host tests(fake) → stage1 build
     → qemu tests(shard×3) → bench(vs baseline，>15% 红) → compat-state 覆盖率 → probe-busybox(信息性)
```
- PR：`lint + gen-abi + trace-check + host tests + stage1 + qemu shard 1/3`（目标 ≤ 8 min）
- merge 主干：全量 qemu 3 分片 + bench（≤ 25 min）
- 夜间（Y-6）：`stress-{ipc,fd,mem,session}` 12 h + musl `make check` 全量 + probe-busybox 趋势报告
- 产物：`sysroot.tar.zst` + SHA256 + `bench/report.json` + `compat-state.md` 差异 + `symbol-trace.md`
- **`stress-*` 不进门禁**（否则 25 min 预算必然爆），但夜间失败 = 次日首优先修复。
- 单测分片说明：QEMU 启动约 3 s/用例，250 用例分 3 片才压得进 25 min；新增用例 >30 个时必须同时提分片调整 PR。

---

## 18. 运维/排查手册（开发者向）

| 现象 | 首查 | 命令 |
|---|---|---|
| 桩返回 `-1` 但 errno 不对 | 是否踩到内核扩展码 | `sys_get_errno_raw` / `ruxos_errno()` |
| 随机 SIGSEGV | TLS/IFUNC/ldso 加载顺序 | `T-LDSO-02`，`readelf -l` |
| 句柄莫名失效 | `ECAPSTALE` → gen 复用 | `T-HANDLE-01`、cap 表 dump（`sys_cap_lookup`） |
| 性能突然劣化 | 直接 IPC 降级 | `perf-ipc-direct` + `sys_features` |
| 死锁 | 锁顺序 | `scripts/lint-lock.sh` + `EIPCDEADLK` 日志 |
| 编译期符号找不到 | 版本脚本漏放 | `libc.map` diff + `nm -D` |
| 崩溃后不恢复 | 重启预算耗尽 | crash server 的 `crash_seq` |

`scripts/gdb.sh` 提供：内核 + 用户态双进程 gdb（QEMU `-s -S` + `target remote :1234`），并带 `rux-cap`/`rux-msg` 自定义 gdb 命令解析 label 位域。

---

## 19. 里程碑（按周）

| 周 | 里程碑 | 交付物 | 出口判据 |
|---|---|---|---|
| W1–2 | M1 ABI 基座 | IDL + gen-abi + C 头 + 布局断言 | `T-ABI-01..04` 绿 |
| W2–3 | M2 裸通路 | 桩模板 + crt1 + `libsys-rs` + hello-serial | `T-STUB-*` 绿，`sys_nop` ≤120ns |
| W3–5 | M3 IPC 通路 | `ipc` crate + fake backend + root server 客户端 | `T-IPC-01..06` 绿 |
| W5–8 | M4 libc 主体 | musl 5 替换点 + 最小 stdio + dtable | 自家 C 程序 + busybox 静态探针（信息性） |
| W8–11 | M5 动态链接 | `ld.so` 最小闭环 + `dl_iterate_phdr` | `T-LDSO-*` 绿 |
| W11–14 | M6 线程/TLS/**基础信号** | 真多线程 + futex + atfork reset + `sigaction/kill/sigreturn/abort` | `T-THR-*`、`T-SIG-01..03`、无泄漏 |
| W14–17 | M7 Rust 基座 | target JSON + core + std overlay | `T-RS-01/02` |
| W17–19 | M8 韧性 | crash 恢复 + 熔断 + tty 前台进程组 | `T-LOG-01` ≤100ms、`T-TTY-02` |
| W19–22 | M9 出口 | 全量矩阵 + 性能达标 + 文档 | §11 十条全绿 |

**M 与 P0 的一致性说明**：基础信号（§4.5.1）已从 M8 提前到 M6，因为 `abort`/断言是 stdio 与测试 harness 的地基 —— 晚做会导致 M4/M5 的所有失败用例只能靠退出码判断（走查 YELLOW 转 RED 的根因之一）。

**推荐开发模式**：M1/M3/M4 可三线并行（跨依赖只在 ABI 表，冻结后无冲突）；M5/M6 必须串行（ld.so 与 TLS 互相牵制）；单 Agent 负责 §4 ABI 变更裁决，避免多线同时改号段。

---

## 20. 配置与开关

| 变量 | 默认 | 说明 |
|---|---|---|
| `RUX_ABI_CHECK` | `1` | 启动握手，设 `0` 仅供测试（永不用于发布默认） |
| `RUX_IPC_DIRECT` | `on` | 关→可对比时延（`perf-ipc-*`） |
| `RUX_LOCK_DEBUG` | `off` | 开→锁顺序断言 + 持锁计时 |
| `RUX_KERR_VERBOSE` | `off` | 开→扩展错误码原样上抛（跳过有损映射） |
| `RUX_LD_DEBUG` | `off` | ld.so 搜索/重定位日志 |
| `RUX_FD_MAX` | `65536` | dtable 上限 |
| `RUX_STACK_SIZE` | `8 MiB` | 主线程栈（内核 loan） |
| `QEMU_SMP` | `4` | 测试环境差异记录进 `bench/baseline.json` |

Feature flag：`compat.linux_persona`（默认禁用，V2 开启）、`libc.wide_char`（P2 禁用）。

---

## 21. 待决策表（RED = 开发前必须拍板）

> **阶段五扫雷后的状态**：`walkthrough.md`（同目录） 列出的 7 项 RED（RED-1..7）**已通过本册 §4.13/4.14/4.15/4.5.1/5.11/5.12 的修订回写闭环**，其中涉及产品取向的 4 项（D-13..D-16）仍需你确认。

| ID | 议题 | 推荐 | 阻塞级别 | 状态 |
|---|---|---|---|---|
| D-01 | 项目代号（当前占位 `ruxos`） | 由你定；若保留 `ruxos`，triple=`x86_64-ruxos-none` | 🔴 发布前必须，编码期可替换 | 待你给名 |
| D-02 | `futex`/`wait_multi` 是否内核化 | **是**（纯用户态会让 uncontended 之外的锁全崩） | 🔴 影响 M6 计划 | 待确认 |
| D-03 | `O_CLOEXEC` 归属 | libc（本文档已按此设计） | 🟡 确认即可 | 已按推荐写死 |
| D-04 | `page_loan` 粒度与上限 | 1–256 页，单条消息 1 次借出 | 🟡 | 待确认 |
| D-05 | 锁顺序文档落点 | `docs/lock-order.md` + CI lint | 🟡 | 已写入 |
| D-06 | 错误码数值是否复用 Linux errno | **复用** `-1..-132`（降低 musl 适配成本） | 🔴 | 已按推荐写死 |
| D-07 | 有损 errno 映射表的维护方 | libc 单点 `kerr_map.c` + `compat-state.md` 标注 | 🟡 | 已写入 |
| D-08 | `dl_iterate_phdr` 是否提前 P0 | **是**（unwinding 前置） | 🟡 | 已升 P0 |
| D-09 | 直接 IPC 快路径是否 V1 交付 | **是**（否则时延预算不成立） | 🔴 依赖内核排期 | **跨册依赖，需内核侧承诺** |
| D-10 | Rust 服务器是否允许链接 libc | 默认不允许，白名单制 | 🟡 | 已写入红线 |
| D-11 | busybox 探针是否进 CI 门禁 | 不进（K-3 不承诺），仅信息 + 趋势 | 🟡 | 已写入 §17 |
| D-12 | 静态 PIE 是否 V1 | P1（`Scrt1` 已预留） | 🟢 | — |
| **D-13** | **多线程 `fork` V1 返回 `EAGAIN`（不支持）** | 推荐接受：程序改用 `posix_spawn`；代价是个别老程序报错退出 | 🔴 需你确认取舍 | 新增（RED-2） |
| **D-14** | `vfork` 定为 `EUNIMPLEMENTED` | 推荐接受（半实现比不实现更危险） | 🟡 | 新增 |
| **D-15** | `mlock/munlock` 定为 no-op 成功 | 推荐接受（返回 `ENOSYS` 会让数据库类程序拒绝启动） | 🟡 | 新增（RED-1） |
| **D-16** | `libm` 全量存在但不计入承诺符号 | 推荐接受 | 🟡 | 新增（RED-4） |
| **D-17** | `MAP_PRIVATE` 写时复制 是否 V1 | 推荐 P1（V1 只承诺只读文件映射），影响 `exec` 大程序内存占用 | 🟡 | 新增（RED-5） |
| **D-18** | W^X 强制（`PROT_WRITE` + `PROT_EXEC` 组合拒绝） | **是**（V1 就开，别留到后面改不动） | 🔴 | 新增（§4.14） |

---

## 22. 上线（发布）检查清单 —— V1 出口逐项签字

> 对应模板的"上线准备审查"。OS 基座库没有"部署"，但有**冻结点**：一旦对外发 sysroot，ABI 即不可回退。

**基础设施确认**
- [ ] `make gen-abi --check` 在干净 clone 上零 diff
- [ ] `symbol-trace.md` 覆盖 100% `libc.map` global 符号，无空引用号
- [ ] `compat-state.md` 无空"已知偏差"栏
- [ ] sysroot 产物 SHA256 已记录并同步到内网镜像
- [ ] `bench/baseline.json` 含 `(qemu / qemu-cpu / hostcpu)` 三元组，PROVISIONAL 标注已全部转实测

**依赖预热（OS 语境 = 版本冻结与可复现性）**
- [ ] musl submodule commit + patch series 可重放（`make reproduce-musl`）
- [ ] nightly 日期钉死，`-Zbuild-std` 在 CI 镜像内可离线跑
- [ ] QEMU/limine 二进制 SHA256 校验通过
- [ ] `NT_RUXOS_ABI` 的 `ABI_VERSION` 已定为发布值并写入 `abi-changelog.md`

**灰度策略（替代"灰度发布"）**
- [ ] 阶段 1：内部自研程序（3 个）用新 sysroot 重编 + 全量测试
- [ ] 阶段 2：busybox 探针集（信息性，观察 `compat-state` 偏差是否扩大）
- [ ] 阶段 3：对外发 sysroot tarball，**同号段一年内不复用**承诺生效

**出口 Smoke Test（人工，5 分钟）**
- [ ] `hello`（C，动态）打印到 tty
- [ ] `ls`（自研静态）
- [ ] `fork + exec + wait4` 往返 1000 次无泄漏
- [ ] `kill -9` 一个 server → 其余存活
- [ ] Rust `core` hello

**快速回滚触发条件**
- 出现任一 `EPAGENEEDBACK`（内核不变量违规）→ 立即停发
- 承诺符号测试通过率相比上一发布下降 >3% → 停发
- `bench` 关键路径劣化 >15% 且无法归因 → 停发

**人工应急（无技术降级时的兜底）**
- 已发布 sysroot 发现 ABI 破坏：走 `abi-changelog.md` 的 **major bump + 旧号保留返回 `ENOSYS`** 路径。**本册不提供 ABI 回退能力**，只有向前兼容 —— 因此 §4 的冻结必须是慢决策。
- 内核侧 ABI 承诺未到位（尤其 D-09 直接 IPC）：延后 M5/M6，**不降 ABI 质量**，并同步通知所有下游服务器开发者。
- musl 上游出现安全更新：由 libc-maint 评估是否重放 patch series，**禁止直接 bump musl 版本**（会击穿 §11 判据 #9）。

## 23. 附录 A：V2 规划（明确不做）

Linux 二进制 persona 与 netlink/procfs/sysfs 伪实现；POSIX 覆盖率与 LTP/PISA；aarch64/riscv64；locale + 宽字符；`dlopen` 高级语义与 `dlmopen`；IFUNC/`DT_RELR`；形式化验证与内核 spec；多 itss 热备；rootfs 只读缓存与批量 bio；完整多线程 `fork`（§5.11）。

## 24. 附录 B：走查记录

见 `walkthrough.md`（同目录） §1（四条走查路径：正常加载链 / 关键异常 / 权限边界 / 韧性降级）。

## 25. 附录 C：扫雷审查结果

见 `walkthrough.md`（同目录） §2（RED/YELLOW/GREEN 三级清单；7 项 RED 已回写至本册 §4.13/4.14/4.15/4.5.1/5.11/5.12 与 §21）。

## 26. 附录 D：变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-10-11 | 初版：K-1/K-2/K-3 锁定；§4 ABI 冻结；§21 待决策 12 项 |
| v1.1 | 2026-10-11 | 阶段四走查 + 阶段五扫雷：补 §4.13 追溯表 / §4.14 mmap+页池 / §4.15 ABI 握手；新增 `sys_brk`/`sys_as_derive`/`sys_pool_ctrl`/信号四调用/`sys_console_write`/`sys_crash_ack`；新增 §5.11 fork-exec、§5.12 tty；信号与 fork 升 P0 并调整 M6/M8；§6.1 标注 PROVISIONAL；§13 区分承诺/存在符号；§17 stress 移夜间；§21 增至 18 项待决策 |

---

**AI 辅助生成说明**：本文档由 AI 依据已确认决策整理，`§4` 的号表/位域、`§5.5` 替换点、`§21` 推荐值均为**提案**，需内核侧评审确认后方可作为实现约束。

---

内容由 AI 生成
