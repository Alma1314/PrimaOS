---
id: report-stage6-implementation-review
kind: report
authority: scoped-review-evidence
status: reviewed-with-open-findings
lifecycle: current
updated: 2026-11-06
---

# 阶段 6 实现复核（2026-11-06）

## 结论与范围

本报告复核阶段 6（Linux x86_64 静态 ELF）在 `b128e4f` 上的实现与证据，不修改实现、
不改写历史报告、不改变阶段勾选。结论：

- 阶段 6 申报的技术退出 4/4 与交付 D1–D5，在可复核范围内成立。本复核独立复现了其中
  最关键的一组证据：同一未修改 busybox 与探针 ELF 在 Prima 与 pinned Linux 6.8.0-31 guest
  的 11 项双轮差分（stdout/stderr/status 逐字节、内核审计闭合）、完整 BIOS 套件 72/72，
  以及 host 门禁、lint、TCB 与文档工具；数字与[统一验收](2026-11-02-stage6-process-and-differential.md)一致。
- 未发现阻断级缺陷。发现 2 项中等、4 项低风险（含 1 项契约漂移）与 1 项回归风险（见[发现](#发现)），
  均不改变阶段 6 按其有界范围的技术结论，但应登记并指定后继。
- 本复核由工作区内的 AI 代理执行，属于技术证据；不是组织级签署、外部认证或独立第三方评审。

范围：阶段 6 的实现与证据链（`b128e4f` 及阶段跟踪 2026-11-04 结论）。不含阶段 7 设计、
Lean 证明内容审查、SMP、实机与 KVM。

## 基线与输入

- 提交：`b128e4f94c32325f4ce83e018e5dcc572ff57926`（2026-11-04 22:53:53 +0800）；
  复核时工作树干净，`origin/master` 与 HEAD 相同。
- 工具链：仓库 pinned nightly（`rustc 1.99.0-nightly (1a833e165 2026-07-29)`）、
  GCC 13.3.0、QEMU 8.2.2；宿主 8 核，TCG 单线程，无 `/dev/kvm`。
- 输入身份：`/usr/bin/busybox` 为 Ubuntu noble `busybox-static 1:1.36.1-6ubuntu3.1`，
  2 124 608 B，`sha256=dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14`，
  与 D12 台账和统一报告一致；差分 harness 在构建期校验该哈希后才打包。
- 对照文档：[阶段跟踪](../../../Prima%20OS%20实现阶段.md)、
  [Linux profile v1](../compat/stage6-linux-profile-v1.md)、
  [D12 白名单](../compat/stage6-static-whitelist.md)、
  [Gate 原生 ABI 台账](../compat/stage6-gate-native-abi.md)、
  [M1 协议](../specs/2026-10-29-stage6-m1-protocol.md)、
  [受管装载](../specs/2026-11-01-stage6-managed-launch.md)、
  [受管进程](../specs/2026-11-02-stage6-gate-process.md)。

## 实现或审查内容

静态复核读阶段跟踪、profile v1、白名单、统一报告与上述契约，并抽查
`kernel/src/{gate,syscall,launch,exec,bootstrap,user,mem,preempt,gate_audit}.rs`、
`prima-user/src/{linux,linux_main,linux_process,linux_signal,linux_thread}.rs`、
`prima-runtime/src/gate.rs`、`prima-abi/src/lib.rs`、`xtask/src/{stage6,linux,qemu,main}.rs`。

复核关注四点，均未发现越界：

1. **内核中立性**：`kernel/src` 只有 `exec.rs` 一处测试名含 "linux"；`cosir/` 无 Linux 字样；
   `prima-ostd`、`prima-runtime` 无 Linux 语义；`prima-abi` 仅 auxv 键（ELF 装载 ABI 事实，
   已在台账声明）。Gate 只搬运不透明 `u64`，门控任务的全部 syscall/int80 号（含 0 与原生控制号）
   先递送服务者（`syscall.rs` `serve`）。
2. **一次性与预检**：attach/wait/reply/abort 与复制/帧/进程控制都在变更前预检审计容量
   （`gate_audit::preflight_gate_audit`），失败进入全局 fail-stop；答复只写客户端 `rax`
   （`gate_reply`），请求槽与 root 代次在每次事务前重验。
3. **用户态边界**：Linux 号、errno、flags、fd、utsname、stat/dirent 解释全部在
   `prima-user`；未实现号与未实现变体返回确定 `-ENOSYS`，原始号/六参数/终止值经内核 89 读回。
4. **证据不变量**：30 秒门限未被放宽（`xtask/src/qemu.rs` 断言仍为 30 s）；差分 harness
   对重复、缺项、乱序、失败见证各有负例单测；未提交日志/镜像/哈希附件（`git ls-files`
   与 `target/` 检查）。

## 命令与结果

本机 2026-11-06 实跑，全部在复核开始时的新 shell 中独立执行：

| 命令 | 结果 | 与报告的关系 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | 退出 0；**1118** 通过、0 失败、1 默认忽略（19 个测试二进制） | 与报告 1118 一致 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo test --workspace --features xtask/linux-probe` | 退出 0；**1119** 通过、0 失败、1 忽略 | 与报告 1119 一致 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | 退出 0；fmt、Clippy `-D warnings`、裸机构建、**41 文件零 `unsafe`** | 一致 |
| `CARGO_NET_OFFLINE=true cargo xtask tcb` | trusted=2087、mechanism=38424、total=40511、**ratio=5.15%**，unsafe=79/0，include_bytes=0、prima_user_dep=false、service_name_lines=0、protocol_lines=52 | 与报告终值一致 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo run -p xtask --features linux-probe -- test-linux` | 退出 0；`PASS S6REF cases=11 pinned_guest=1 same_elf=1`；11 案例 × 2 轮全部 `stdout=1 stderr=1 exit=1 audit=1` | **独立复现**；报告称两轮全通过 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base 4335e18` | 全部退出 0；369 文档、3575 章节 | 一致（文档数较统一报告成文时增长） |
| `cargo run -q -p cosir --example stage6_domains` | 退出 0；两根/两空间、跨域 `NotHeld`、单侧销毁后另一侧继续 | 一致（模型示例，非机器） |
| `sha256sum /usr/bin/busybox` | `dbac288c…ac14`，2 124 608 B | 一致 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | 退出 0；**72/72** 轮、36 程序各两轮 0 失败；结尾 `PASS full BIOS suite: 36 programs, 72 runs`、`PASS stage1 boot slice` | **独立复现**；与报告单次完整 72/72 一致 |

说明：无 KVM、无实机；差分参考侧使用 pinned Ubuntu 6.8.0-31 内核的 QEMU guest（`tcg,thread=single`，
180 s 独立上限），未用宿主执行替代 guest；Prima 侧每案例独立启动、双轮、30 s 门限。

## 发现

| ID | 严重度 | 发现 | 证据 | 影响与建议 |
|---|---|---|---|---|
| RV-1 | 中 | **单次启动的 Gate 交付有全局上限（约 1279 次交付，每个 Linux 调用一次）**：每次 `gate_wait` 交付铸造一个请求根能力，历史表只追加、上限 1280；满额后 `gate_wait` 返回 `ENOSPC`，LibOS 的 `unwrap` 使服务终止，客户端按配置值孤儿化 | `kernel/src/bootstrap.rs:1638`（`MAX_CAPABILITY_HISTORY=1280`）、`bootstrap.rs:4292`（`can_record_capability`）、`kernel/src/gate.rs:1254`（交付即铸造）、`gate.rs:1209`（`Error::Capacity`）、`prima-user/src/linux_main.rs`（`rt.gate_wait(gate).unwrap()`） | 白名单实测最大 M3 445 次；G4 演练 1000 次，余量 28%。该界限未写入 profile v1。建议在 profile 的 partial/unsupported 边界登记"单次启动 ≤1279 次交付"，并在阶段 7 设计可回收历史/独立计数器 |
| RV-2 | 中 | **审计扫描槽 18 < 家族 32 线程上限**：销毁审计一次扫描用 `[None; 18]` 暂存在途 trap，`expect("bounded live Tasks")`；内核允许每家族 4 进程 × 8 线程，且容量等待的 trap 也各有一条未终结记录 | `prima-user/src/linux_main.rs:46`、`:62`；`kernel/src/gate.rs:27`（队列 8）、`:1136`（容量等待也记 trap）、`:25`（绑定 64） | 当前案例并发远低于 18；但上界不是由家族/队列上限推导。超过 18 个同时未终结 trap 时服务 panic（fail-closed，不是静默错误）。建议由常量推导或改为动态/诊断 |
| RV-3 | 低 | **契约漂移：外部帧预算写 768，实现为 1024**：受管装载契约仍写"192→768"，代码与统一报告已因 3 159 672 B 制品越过 768 页而扩到 1024 | `docs/superpowers/specs/2026-11-01-stage6-managed-launch.md:44`；`kernel/src/bootstrap.rs:2632`；统一报告"输入身份/行为与边界"节 | 按仓库规则应记录偏差。建议在契约加后继说明（3 159 672 B = 772 页 > 768），或在台账登记 1024 |
| RV-4 | 低 | **白名单 `ls` 判据措辞过弱**：台账写"条目集合与参考 guest 一致（排序后比对）"，实际证据是 stdout/stderr 逐字节相等 | `docs/superpowers/compat/stage6-static-whitelist.md` 成员表；差分 harness 逐字节比较 | 证据强于判据，不是缺陷；建议统一措辞，避免后续读者误以为只做集合比较 |
| RV-5 | 低 | **`read` 的输入游标减法未检查**：`INPUT.len() - shared.input_position` 依赖"游标单调且不越界"的隐式不变量 | `prima-user/src/linux.rs:302` | 当前不变量成立；一旦输入源或游标改动，下溢会造成 panic/越界切片。建议 `checked_sub` 或显式断言 |
| RV-6 | 低 | **D17 追加段（73–89）用测试内联数组钉死**，未扩进 `FROZEN` 常量；注释说"扩展手写 FROZEN 表"，实现是第二个数组 | `prima-abi/src/lib.rs:476`、`:563`、`:566` | 语义等价且能挡重编号；属可读性/维护性问题。建议合并为一张 88 项 `FROZEN` |
| RV-7 | 风险 | **S6-X3 无自动守卫**：内核/COSIR 当前确无 Linux 语义（本次 grep 复核），但唯一保证是人工复核 + TCB 耦合指标；阶段 7 的 7-A 会引入人格/共享 root，无测试可挡回归 | 复核 grep：`kernel/src` 仅 `exec.rs` 一处测试名；`cosir/`、`prima-ostd/`、`prima-runtime/` 0；`prima-abi` 仅 auxv 测试名 | 阶段 6 条件按当前源码成立；建议在 7-0 增加窄口径守卫（禁止内核出现 Linux 号表/errno 映射/clone flags 等），不要求关键字全局扫描 |

值得记录的正面结论（非穷举）：审计 fail-stop 的"首次诊断 + 不可重置 + 预检先于变更"设计完整；
答复只写 `rax`、请求一次性终结、root 代次与 cookie 双重校验在代码与 host 反例中一致；
差分 harness 的输入哈希、同源 ELF、负例单测与"不使用宿主替代 guest"边界清楚；
白名单成员与 G4 计量都是真实请求路径，不是空循环。

## 失败、反例与限制

- 本复核**未独立重跑** G1–G4、七个审计停止边界与 D14 的 QEMU 探针，也未重跑 Lean；
  这些结果仍以[统一验收](2026-11-02-stage6-process-and-differential.md)及其所链报告为准。
  本报告不对它们作二次确认。
- 完整 BIOS 套件在复核期间作为单条命令执行：36 程序 × 2 轮 = 72/72，退出 0，0 失败；
  未拼接历史分段结果，30 秒门限与既有断言未改动。
- 无 KVM/实机：任何延迟数字只是 TCG 观测，不构成性能门槛；M1e 的 p50/p99 未在本轮复测。
- 复核不是独立组织评审：同一工作区、同一代理完成，可能共享盲点；读者应把它当作"可复现的
  技术核对"，而不是第三方认证。

## 阶段/profile 影响

- 不改变阶段 6 的状态、勾选或 profile v1 冻结范围；不启动阶段 7。
- 建议后继（不改本轮结论）：把 RV-1 的交付上限与 RV-3 的 1024 帧预算登记到对应 profile/契约；
  RV-2、RV-5 作为实现硬化项；RV-7 进入阶段 7-0 的门禁清单。
- 重审触发：任何引用超过 1 279 次交付或超过 18 个并发 trap 的工作负载；外部帧预算再次变化；
  阶段 7-A/7-B 引入共享 root、SMP 或第二 personality 时。
