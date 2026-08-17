# 页表/GDT/IDT/trap 框架设计（S1-D4，切片 B）

> 文档类型：真实内核实现设计（S1-D4）
>
> 日期：2026-08-17
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-08-17）

## 0. 定位与前置

垂直切片固定为 **COW/page fault → handler → `exc_resume` → TLB → Linux trace**。切片 A（S1-D3，物理内存发现）已实现并合并回 master（证据 SPEC-017）。本切片是真实内核链第二片，对应 tracker **S1-D4「初始页表、GDT/IDT 与 trap 框架」**。

核心目标：在已启用的物理内存直映射基础上，验证 bootloader 建好的页表可用、装载 GDT/TSS、装载 IDT（最小 trap 闭环）。**本切片为后续页表/trap 相关细化（S2 + COW 垂直切片后半段）铺路**，但它自身不实现完整异常分派、不进入用户态。

默认配置下 `physical_memory_offset = None`（无物理内存直映射）。切片 B 要读物理帧/页表，**必须先启用 `physical_memory` 映射**（B1 第一步）。

## 0.1 已确认的关键决策

| 决策 | 结论 |
|---|---|
| 切片粒度 | 拆四个子片 B1–B4，各自独立测试、独立合并回 master |
| 页表策略 | **沿用 bootloader 建好的 CR3 页表**，不自建、只读探测（translate） |
| 帧分配器 | **按需才引入**；B1 起启用直映射即可读写物理帧，无需立即实现分配器（YAGNI） |
| trap 深度 | 最小 trap 闭环：IDT + 一个测试向量 + asm 入口 + `iretq` 返回，不做完整异常分派/IRQ/PIC |

## 0.2 bootloader 交接关键事实（已核实 bootloader_api 0.11.17）

- `BootInfo.kernel_addr` / `kernel_len`：内核 ELF 物理范围（切片 A 已用）。
- `BootInfo.physical_memory_offset: Optional<u64>`：当 `Mappings.physical_memory = Some(Mapping)` 时，bootloader 把全部物理内存直映射到该虚拟偏移并回传；默认 `None`。
- `Bootinfo.memory_regions`：`Usable`/`Bootloader`/`UnknownUefi`/`UnknownBios`（切片 A 已用）。
- `BootloaderConfig.mappings.physical_memory`：启用物理直映射的开关（默认关闭）。
- bootloader 0.11.17 **无** `map_physical_memory`/`recursive` Cargo feature 标志；映射由 kernel 侧 `BootloaderConfig` 直接控制，无需改 Cargo.toml feature。
- bootloader 已加载内核栈（`kernel_stack_bottom`/`kernel_stack_len`，默认 80KiB 含 guard page），TSS 的 `rsp0` 可直接用它，无需分配器。

## 一、B1：物理内存直映射启用 + 探测

**目标**：建立"物理内存可读可写"能力——本切片 B 所有读物理帧/页表/写 GDT/IDT 的前提。

**Files（新增/修改）**：
- `kernel/src/main.rs`：`entry_point!` 改用自定义 `BootloaderConfig`；在直映射建立后调用探测。
- `kernel/src/config.rs`（新）：定义被 `entry_point!` 引用的 `BootloaderConfig`，启用 `mappings.physical_memory`。
- `kernel/src/mem.rs`：新增纯函数，用 `physical_memory_offset` 把物理地址换算成虚拟地址；新增探测函数（读物理字节）。

**架构**：
- `entry_point!` 的签名（bootloader_api 0.11.17 已核实）：`entry_point!(fn)` 或 `entry_point!(fn, config = &BOOTLOADER_CONFIG)`，其中 `BOOTLOADER_CONFIG` 为 `static BootloaderConfig`。本项目构造为：
  ```rust
  use bootloader_api::{BootloaderConfig, Mapping, entry_point};
  pub static BOOTLOADER_CONFIG: BootloaderConfig = {
      let mut config = BootloaderConfig::new_default();
      config.mappings.physical_memory = Some(Mapping::new_default()); // Dynamic，bootloader 选址
      config
  };
  entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);
  ```
  bootloader 据此把物理内存直映射进虚拟地址空间，并将偏移回传给 `BootInfo.physical_memory_offset`。
- `PhysicalVirtMapper::new(offset)`（或 `phys_to_virt(phys) -> VirtAddr`）纯函数：`offset + phys`。可 host 单测，给定 offset/phys 断言换算出正确 VA；不依赖真实物理合法性。
- 探测证明：取一个**已知存在且可安全读**的物理地址（如 `0x0` 处的 BIOS 数据区、或切片 A mem 清单第一个可用帧），经 `phys_to_virt` 读一个字节，断言 `offset <= v < offset + phys_memory_len`（映射范围）且读到稳定值。直映射本身由 bootloader 保证，探测用于证明内核侧换算与读取路径有效。

**错误分离**：若配置启用但 `physical_memory_offset` 仍为 `Optional::None`，属配置未生效——复用切片 A 的 `protocol::memory_error` + `qemu::exit(PANIC)` 路径（`MemError::ImageOverlapsUsable` 等已有枚举命名保持；直映射缺失走同一条 `memory_error` 输出 + `PANIC` 退出）。

**证据形态**：
- host 单测：`phys_to_virt` 边界（offset 0、非对齐 phys、接近上限）、`Optional` 解包失败路径。
- QEMU boot-trace：新增 `physical_map_ready` 事件（返回 `ok`，携带直映射偏移的校验），xtask 断言映射范围含一个已知物理地址。

## 二、B2：页表沿用 + page-walk translate

**目标**：证明 bootloader 的 CR3 页表有效且内核能读它——不修改、不自建，只读探测。

**架构**：
- `cr3()`：`asm!` 读 `cr3` 寄存器得到 `PhysAddr`（PML4 表物理地址）。读 cr3 无副作用，unlikely 出错点：仅一条 `mov`。放 `kernel/src/cpu.rs`（新）或紧跟 `mem.rs`。
- `translate(cr3_virt, vaddr)`：纯函数，沿 4 级页表（PML4→PDPT→PD→PT→phys）translate。输入为 PML4 的虚拟映射基址 + 目标虚拟地址，输出 `PhysAddr` 或 `TranslateError`（未映射/非 canonical）。页面大小 4KiB（本片只用 4KiB，不做 2MiB huge page 解析，遇不可解析位返回 not-mapped）。
- 探测目标：取一个已知符号（如 `.boot_info` 的虚拟地址）经 `translate` 得到物理地址，与已知值（bootloader 提供的 `BootInfo` 位置，或切片 A 的 `kernel_addr`）比对断言一致。
- 便利：bootloader 页表把内核放在高半区（`0xffff...`），虚拟地址经 `cr3` 页表 translate 到物理。内核当前以 `physical_memory` 直映射读物理，需把 cr3 的 `PhysAddr` 用 B1 的 `phys_to_virt` 转成虚拟再解引用——两个能力在此组合。

**关键点**：`translate` 是纯函数，可**完全 host 单测**——喂一个手工构造的 4 级页表数组（用 `#[repr(C)]` 的 `PageTable`/`PageTableEntry` 结构），验证 translate 正确解析、缺页/未映射/清零位返回错误。`PageTableEntry` 位布局是 x86-64 架构常量，可单测编码正确性。

**证据形态**：
- host 单测：`PageTableEntry` 位布局（present/rw 位）、`translate` 四级解析、未映射/非 canonical 错误、`phys_to_virt` 组合。
- QEMU boot-trace：新增 `page_table_ready` 事件，断言 translate 某个符号得到期望物理帧。

## 三、B3：GDT + 最小 TSS

**目标**：装载内核 GDT 段表 + 最小 TSS（为 B4 trap 提供合法段选择子与内核栈切换）。

**架构**：
- **GDT**：`#[repr(C, packed)]` GDT 项数组：
  - index 0：null（全零）。
  - index 1：内核代码段（base 0，limit 作为 64 位 code 描述符，present/user?=0, DPL=0, L=1, type=code）。
  - index 2：内核数据段。
  - index 3：（保留）TSS 段描述符占位（本片 B3 由一个 TSS 段描述符 + 装载）。
  - （用户段保留先不加——本片不进 ring3。）
- GDT 描述符 `#[repr(C, packed)]` 结构编码 x86-64 位布局：`[limit_low:u16, base_low:u16, base_mid:u8, access:u8, flags_limit_hi:u8, base_hi:u8]`。构造是纯函数可 host 单测。
- `load_gdt(&Gdt)`：`asm!` `lgdt` + reload `cs`/`ds`/`es`/`ss` 选择子。`lgdt` 用 `lgdt [ptr]`（`asm!` 内联，`.att_syntax`/`intel` 依项目习惯统一）。
- **TSS**：`#[repr(C)]` `TaskStateSegment`，仅设必需的 `rsp0`（= 内核栈顶 `kernel_stack_bottom + kernel_stack_len`）+ `iopb` 偏移。TSS 段描述符加入 GDT（`TSSDesc` 编码），用 `ltr` 装载。TSS 基址/物理可在 `.bss` 静态（bootloader 已映射内核 BSS）。

**关键点**：`lgdt`/`ltr` 是特权指令，需 `asm!` unsafe。段 reload（`cs` 用 far return/jmp 或 `retfq`）有细微 asm 细节，本片不进入用户态，仅切到内核段即可；或统一在 B3 用简单 `lgdt` + 段寄存器 `mov`（64 位下 `cs` 仍需 far jump 才可靠）。TSS `rsp0` 直接用 bootloader 内核栈，无需分配器。

**证据形态**：
- host 单测：GDT 描述符编码（access/flags/limit/base 位）、TSS 字段布局、GDT 数组条目重读一致。
- QEMU boot-trace：新增 `gdt_tss_ready` 事件，断言装载后继续正常运行（不崩溃）+ 预期反馈。

## 四、B4：IDT + 最小 trap 闭环

**目标**：定义 IDT、装载，触发一次软异常，走真实 trap 入口（保存现场 → Rust handler → `iretq` 恢复）验证完整闭环。

**架构**：
- **IDT**：`#[repr(C, packed)]` 的 `InterruptDescriptorTable`（256 项）。本片启用一个测试向量（breakpoint `0x3`，无 error code，trap 入口最简单）。其余向量置为 not-present（或统一 minimal stub）。描述符字段：`[offset_low:u16, selector:u16, ist:u8(reserve), type_attr:u8, offset_mid:u16, offset_hi:u32]`，可 host 单测编码。
- **Trap 入口**：`global_asm!` 一段。breakpoint 不压 error code，入口为：`push` 保存通用寄存器到栈上连续区域作 `InterruptContext`，`mov` 传参（context 指针）给 Rust handler（`extern "C"` 或项目约定 calling convention），`call`，从 context `pop` 恢复，`iretq`。field：RIP/CS/RFLAGS/RSP/SS 由 CPU 压入（`iretq` 用），通用寄存器由入口保存。
- **Handler**：Rust 函数读 `InterruptContext`；对 `0x3` 记录全局 `static TRAP_VECTOR_SEEN: AtomicU8`，对其余（未启用）走 panic。
- **触发证明**：`kernel_main` 末尾 `asm!("int3")` 触发 breakpoint → 真实 trap入口 → Rust handler 置标志 → `iretq` 返回 → 继续打印 `trap_ready`（携带 TRAP_VECTOR_SEEN 值）→ SUCCESS。

**关键点**：`global_asm!` + trap context 是切片最真实的部分。寄存器保存/恢复必须匹配 Rust calling convention（用 `extern "x86-interrupt"` 或显式 `asm!` 保存所有 caller-saved）。breakpoint 无 error code，规避 error-code 后的栈布局问题，是正确的最小起点。handler 幂等（触发计数原子递增）。

**证据形态**：
- host 单测：IDT 描述符编码、`InterruptContext` 布局（与 asm 保存顺序一致）、error-code 有无判定逻辑。
- QEMU boot-trace：`trap_ready` 事件 + `TRAP_VECTOR_SEEN == 1` 断言真实 trap→handler→返回发生且恰好一次。

## 五、跨子片共同约束

- **依赖与组织**：沿用切片 A 的 lib+bin 结构；GDT/TSS/IDT/页表均用 `#[repr(C)]` 自行编码，**不引入外部 `x86_64` crate**。`asm!`/`global_asm!` 用到 `unsafe`，但收敛到 `kernel/src/cpu.rs`、`kernel/src/gdt.rs`、`kernel/src/idt.rs` 等**细化层模块**（承接 G2「唯一允许 unsafe 的细化 crate + safe 机制 crate」方向）；纯逻辑（`phys_to_virt`、`translate`、描述符编码）可 host 单测。
- **不新增依赖**；`bootloader_api` 保持 `=0.11.17`。
- **验收**：每子片 `cargo fmt --all --check`、`cargo test -p kernel`、`cargo test -p xtask`、`cargo test -p cosir`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo xtask test`（六 PASS，QEMU 退出码 33）对齐 TEST-006。
- **证据边界**：本切片证明"真实内核建立了物理直映射、能读 CR3 页表、装载 GDT/TSS、装载 IDT 并完成一次 trap 闭环"。**不宣称**：自建页表、进入用户态 ring3、完整异常分派/IRQ/PIC、多核、`KERNEL-TRACE`（COW 端到端）、`REFINEMENT-PROOF`。

## 六、子片依赖与增量提交划分

```
B1 → B2 → B3 → B4
（直映射 / 页表读 / GDT+TSS / IDT+trap）
```

各子片独立提交独立合并；提交顺序即上表。每片完成 `cargo xtask test` 无回归。

## 七、证据边界（措辞纪律）

本切片仅证明上述四类真实内核能力。验收证据 = host 单测（真实内核 lib 的纯逻辑：换算、页表 translate、描述符编码）+ QEMU boot-trace 集成断言（`physical_map_ready`/`page_table_ready`/`gdt_tss_ready`/`trap_ready`）。不宣称：自建完整页表、ring3、完整异常分派（16 向量 + IRQ + PIC）、SMP、`KERNEL-TRACE`、`REFINEMENT-PROOF` 或机器证明（BLK-001 仍 Open）。
