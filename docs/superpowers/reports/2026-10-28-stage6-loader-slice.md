---
id: report-stage6-loader-slice
kind: report
authority: scoped-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-10-28
---

# 阶段 6 准备性切片：静态 ELF 装载一致性与初始 auxv

## 结论与范围

本文记录阶段 6 **机械可达部分**的一次实现：把内核静态 ELF 装载器从"只接受本仓库自建
映像的页对齐形态"扩展到 ELF 规范要求的形态，并把 exec 的初始 `auxv` 收敛成单一可测
构造器。两条改动都有 host 单测，第三条有真实未修改静态二进制的装载演练。

- **完成的**：段对齐按 `p_vaddr ≡ p_offset (mod 4096)` 判定（映射从向下对齐页开始）、
  `PT_LOAD` 上限 4→8 且溢出确定性失败、W+X 段与可执行栈确定性拒绝、`PT_GNU_RELRO`
  显式归类、exec 初始栈从 `auxv` 内联字面量改为 `static_auxv`/`build_initial_stack`
  并补齐 `AT_EXECFN` 等条目。
- **未完成的**：阶段 6 的**全部**退出条件仍然未勾选。本次切片不产生 Linux 系统调用
  递送（S6-M1）、不产生 LibOS profile（S6-M3）、不产生与参考 Linux 的差分（S6-M5），
  因此没有任何未修改的 Linux 静态二进制在本系统中**运行**过。剩余工作逐项见
  [未完成清单](#未完成清单阶段-6-剩余工作)。
- **性质**：技术证据，非签署。阶段 6 状态仍为"未开始"（D10），交付检查与退出条件
  一律不勾选。

## 基线与输入

| 项 | 值 |
|---|---|
| 基线 commit | `1e924f5`（阶段 6–7 细化提案提交）；本次改动（装载器、初始栈、`build.py` 的 locale 修复）随本报告同批提交 |
| 工具链 | `nightly-2026-07-30`（`rust-toolchain.toml`），targets `x86_64-unknown-none`/`uefi` |
| 上位契约 | [白皮书 §15](../../../Prima%20OS.md)、[阶段跟踪](../../../Prima%20OS%20实现阶段.md)、[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)、[静态程序契约](../specs/2026-10-26-static-program-contract-design.md) |
| 未修改二进制样本 | Ubuntu `busybox-static` `/usr/bin/busybox`，BusyBox v1.36.1 (Ubuntu 1:1.36.1-6ubuntu3.1)，2 124 608 B，`sha256=dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14` |
| 样本位置 | 宿主 `/usr/bin/busybox`，**不入库**；演练由环境变量 `PRIMA_ELF_SAMPLE` 触发，默认 `#[ignore]` |

样本的程序头（`readelf -lW`，节选）是本切片的事实来源：

```text
Type         Offset   VirtAddr          FileSiz  MemSiz   Flg  Align
LOAD         0x000000 0x0000000000400000 0x006c8  0x006c8  R    0x1000
LOAD         0x001000 0x0000000000401000 0x1a7865 0x1a7865 R E  0x1000
LOAD         0x1a9000 0x00000000005a9000 0x0521e5 0x0521e5 R    0x1000
LOAD         0x1fb820 0x00000000005fc820 0x009568 0x0108e8 RW   0x1000
TLS          0x1fb820 0x00000000005fc820 0x000028 0x000082 R    0x8
GNU_STACK    0x000000 0x0000000000000000 0x000000 0x000000 RW   0x10
GNU_RELRO    0x1fb820 0x00000000005fc820 0x0077e0 0x0077e0 R    0x1
```

两个关键事实：第 4 个 `PT_LOAD` 的 `p_vaddr=0x5fc820`/`p_offset=0x1fb820`
**不同时页对齐但同余**（同为 `0x820`）；样本只有 **4 个** `PT_LOAD`，也没有 `PT_PHDR`。

## 实现内容

改动集中在三个文件，均属 OS 中立机制，没有引入任何 Linux 语义、错误码或 syscall 号。

### 1. 段对齐：从"必须页对齐"改为"必须同余"

`kernel/src/exec.rs`。旧实现要求每个 `PT_LOAD` 的 `p_vaddr` 页对齐：

```text
if vaddr & 0xfff != 0 || memsz == 0 { return Err(Misaligned) }
```

对上面的样本，第二个条件不成立但第一个成立（`0x5fc820 & 0xfff == 0x820`），因此旧
解析器对真实静态 busybox 返回 `Misaligned`。ELF 规范与 Linux 装载器只要求
`p_vaddr ≡ p_offset (mod 页大小)`，映射从 `p_vaddr` 向下对齐的页开始。

新表示把"段的 VA"与"映射的起点"分开，两者都由解析器计算，调用方不再自行取整：

| 字段 | 含义 |
|---|---|
| `vaddr` | `p_vaddr`，段首字节的 VA（不要求页对齐） |
| `map_start` | `p_vaddr` 向下对齐到页 = 映射第一页的 VA |
| `offset` | `p_offset`，段首字节的文件偏移 |
| `file_base` | `p_offset` 向下对齐到页 = 映射首页对应的文件偏移 |
| `lead()` / `file_bytes()` / `pages()` / `map_end()` | 首页内偏移、首页文件字节数、映射页数、页对齐结束 VA |

装载路径（`kernel/src/syscall.rs` 的 `prepare_exec_image`、`kernel/src/bootstrap.rs`
的 `commit_exec`）改为按 `map_start`/`pages()` 建映射、从 `image + file_base` 起复制
`file_bytes()` 字节。首页包含段首之前的同页文件字节，这与 Linux 映射整页的行为一致，
同时保证 `AT_PHDR` 指向真实的程序头表而不是零填充。

### 2. 装载上限与负例

- `MAX_EXEC_SEGMENTS` 4→8。样本只需 4 段（实测），余量用于"代码/只读/可重定位只读/
  数据/TLS/对齐填充"变体；超限仍确定性返回 `ExecError::TooManySegments` → `ENOEXEC`。
- 新增 `ExecError::WritableExecutable`：同时带 `PF_W|PF_X` 的 `PT_LOAD` 在解析期拒绝。
- 新增 `ExecError::ExecutableStack`：`PT_GNU_STACK` 带 `PF_X` 时拒绝；缺省该头不拒绝。
- `PT_GNU_RELRO` 显式归类并**不执行**只读化：静态自举重定位（如 `IRELATIVE`）要求该
  区域装载时可写，只读化属于装载后的 `mprotect` 语义。这是记录在案的缺口，不是遗漏。
- `p_memsz < p_filesz` 与 `p_memsz == 0` 统一为 `BadProgramHeaders`。

容量影响（`std::mem::size_of` 实测）：`ExecSegment` 40→56 B，`ExecPlan` 192→480 B。
`ExecPlan` 只在 `exec_call` 的单个栈帧内构造并按引用下传，相对 32 KiB syscall 栈仍
有两个数量级余量；这一条是**观察值**，不是新的容量契约。

### 3. exec 初始 auxv：单一构造器、补齐 `AT_EXECFN`

旧实现把 9 对键值内联在 `prepare_exec_stack` 的调用参数里，且缺 `AT_EXECFN`：内核
exec 路径与阶段 5 M12 冻结的 C 探针路径（[8 对](../reports/2026-10-22-stage5-m12-c-startup.md)）
已经**漂移**。新实现把集合收到 `exec.rs` 的一处：

- `static_auxv(plan)` 返回 13 对：`AT_PAGESZ`(4096)、`AT_PHDR`、`AT_PHENT`(56)、
  `AT_PHNUM`、`AT_BASE`(0)、`AT_ENTRY`、`AT_FLAGS`(0)、`AT_SECURE`(0)、
  `AT_UID`/`AT_EUID`/`AT_GID`/`AT_EGID`(0)、`AT_CLKTCK`(100)；
- `build_initial_stack(top, plan, argv, envp, buffer)` 再加 `AT_HWCAP`(0) 与
  `AT_EXECFN`（**指向栈上的 `argv[0]`**，在字符串地址确定后追加）；
- `build_stack` 保持为底层布局原语，行为不变（仅多一个内部 `execfn` 开关）。

明确不提供，理由是硬约束而非省事：`AT_RANDOM`（项目无安全熵，D13 未裁决）、
`AT_PLATFORM`（需额外栈上字符串）、`AT_SYSINFO_EHDR`（无 vDSO）。`AT_HWCAP=0` 表示
"不宣告任何可选 CPU 特性"，libc 仍须用 `cpuid` 判定。

单测覆盖：同余但非页对齐的段（含 `map_start`/`file_base`/`lead`/`pages` 断言）、
不同余拒绝、W+X 拒绝、可执行栈拒绝、段上限与溢出、页级重叠（页内偏移不同的两段落在
同一页）拒绝、坏头拒绝、初始栈 15 对键值（含 `AT_EXECFN` 指向 `argv[0]`、
`AT_RANDOM` 必须缺省）。

### 4. 验证环境：musl 归档审计的 locale 依赖（构建工具，不碰 OS 语义）

本次切片在**默认 zh_CN.UTF-8** 宿主上首次运行 `cargo xtask test` 时，36 个分组**全部**
在同一前置失败：`ERROR musl: unexpected raw syscall boundary in musl: {'': [58, 48]}`。
这不是本切片的回归，而是[独立质量审查](2026-10-27-implementation-quality-review.md)
已复现并定级的 P0：`prima-user/musl/build.py` 用英文正则 `":     file format "` 解析
`objdump -d` 的归档成员边界，中文 locale 下成员头是 `在归档文件 … 中：`／`文件格式`，
成员名解析为空，审计断言随即失败。后果是**任何非英文 locale 的宿主机都无法产生 QEMU
验收证据**。

修复（`prima-user/musl/build.py`，仅构建工具，不碰被审计的 musl 源）：所有 `output()`
子进程统一注入 `LC_ALL=C`/`LANG=C`，使 binutils/git 的可解析输出与宿主 locale 无关；
`build.py` 的改动会改变缓存戳（`cache_key` 含本文件字节），因此会触发一次完整重建。

复核（默认 locale）：`python3 prima-user/musl/build.py` → `PASS musl static sysroot`；
`python3 prima-user/musl/build.py --check` → `cache=hit`（命中时会跑归档审计）。
对照：同一宿主上未经 env 覆盖的 `objdump -d` 输出为 `在归档文件 … 中：`，即缺陷输入。

## 命令与结果

| 命令/输入 | 本次实跑 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test -p kernel --lib exec::` | 本次 | PASS，11 passed（1 ignored 为样本演练） | host 单测 |
| `PRIMA_ELF_SAMPLE=/usr/bin/busybox cargo test -p kernel --lib exec::tests::plan_accepts_operator_supplied_static_linux_elf -- --ignored` | 本次 | PASS：2 124 608 B 的静态 busybox 装载计划成立，`AT_PHDR≠0`、入口落在可执行段、每段同余成立 | host 单测 + 宿主二进制 |
| `cargo test --workspace` | 本次 | PASS：kernel lib 491 passed/1 ignored，其余 crate 全绿（167/43/42/39/17/5/3/2/1/…） | host 全门禁 |
| `cargo xtask lint` | 本次 | PASS：fmt、clippy `-D warnings`、`x86_64-unknown-none` 裸机构建（`deny(unsafe_code)`）、`kernel/src` 零 `unsafe`（32 文件） | host 门禁 |
| `python3 prima-user/musl/build.py` / `--check` | 本次 | PASS（默认 `zh_CN.UTF-8` locale）；`--check` 命中缓存时执行归档审计并通过 | host 构建门禁 |
| `cargo xtask test` | 本次 | **PASS：`full BIOS suite: 36 programs, 72 runs` + `PASS stage1 boot slice`**，默认 locale，无 `LC_ALL` 覆盖 | QEMU 执行 |

说明：`cargo xtask test` 只证明**既有**阶段 5 探针（含 `exec`/`execve` 与 R1/R2/R3）
在改动后仍全部通过，其中 exec 探针走的正是被改动的 `SYS_EXEC` 装载与初始栈路径。它不
证明任何未修改 Linux 二进制可运行——那需要 S6-M1 与 S6-M3。

首次以默认 locale 运行该套件时，36 个分组全部在 musl 审计处失败（见
[4. 验证环境](#4-验证环境musl-归档审计的-locale-依赖构建工具不碰-os-语义)）。修复
`build.py` 后重跑，得到上表的 PASS；这次失败与修复都保留在本文，不用后一次 PASS 改写。

## 失败、反例与限制

- **反例（本切片要修的那个）**：真实静态 busybox 在旧解析器下得到 `Misaligned`
  （第 4 个 `PT_LOAD` 的 `p_vaddr` 非页对齐）。修法不是放宽检查，而是把检查改成规范
  要求的形式；本仓库自建映像全部页对齐，因此该扩展不放宽任何既有约束。
- **解析通过 ≠ 可运行**：本次没有任何 Linux 二进制在本系统执行。缺的是 S6-M1 的
  同步请求/回复递送与 S6-M3 的 LibOS profile，两者都不在本切片内。
- **映像上限（三道，全部未解决）**：① `exec_call` 要求 `image_len ≤ 1 MiB`，样本为
  2.03 MiB；② 装载是"从请求者地址空间整块 copy-in"，未修改二进制需要流式/按页装载；
  ③ 单次 exec 的映像帧上限是 `MAX_SEGMENT_FRAMES = 149`（约 596 KiB），样本按
  `PT_LOAD` 计算需 `1 + 424 + 82 + 18 = 525` 页，因此即使前两条放宽，样本仍会在
  `allocate_exec_segment` 得到 `ENOSPC`。这三条是 S6-M2 的实质剩余，不是参数微调。
- **初始栈只有 8 页（32 KiB）**：对探针够用，对真实 libc 启动与 `busybox` 的多层
  调用偏紧，属 S6-M2 的剩余参数。
- **W^X 只在装载期检查**：`EFER.NXE` 未启用，用户页当前全部可执行，因此拒绝 W+X 段
  只是"确定性拒绝坏映像"，不是运行期写执行隔离。NX 需要页属性机制改动，未做。
- **RELRO 仍可写**：只读化需要装载后的 `mprotect` 语义与 LibOS 协作，未做。
- **两个 auxv 构造器仍在**：内核 exec 路径与 `prima-servicemgr` 的 C 装载路径各有一套。
  本切片通过收敛到 `static_auxv` 消除了内核侧的漂移，但两路径合并属 S6-M2/S6-M6。
- **不提供熵**：`AT_RANDOM` 缺省，SSP/canary 在 Prima 构建中关闭；这是既有的书面
  弱化，D13 仍待裁决。

## 未完成清单（阶段 6 剩余工作）

阶段 6 的定义与退出条件见[阶段跟踪 §阶段 6](../../../Prima%20OS%20实现阶段.md)，
工作项拆分见[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)。下表是**当前**
状态，勾选状态以阶段跟踪为准（本切片不勾选任何一条）。

### 交付检查（S6-D1–D5）

| ID | 剩余工作 | 当前状态 | 阻塞/前置 | 下一步 |
|---|---|---|---|---|
| S6-D1 | 静态 ELF 装载器 + Linux 初始栈 | **部分**：段一致性、负例、`auxv` 集合与 `AT_EXECFN` 已完成；缺流式装载、映像上限、栈规模、`AT_RANDOM` | D13（熵策略） | 按页装载与更大初始栈；D13 裁决后补 `AT_RANDOM` |
| S6-D2 | OS 中立的 syscall/exception 递送 | **未开始**（这是阶段 6 的真实难点） | 无（但需架构评审） | 见 S6-M1 |
| S6-D3 | 版本化的 Linux LibOS syscall profile | 未开始 | D12（白名单）、S6-M1 | 先定白名单再定 profile 行 |
| S6-D4 | profile 必需的 futex/TLS/进程环境 | 未开始；内核有 `SYS_TASK_SET_FS_BASE`/`SYS_WAIT_WORD` 等原语，缺 Linux 细节 | S6-M1、S6-M3 | `CLONE_*`、`set_tid_address`、`sigaltstack`、`wait4`/rusage |
| S6-D5 | 未修改静态 ELF 白名单与测试镜像 | 未开始（本次只有本地样本演练，未入库） | D12 | 定成员与 sha256，入库最小镜像 |

### 退出条件（S6-X1–X4）

| ID | 剩余工作 | 当前状态 |
|---|---|---|
| S6-X1 | 白名单静态 ELF 无需修改即可运行 | 未满足：无 LibOS，无运行见证，白名单未定义（D12） |
| S6-X2 | 支持的 syscall 与参考 Linux 差分通过 | 未满足：差分脚手架只有 `cow-write-fault` 一个 tracepoint |
| S6-X3 | HexArc/COSIR 不含 Linux 专属对象或 syscall 语义 | **目前成立但无门禁**：内核仍是 OS 中立，本次改动未新增 Linux 语义；需要自动化守卫防回归 |
| S6-X4 | 未支持 syscall 确定性且可审计地失败 | 部分：内核未登记号确定性 `ENOSYS`；"可审计"属于 LibOS 侧，未实现 |

### 工作项（S6-M1–M6）

| ID | 剩余工作 | 估算（人周） | 依赖 | 说明 |
|---|---|---|---|---|
| S6-M1 | 中立递送：同步 call/reply、`syscall_event` 能力、回复一次性、中断/取消、延迟口径 | 3–5 | — | 全阶段最易低估；单核 QEMU 见证是阶段 6 的停止条件 |
| S6-M2 | 未修改静态 ELF 加载（本次已推进，未完成） | 1–2 剩余 | S6-M1 | ①`image_len ≤ 1 MiB` ②整块 copy-in ③`MAX_SEGMENT_FRAMES=149`（样本需 525 页）④8 页初始栈 ⑤两路径合并 ⑥`AT_RANDOM`（D13） |
| S6-M3 | Linux LibOS profile v1（逐 syscall 行为与 errno） | 6–10 | S6-M1、S6-M2（D12） | 长尾项 |
| S6-M4 | Linux 进程/线程/信号语义 | 4–6 | S6-M1、S6-M3 | pthread、fork+exec、`EINTR` |
| S6-M5 | 差分与验收工程（需与 M3/M4 并行） | 2–3 | S6-M2–M4 | 逐 syscall 差分矩阵、分组并行、确定性 trace |
| S6-M6 | 收口与签署包（profile 冻结、证据索引） | 1–2 | S6-M1–M5 | 组织级评审 |

### 需要裁决（不是实现工作）

| ID | 问题 | 阻塞 |
|---|---|---|
| D12 | 白名单成员与版本（`busybox` 6 子命令 + 2 探针 + sha256？） | S6-X1、S6-M3 |
| D13 | 无熵条件下 `AT_RANDOM`/canary 策略与书面弱化 | S6-M2、S6-M4 |
| D14/D15/D16 | 多 LibOS 前移、SMP 状态分区、阶段 7 子轨拆分 | 阶段 7 计划 |

**结论性判断**：阶段 6 剩余工作里，"装载器与初始栈"这一类机械项已经基本清空；剩下的
18–29 人周估算几乎全部落在 S6-M1（递送机制）与 S6-M3（profile 行为）上。若 D12–D16
不裁决，即使继续做装载侧硬化也不会缩短阶段 6，因此不建议在没有 D12/D13 的情况下继续
扩展本切片。

## 阶段/profile 影响

- 阶段 6 状态不变：仍为"未开始"（D10），交付检查与退出条件**一条未勾选**。
- profile v2 与 SPEC-152 不受影响：冻结的 `P5-BOOT-01` 描述的是 C 探针装载路径
  （`prima-servicemgr`），本次改的是内核 `SYS_EXEC` 路径的初始栈；两者并列存在，
  漂移已在[限制](#失败反例与限制)中记录。
- 与旧文档的偏差（记录而非静默改写）：`2026-10-23-stage5-m17c2-exec-syscall.md`
  设计把 auxv 定为 `AT_PAGESZ`/`AT_ENTRY`/`AT_NULL` 最小集，
  `2026-10-23-stage5-m17c3-exec.md` 报告记录实现为 `AT_PAGESZ`/`AT_ENTRY`（后扩到 9 对）。
  本文把内核 exec 路径改为 15 对（含 `AT_EXECFN`）。两份文档保持其写作时的范围与状态
  （前者标注"设计（未实现）"），本文是其后的实现事实；若组织级评审要求回到最小集，
  应改本文的 `static_auxv` 而不是改历史文档。
- 无新增/修改任何 syscall 号、错误码或能力；`ExecError` 的两个新变体仍统一映射到
  `ENOEXEC`，不暴露给用户态。

## 后继关系

- 上游：[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)（S6-M1–M6 拆分）、
  [静态程序契约](../specs/2026-10-26-static-program-contract-design.md)（persona 护栏）、
  [M17c3 exec 报告](2026-10-23-stage5-m17c3-exec.md)（内核 exec 的原始语义）。
- 后继：S6-M1 的递送机制单独成文并需架构评审；本次切片在 S6-M1 有结论后应按其
  请求/回复语义重新检查初始栈交接（`AT_EXECFN`/`AT_PHDR` 是否仍由内核构造，还是
  移交 LibOS）。若届时结论要求改由 LibOS 构造，本文第 3 节将被后继设计取代。
