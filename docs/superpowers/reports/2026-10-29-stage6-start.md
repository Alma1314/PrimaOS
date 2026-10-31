---
id: report-stage6-start
kind: report
authority: scoped-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-10-31
---

# 阶段 6 启动与首个 S6-M2 增量：确定性 `AT_RANDOM`

## 结论与范围

- **阶段 6 进入"进行中"**：作者于 2026-10-29 指示启动（此前按 D10 为"未开始"）。
  启动是状态变更，不是退出条件达成：阶段 6 的 5 项交付检查与 4 项退出条件**仍然未勾选**。
- **首个增量实现了 D13 的 `AT_RANDOM` 政策**：初始栈提供 16 字节公开常量块，
  `AT_RANDOM` 指向它。这是 S6-M2（未修改静态 ELF 加载）的一项，不是完整的 S6-M2。
- **白名单按 D12 冻结为台账**（[stage6-static-whitelist.md](../compat/stage6-static-whitelist.md)），
  它是契约，不是实现状态。
- **未覆盖**：没有任何未修改二进制在本系统运行；S6-M1（同步递送）仍缺架构设计，
  装载侧容量缺口未动。

## 基线与输入

| 项 | 值 |
|---|---|
| 基线 commit | `3096683`（D11–D16 裁决），本次改动在其后 |
| 工具链 | `nightly-2026-07-30`；QEMU 由 `cargo xtask test` 驱动 |
| 白名单制品 | Ubuntu `busybox-static 1:1.36.1-6ubuntu3.1`，`sha256=dbac288c…`（见台账） |
| 依据 | [D11–D16 裁决](../specs/2026-10-28-stage6-7-release-decisions.md)（D10/D12/D13/D14）、[装载切片报告](2026-10-28-stage6-loader-slice.md) |

## 启动决定与边界

| 项 | 记录 |
|---|---|
| 依据 | 作者 2026-10-29 指令；已写入阶段跟踪决策日志与阶段 6 状态行 |
| D10 的实质约束保持有效 | Linux persona 与 syscall 语义留在用户态 LibOS，内核只做 OS 中立机制；本次改动未引入任何 Linux syscall 号或错误码 |
| D14 的顺序调整 | 原裁决要求"阶段 6 动工前"做 7-A 有界探索原型。启动指令与它的先后冲突，因此按以下口径记录：原型**保留为 S6-M1 架构评审前的检查点**，可与 S6-M2 并行；在原型给出结论前不进入 S6-M3 的 profile 冻结（见[裁决文书的调整记录](../specs/2026-10-28-stage6-7-release-decisions.md#阶段-6-启动记录2026-10-29)） |
| 未勾选 | S6-D1–D5、S6-X1–X4 全部保持未勾选 |

## 实现内容

`kernel/src/exec.rs`：

1. 新增 `AT_RANDOM = 25` 与 `AT_RANDOM_BYTES = b"PRIMA-RND-16BYTE"`（16 字节）。
2. `build_stack_inner` 增加 `random` 开关：在建栈时把 16 字节块写到栈缓冲最高处
   （`[top-16, top)`），因此它落在可读栈页内；`AT_RANDOM` 在 `auxv` 之后、
   `AT_EXECFN` 之前追加，值为该块地址（8 字节对齐）。
3. `build_initial_stack` 打开该开关；`build_stack` 底层原语行为不变。
4. 栈字数预算：基本 14 对 + `AT_RANDOM` + `AT_EXECFN` = 16 对；最坏情况
   （`argc=16`、`envp=8`）为 61 字，仍在 64 字上限内。

**为什么是常量而不是"随机"**：项目没有可信熵源。该字段的作用是让 SSP 构建的未修改
二进制完成 libc 启动（其 canary 初值可能从 `AT_RANDOM` 读取，实测冻结制品有 2 402 处
`%fs:0x28`），不是安全机制。内容确定、公开、可审计；任何"抗利用"论证不得引用它。
改动内容即重新裁决（D13）。

单测覆盖：`AT_RANDOM` 存在、非零、8 字节对齐、指向的 16 字节等于常量、`auxv` 共 16 对；
`AT_EXECFN` 仍指向栈上 `argv[0]`。

## 命令与结果

| 命令/输入 | 本次实跑 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test -p kernel --lib` | 本次 | PASS：491 passed，1 ignored（样本演练） | host 单测 |
| `cargo xtask lint` | 本次 | PASS：fmt、clippy `-D warnings`、裸机构建、`kernel/src` 零 `unsafe` | host 门禁 |
| `cargo xtask test` | 本次 | **PASS：`full BIOS suite: 36 programs, 72 runs` + `PASS stage1 boot slice`**（默认 locale，`AT_RANDOM` 改动后） | QEMU 执行 |
| `python3 scripts/docs.py self-test/build/check` | 本次 | PASS（含新增台账与报告） | 文档治理 |

说明：`cargo xtask test` 覆盖既有 36 个阶段 5 探针 × 2 轮；exec/execve 探针走的就是被改的
`SYS_EXEC` 初始栈路径（初始栈多了一对 `AT_RANDOM` 与 16 字节块），因此它是本次改动的回归
证据。它不证明任何未修改二进制可运行。

## 失败、反例与限制

- **`AT_RANDOM` 不是熵**：canary 值可预测；无 ASLR、无密钥。把本次改动读成"安全增强"是误读。
- **canary 是否真的从这里加载未验证**：冻结制品的 libc 启动对 `AT_RANDOM` 的具体使用
  需要运行期见证（S6-M2 后续），本次只保证字段存在且可读。
- **容量三道缺口未动**：`image_len ≤ 1 MiB`、`MAX_SEGMENT_FRAMES=149`（冻结制品需 526 页）、
  8 页（32 KiB）初始栈。这三条任一未解，白名单成员都无法真正启动。
- **两套 auxv 构造器仍在**：内核 `SYS_EXEC` 路径（本次改动）与 `prima-servicemgr` 的 C
  装载路径各自构造初始栈；合并属 S6-M2/S6-M6。
- **S6-M1 仍无设计**：内核目前只有异步异常通知与信号管道，没有"同步 call/reply"语义；
  这是阶段 6 的唯一真实难点，必须先有架构文档再动代码。

## 阶段影响

- 阶段 6 状态：`未开始` → **`进行中`（2026-10-29）**；退出条件与交付检查不受影响。
- S6-M2：部分推进（初始栈 auxv 完整 + `AT_RANDOM`）；仍缺流式装载、容量、白名单镜像。
- 阶段 5 / profile v2 / SPEC-152：不受影响。
- 阶段 7：D16 子轨结构不变；7-A 探索原型从"启动前置"调整为"S6-M1 评审前检查点"。

## 后继关系

- 上游：[D11–D16 裁决](../specs/2026-10-28-stage6-7-release-decisions.md)、
  [装载切片](2026-10-28-stage6-loader-slice.md)、[白名单台账](../compat/stage6-static-whitelist.md)
- 下一步：S6-M1 递送架构文档（设计与评审）→ 装载容量（`MAX_SEGMENT_FRAMES`/`image_len`/
  初始栈）→ 白名单镜像入库 → S6-M3 profile 行

## ELF 输入读取后继（2026-10-31）

[按需读取与故障回滚](2026-10-31-stage6-streamed-elf-input.md)关闭原生 exec 的 1 MiB
文件长度与首个 4 KiB 程序头限制，并验证超过 2 MiB 的稀疏合成输入。段数据在该后继的
基线 `b0d78cf` 中已经逐页复制；本文早期“整块 copy-in/流式装载未实现”的描述不再作为
当前实现结论。149 页映像帧容量、8 页初始栈验证、VFS 大文件来源与 Gate/LibOS 的缺口
仍保留；后继不代表白名单成员运行或 S6-D1 完成。
