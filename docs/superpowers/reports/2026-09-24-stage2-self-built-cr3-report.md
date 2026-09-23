# 切片 X 报告：自建内核 CR3 与镜像逐页映射（ISSUE-AUTH-001）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-24
>
> 对象：`kernel/src/{vspace,user,cow,resume,protocol,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 `ISSUE-AUTH-001` 的解除条件之一——**让 init 运行在自建页表上并逐页映射 image**，
> 并如实登记两条未解的规范偏差

## 0. 结论速览

- **自建 CR3 已落地**：内核在 boot 早期构造一次**自有 PML4 与全部中间页表**（`kernel_root`），
  显式复刻三层映射：内核镜像（**逐页**）、物理直映射（恒等区间）、启动栈（**逐页**）。
  所有任务根改由它克隆，故内核高半区**不再是 bootloader 页表的共享上级**。
- **判决性证据（新增独立事件 `self_root_ready`）**：
  `root=0x5b000;kernel_root=0xa000;image_table=0xb000;owned=1;pages=165;translated=0x1000000`。
  其中 `owned=1` 表示**运行中的地址空间**用来映射内核镜像的那张中间表帧，在帧注册表里的归属是
  **保留页表帧**（W6 第 3 类）——即镜像确由本内核自有且被审计的页表映射；`translated` 与
  `page_table_ready.translated` 构成跨事件不变式。
- **本切片暴露并修掉一个既有缺陷（反例保留）**：`vspace::count_missing` 每个**新建路径**多分配
  一张页表帧且从不归还（注释把它算成"3 个中间层 + 1 个叶 PT"，实为叶 PT 已含在 3 张内）。
  小规模映射下泄漏 2–3 帧不可见；逐 4 KiB 覆盖 128 MiB 直映射时泄漏 60+ 帧并撑爆
  `MAX_TABLE_FRAMES`。修正为 `3 - k` 并加 host 单测。
- **定位出一条必须登记的实现事实**：内核 ELF 是 PIE，bootloader 按程序头**逐段**装载，
  各段 `vaddr = file_offset + k*4096` 且 **k 逐段不同**——"整段镜像 VA→物理单一常量偏移"**不成立**，
  统一偏移在段边界之后读到错页（实测）。镜像页物理帧改为**逐页向源根解析**。
- **两条规范偏差照实登记、不宣告闭合**：(1) §14 `{R,X}` 与内核可写 `.data`/`.bss` **不可调和**
  （同一段 VA 不能既只读又可写，前置 3 排除别名映射），解它须做**链接脚本级段切分**，本仓库
  无链接脚本，故**不在本切片**；(2) §14 的 `image` 与实现的 init 可执行镜像语义分叉。
- **不勾选任何交付项与退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不是组织级第三方审查，不构成 `REFINEMENT-PROOF`。
- 调试手段记录（供复用）：QEMU `-d int,cpu_reset` 给出异常向量/错误码/RIP/CR（三重故障定位首选），
  `-d in_asm` 取出故障 RIP 处的**实际内存字节**与 ELF 反汇编对账；串口非阻塞 `out` 的末尾字符
  在 reset 时会丢，须轮询 LSR 且以异常日志为准。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `vspace.rs` | 新增 `SelfRootSpec`、`build_self_root`（逐页复刻镜像/直映射/启动栈，失败回滚本次表帧）、`SelfMap`、`leaf_phys_or_none`（支持 1 GiB/2 MiB 大页）；修正 `count_missing` 泄漏 |
| `user.rs` | 新增 `KERNEL_ROOT`/`KERNEL_IMAGE` 静态、`build_kernel_root`、`IMAGE_SLOT`、`SelfRootWitness`/`self_root_witness`；`build_task_root` 改为克隆自建根；`prepare`/`prepare_task2` 去掉 `root_phys` 参数 |
| `cow.rs` | 新增 `rebase_root`（把 COW 上下文的根改指自建根，使 `SYS_TASK_CREATE` 也克隆自建根） |
| `main.rs` | 在 E/F 见证与 `cow::prepare` **之后**、`user::prepare` **之前**构造自建根并 rebase |
| `resume.rs` | `after_resume` 采集并上报 `self_root_ready` |
| `protocol.rs` / `xtask/src/protocol.rs` | 新增事件与校验器（4 条反例）、`EXPECTED_EVENTS` 30→31、golden 夹具更新 |

### 2.1 镜像为什么必须逐页取物理帧

首版按 `phys = kernel_phys_start + (va - kernel_image_offset)` 复刻。实测：CR3 成功写入自建根后，
在 `cpu::write_cr3` 的 `pop rax`（ELF 偏移 `0x5e5a8`）处 #GP → #DF → 三重故障。
逐页对账（同一 VA，源根 vs 自建根）给出决定性证据：

```
vboot=0x105d000  byte_boot=0x58   ← bootloader 根：正确（pop rax 的机器码）
vself=0x105e000  byte_self=0xcc   ← 自建根：整整错一页
```

根因：ELF 是 PIE，4 个 LOAD 段的 `vaddr = file_offset + k*4096`，k 分别为 0/1/2/3，
故统一偏移从第 2 段起失配。修正为逐页解析源根（`leaf_phys_or_none`）。

### 2.2 为什么需要大页感知解析

`cpu::translate` 遇 `PS` 位直接返回 `PageNotMapped`，而 bootloader 根对直映射/启动栈用大页。
新增 `leaf_phys_or_none` 支持 1 GiB（PDPT）与 2 MiB（PD）大页，页内偏移取自 VA 低位。

## 3. 实测证据（2026-09-24）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30671;image=804;free=30671;regions=3;kstart=0x1000000;kend=0x1324000
PRIMA|v=1|seq=25|event=preempt_ready|status=ok|data=ticks=4;pre1=2;pre2=2;root1=0x53000;root2=0x5b000;cr31=0x53000;cr32=0x5b000;…
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31475;free=30561;segment=19;table=91;image=804;conserved=1;disjoint=1
PRIMA|v=1|seq=30|event=self_root_ready|status=ok|data=root=0x5b000;kernel_root=0xa000;image_table=0xb000;owned=1;pages=165;translated=0x1000000
PRIMA|v=1|seq=31|event=boot_complete|status=ok
```

守恒：`30561 + 19 + 91 + 804 = 31475 = universe` ✅；`conserved=1`、`disjoint=1`。
`segment=19` 与改前一致——本切片未改变平台 Segment 的取帧次数与顺序。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **288**、xtask **153** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**31 条事件**序列 + 各 data 校验器。
- `cargo xtask compat`：`PASS compat cow-write-fault`。
- 事件流逐条 diff（上一提交 `41eb511` vs 本切片）：改动面见设计文档 §4。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件 | 变化 | 原因 |
|---|---|---|
| `memory_ready` | `usable` 30693→30671、`image` 782→804、`kend` 0x130e000→**0x1324000** | 自建根与见证代码使镜像 +22 帧 |
| `bootstrap_ready` | `image_len` 3202664→3289112、`image_frames`→804、`free_frames`→30671 | 同上 |
| `cow_ready` | `shared` 0x6000→**0x5000**、`private` 0xb000→**0x9000** | **`count_missing` 泄漏修正**：此前每个新建路径少耗 1 帧，此点已少耗 2 帧，帧序整体前移 |
| `preempt_ready` | `root1/cr31` 0xc000→**0x53000**、`root2/cr32` 0x15000→**0x5b000**、`data1/data2` 0x1e000/0x22000→**0x63000/0x66000**、`rsp*`/`kstack*_top` 后移 | 任务根改为克隆**自建根**（且此前已多分配 ~74 张表帧）；内核栈随镜像后移 |
| `frames_ready` | `free` 30648→**30561**、`table` 26→**91**、`image`→804；`segment=19` 不变 | 自建根自有表帧 + 镜像增长；守恒式仍成立 |
| **新增** `self_root_ready` | seq=30 | 本切片判决性见证（新增独立事件，落在 `frames_ready` 之后） |
| `boot_complete` | seq 30→**31** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 24 条 | **逐字段不变** | 含 `page_table_ready.translated`、`vspace_ready`、`platform_segments_ready`、`authority_ready` |

以上按维护规则 5 记录，**不视为回归、也不隐藏**。

**容量登记**：`table` 由 26 升至 91（`MAX_TABLE_FRAMES = 128`，余量 37）。逐 4 KiB 覆盖
128 MiB 直映射约需 66 张。余量收紧是事实；后续若逼近上限，按项目纪律须扩展模型（如引入大页映射）
而非放宽阈值。

## 5. host 单测

见设计文档 §3：`vspace.rs` 5 条（含"镜像第二页必须用它自己的物理帧"这一判决性反例、
2 MiB 大页解析、`map_4k` 恰好分配 3 张表）；`xtask` 4 条反例（归属丢失、`translated` 不符、
`pages=0`、根帧未对齐）。

## 6. 边界与反例保留

- **§14 `{R,X}` 未满足**：本切片把镜像页映射为 supervisor **可写**（kernel 需写 `.data`/`.bss`）。
  该冲突**不可调和**（前置 3 使 `{R,X}` 只能落在含可写静态的同一段 VA 上，别名映射被排除），
  解它须链接脚本级段切分；本仓库无链接脚本，故**登记为未解冲突**，留后续 SPEC。
- **§14 的 `image` 语义分叉未消**：抽象侧 `image` 是 init 可执行镜像，实现侧 `image` = 内核镜像，
  init 实跑 ring3 用户段。
- **物理帧来自 bootloader 根的逐页解析**：本内核不解析 ELF 程序头，复刻的是 bootloader 已建立的
  VA↔物理关系——"自建页表"而非"自建装载"，登记为方法边界。
- 直映射只覆盖可用 RAM（`0x7fe0000`），不映射 MMIO/帧缓冲（本内核不访问）。
- `ISSUE-AUTH-001` 的**闭合判断交复核者**；不自行宣告满足解除条件。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`；既有 `cargo xtask test` 的 30s/无网络/精确序列
  语义不变（事件数 30→31 属本切片登记的变化）。

## 7. 下一步

1. **切片 Y（SPEC-070 / `ISSUE-TASK-001`）**：`TaskStatus::Faulted` 状态机 + §12.5 迁移入口 +
   真实 QEMU 见证（两个出口留独立 SPEC）。
2. 三节的范畴外复核（组织级门槛）。
3. 后续候选：§14 `{R,X}` 的链接脚本级段切分（未解冲突）、`ISSUE-FRAME-001` 与 `ISSUE-AUTH-001`
   的最终裁定。
