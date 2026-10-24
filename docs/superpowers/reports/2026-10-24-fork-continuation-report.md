---
id: report-fork-continuation
kind: report
authority: scoped-evidence
status: partial-validation
lifecycle: historical
updated: 2026-10-25
---

# fork 续执行与进程边界整改验证

## 结论与范围

ISSUE-FORK-CONTINUATION-001 在当前单核/单线程工作子集内技术收口：
W6/W7/W7b/W8 已撤掉原生 fork 汇编绕行，真实 musl/C 返回、COW 快照、子退出与父等待验收通过。
本轮 Rust937项、lint、进程聚焦8轮及文档检查通过；BIOS取得62/62轮分段成功覆盖。
完整命令在35轮成功后于W8 run2触发30秒超时，**未取得完整单次PASS**；没有自动重试。
本轮先成批完成COW、模型、等待输出和退出接线，再集中验证。
阶段5未签署、阶段6未启动、BLK-003 Open，所有阶段退出勾选保持不变。

## 基线与设计输入

本轮于2026-10-24开始，2026-10-25定稿；日期文件名保留任务起始日期。
直接修改干净 master `a1a45bbd12656f5de184bea81a539a8283fb8053`。
工作契约见[fork 续执行设计](../specs/2026-10-24-fork-continuation-design.md)，
前序实现 [M17b-2a](2026-10-23-stage5-m17b2-cow-primitives.md) 与失败链 [M17b-2](2026-10-23-stage5-m17b2-cow-fork.md) →
[M17b-3](2026-10-23-stage5-m17b3-zombie-wait.md) → [M17b-4 原生绕行](2026-10-23-stage5-m17b4-fork-wait.md)。
Asterinas 本地锁定 `ceb65e3f850096d30283137845ac01ad51dfcdaf` 的 VMAR fork 明确要求父写保护后、返回前同步 TLB；
VmSpace protect 不自动刷新。两份输入与 Git blob 逐字核对，哈希见[来源登记](../../architecture/asterinas-provenance.md)。
仅参考责任顺序，独立适配现有页表/帧注册表，没有复制上游实现、引入依赖或修改参考仓库。

## 实现与根因

旧 fork 改父 PTE 为只读 COW 后直接返回，缓存的可写用户翻译仍可让父继续写共享页，
尤其是 C 包装退栈路径，破坏子进程的继承栈。旧报告观察到保存帧 RAX=0 而 C 返回后成为垃圾值，与该遗漏一致。
本轮撤掉旧原生绕行先复现失败；在完整整改后验证真实 C 返回和父先写的隔离见证。
没有单独的硬件 TLB 可视化或只删刷新回调的 QEMU mutant；证据是代码责任、失败复现、host提交顺序和实际父子快照检查。

- 页表：`clone_root_cow` 在子表全部构造成功后才保护父并重装当前CR3；分配/用户大页/global拒绝清理私有子表，父逐字不变。
  后续模型/调度登记失败也清理子表/根；不承诺整个fork的父PTE原子回滚。supervisor子树共享，NX等标志保留。
  明确修订M17b-2a的用户大页原样共享行为：当前只支持4KiB用户页，遇大页拒绝而不是可写共享；不是已实现大页COW。
- 模型：取帧前检查调用者/单线程/Task/Process/vspace/audit容量；映射复制预检后提交，取消未发布子任务清理Process与映射。
- 等待：旧立即/阻塞路径把int写成8字节；现在统一四字节，检查所有页表级权限、低canonical范围和跨页，显式解析COW后经物理直映射写入。
  状态写回先于最终RAX帧读取，pending随清任务丢弃；错误返回EFAULT/ENOSPC，避免supervisor盲写用户VA。
- 退出：C适配层子返回记私有child标记，musl `_exit`/exit_group走现有SYS_PROCESS_EXIT。
  子路径不发送继承fd的CLOSE，避免误用父的manager/self能力；这没有修复共享cap/close隔离。
- 探针：W6依次执行musl syscall(57)、fork()/waitpid、C __syscall0(57)，父在等待前写64字栈/全局，
  子检查旧快照/TLS errno后正常_exit；父检查私有值、int邻接哨兵、退出状态及ECHILD。
  W7/W7b/W8改走musl fork；截断exec返回后由musl的 `_exit` 退出。
  `test-process`共用原validator、30秒门限和双轮IPC基线，无自动重试。

## 环境、命令与实际结果

pinned nightly-2026-07-30 / rustc1.99.0-nightly (1a833e165 2026-07-29)，CARGO_BUILD_JOBS=2，musl默认make-j2。
QEMU8.2.2 / pc-i440fx-8.2 / TCG thread=single / qemu64 / 1CPU /128M/无NIC，正式时限30秒不变。
构建并行度不等于CPU利用率限制。日志与冻结工件位于ignored `target/fork-validation/`，不提交镜像。

| 本轮命令 | 实际结果与范围 |
|---|---|
| CARGO_BUILD_JOBS=2 cargo test --workspace | PASS937；kernel445，OSTD41，xtask207；含2rustdoc和5compile_fail |
| CARGO_BUILD_JOBS=2 cargo xtask lint | PASS fmt/Clippy-Dwarnings/裸机构建/29个kernel源文件零unsafe |
| CARGO_BUILD_JOBS=2 cargo xtask test-process | 单次PASS8轮：W6/W7/W7b/W8各连续两轮；含Linux W6和W8参考 |
| CARGO_BUILD_JOBS=2 cargo xtask test | FAIL：前35轮成功，W8 run2在30秒超时；最后到内核seq11，无SIG输出 |
| 未提交原后缀驱动（suffix） | FAIL：W9/W7各双轮通过后，W7b run1超时，最后到seq11 |
| 未提交剩余独立组（remainder） | 实际通过pair及失败见分段范围；不重试，组失败保留非零结果并继续其余独立组 |
| cargo xtask tcb | PASS，见下方源码指标 |
| python3 scripts/docs.py self-test / build / check / check --base a1a45bb | PASS304份；报告定稿后重建/复核目录 |

新增8个Rust测试覆盖7个子表分配失败点、源表快照、恰好一次归还、刷新顺序、用户大页/同根拒绝、
NX/supervisor保留、跨页COW写回、缺帧不写用户数据、逐级权限与越界、pending清理以及模型容量/取消回收。
host权限/地址测试不替代真实缺页恢复；QEMU只建立上述单核路径的机器证据。未运行Lean/Miri/SMP。

| 聚焦探针 | stdout长度 / FNV-1a | audit_used /8192 | 观察 |
|---|---|---|---|
| W6 | 62 /673595661 | 1454 | fork/wait/status/cow/echild/guard全1；IPC queued/direct/blocks=45/169/177 |
| W7 | 36 /2578524652 | 1138 | exec/wait/bad全1 |
| W7b | 39 /2616907564 | 4762 | read/exec/wait全1 |
| W8 | 38 /1153297872 | 1156 | kill/wait/signal全1 |

上述四例tasks=12、caps_live=132/peak140/limit160、segment_slots=41/limit48；无容量上限修改。
stdout/IPC因程序变化按实际路径解释；双轮比较和所有旧断言未放宽。

### 分段覆盖口径

完整命令选取default、E1–E6、T1、F1、V1–V3、N1、W1–W3、W6的17个完整pair（34轮），
W7/W7b/W8选择聚焦process.log同一驱动内的双轮（6轮），suffix.log选择W9双轮（2轮），
remainder.log选择剩余成功pair；单独成功轮、重复pair不叠加覆盖。
后缀与剩余组引用当前源模块，从正式xtask二进制提取精确CARGO_BIN_FILE产物路径，
保持原30秒TCG配置、validator、stdout/容量/IPC双轮比较、输入注入和剩余Linux参考。
剩余驱动按独立组继续，任何失败仍使其整体退出1；没有自动重试。
临时驱动编译因省略前缀产生dead_code告警，未压制；正式项目lint零告警通过。
分段覆盖不等于完整命令通过或连续运行稳定，失败输入仍须独立解释。

### TCB与耦合

```text
TCB trusted_lines=1915 mechanism_lines=24998 total_lines=26913 ratio=7.12%
TCB unsafe_tokens trusted=72 mechanism=0
TCB kernel_service_coupling include_bytes=0 prima_user_dep=false service_name_lines=0 protocol_lines=39
PASS tcb metric
```

该指标含内联测试，是源码代理。比例变化不构成可信面、安全或性能证明；本轮OSTD源码无改动。

## 失败、反例与保留边界

1. 在旧内核上撤掉W6内联原生51，`cargo xtask test-process W6`复现
   `PROC|fork=1;wait=0;status=0;cow=1;ech` 后panic，位置为旧版本kernel/src/syscall.rs:2204。
   原始baseline-w6.log保留；冻结baseline-w6.img SHA-256
   `3832e7dc74a9857268fb5a6cfec9ea143c3cc7761182583f9a4a28e09166d3c3`。
2. 只补CR3重装的中间代码两次30秒超时：flush-w6.log最后到seq11，flush-w6-retry.log完全无输出。
   它们不是成功证据，也不能用以声称TLB解释启动超时。
3. 第一轮集中workspace编译因新测试漏引用MAX_VSPACE_ENTRIES失败，E0425；修复引用后通过，
   workspace-first-compile-failure.log保留。用户中断前的kernel-first.log只证明较早版本437项。
4. [Once前轮](2026-10-24-asterinas-once-publication-report.md)三次完整命令失败、60/62分段覆盖与31次超时保持历史事实。
   本轮取得62/62分段成功覆盖及W7 pair，但完整命令仍失败，不追溯改写该版本的缺口，不宣称启动超时根因已解决。
   本轮完整命令W8 run2的失败保留于qemu.log；冻结w8-full-timeout.img SHA-256
   `d4f5effa1cb2018c9505472dbecd498356cbd78df7281196bc7ece5a1cce8c31`。
   同代码W8在聚焦命令双轮通过、完整命令run1通过；超时最后到seq11，原因尚未定位。
   suffix.log在W9/W7双轮通过后W7b run1也在seq11超时；冻结w7b-suffix-timeout.img SHA-256
   `244047ba97c0110e93d4c62a042c55b9df2039400c1227f365454c745866f618`。
   同代码W7b聚焦双轮通过。这些超时不计入成功覆盖，原因仍未定位。
5. COW构造Err回滚与整个fork的回滚不同；后续模型失败可能保留父COW PTE；回调panic不支持。
   两页写回若第二页缺帧，第一页面已私有化但用户数据不写。等待中其他线程撤图与错误后僵尸交付无完整并发保证。
   本片不是通用fault-safe用户copy，不替代其余rawVA调用审计。
6. fd共享cap/close隔离、子VFS授权、CLOEXEC、信号掩码/完整atfork、多线程fork、SMP/PCID、
   COW数据帧的独立引用/回收、尚未接线的fork子FPU状态继承，以及完整寄存器继承证明仍未完成。
   原始汇编51不经过C适配层child标记，其后的libc exit_group不在本片承诺内。

## 阶段与后继关系

只在本文范围技术关闭 ISSUE-FORK-CONTINUATION-001，更新阶段与兼容台账、进程/内存主题路由。
M17b-3/4的失败与绕行、M20/M21当时的限制继续保留为历史材料。
不把单线程libc fork支持等同完整POSIX、阶段5组织级签署、BLK-003关闭或阶段6启动。
重审触发见工作契约；改变页表/CPU、错误回滚、COW所有权、wait输出、退出或能力继承时须补对应证据。

## 本轮最终覆盖清单

机器核对31种用例的完整pair，共62/62轮分段成功覆盖。缺完整pair：无。

| 用例 | 所选同一驱动日志 |
|---|---|
| default | `qemu.log` |
| E1 | `qemu.log` |
| E2 | `qemu.log` |
| E3 | `qemu.log` |
| E4 | `qemu.log` |
| E5 | `qemu.log` |
| E6 | `qemu.log` |
| T1 | `qemu.log` |
| F1 | `qemu.log` |
| V1 | `qemu.log` |
| V2 | `qemu.log` |
| V3 | `qemu.log` |
| N1 | `qemu.log` |
| W1 | `qemu.log` |
| W2 | `qemu.log` |
| W3 | `qemu.log` |
| W6 | `qemu.log` |
| W7 | `process.log` |
| W7b | `process.log` |
| W8 | `process.log` |
| W9 | `suffix.log` |
| W4 | `remainder.log` |
| W5 | `remainder.log` |
| I1 | `remainder.log` |
| M1 | `remainder.log` |
| C1 | `remainder.log` |
| L1 | `remainder.log` |
| L2 | `remainder.log` |
| L3 | `remainder.log` |
| D1 | `remainder.log` |
| A1 | `remainder.log` |

剩余组整体结果：PASS。
