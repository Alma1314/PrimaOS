---
id: report-stage6-review-remediation
kind: report
authority: scoped-implementation-and-validation
status: verified-with-bounded-remediation
lifecycle: historical
updated: 2026-11-06
---

# 阶段6实现复核整改（RV-1–RV-7）

## 结论与范围

本报告承接[2026-11-06实现复核](2026-11-06-stage6-implementation-review.md)的七项发现。
RV-1按审查建议登记既有资源上限；RV-2/RV-5修复实现；RV-3/RV-4修正文档漂移；
RV-6合并冻结台账；RV-7增加窄口径源码守卫。阶段6的有界冻结范围与退出勾选保持，阶段7未启动。
这是局部整改的技术证据，不是组织级签署；能力历史回收与任意程序长时运行不在本次范围。
本次host1122/1123、lint/特性Clippy、同ELF11案例双轮差分及文档四项检查均通过。

## 基线与输入

- 基线：`b128e4f94c32325f4ce83e018e5dcc572ff57926`，本工作树开始时干净。
  发现报告来自主工作区尚未提交的同名文件，本次纳入工作树，保留其原结论并追加后继说明。
- 契约：[Gate协议](../specs/2026-10-29-stage6-m1-protocol.md)、
  [受管进程](../specs/2026-11-02-stage6-gate-process.md)、
  [受管装载](../specs/2026-11-01-stage6-managed-launch.md)；
  计划为[阶段6–7细化](../plans/2026-10-27-stage6-7-refinement.md)，前序运行与失败保留于
  [统一验收](2026-11-02-stage6-process-and-differential.md)。
- 使用仓库pinned `nightly-2026-07-30`。差分输入仍为D12的`/usr/bin/busybox`，
  SHA-256为`dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14`。
  参考侧复用本机缓存、由harness校验SHA-512的Ubuntu 6.8.0-31内核包；不以宿主执行替代guest。
- 本次构建的原生服务载体为3 188 336 B，长度取整779页，仍在1024页外部arena预算内；
  RV-3表中的3 159 672 B/772页指前序统一验收制品，不把两个版本混为同一输入。

## 逐项整改

| 发现 | 改动及范围 | 当前边界 |
|---|---|---|
| RV-1 | [profile v1](../compat/stage6-linux-profile-v1.md)新增单次启动资源边界 | 能力历史1280项只追加；每交付一项，至少Gate根占一项，因此至多1279次，实际因其它能力创建而更低；跨Gate/家族累计。耗尽时wait返回ENOSPC，现有服务终止并触发孤儿值。不是保证可运行1279次，更不是回收机制 |
| RV-2 | `linux_process.rs`/`linux_thread.rs`分别以常量定义4进程、8线程，生产审计使用两者乘积32槽的`PendingTraps` | 一个Task最多一个同步请求，容量等待也占一项；保留重复trap、无源终结与扫描结束未闭合断言。host回归实际填满32个原始记录、逆序终结并跨代复用槽位 |
| RV-3 | 在受管装载契约追加明确后继修订，保留旧192→768预算 | 3 159 672 B需772页，当前外部arena1024页；普通149页、私有exec640页与全局物理预算不变。本次未修改内核预算 |
| RV-4 | D12的`ls`判据改为固定命令stdout/stderr逐字节与退出码比较 | 与实际harness一致，不排序归一化，不扩大成员或变更输入 |
| RV-5 | stdin通过`INPUT.get(cursor..)`验证剩余输入，再取有界切片 | 非法游标返回EIO(-5)，在代理复制前退出且不推进游标；正常输出成功后才推进的规则保持。host回归覆盖末尾后一字节与usize::MAX |
| RV-6 | 73–89并入单张88项手写`FROZEN`，与生产完整号表直接相等 | 52仍保留，保持独立于宏生成实现的逐号钉死，不改号、不增ABI |
| RV-7 | 复用既有注释/字符串剥离与递归Rust文件收集，在`cargo xtask lint`及workspace测试中运行personality守卫 | 扫描kernel/COSIR/OSTD/runtime/ABI的典型Linux号表、errno映射、futex与clone标识符；保留ELF auxv、文档与中立Gate操作；既有原生clone例外限文件、函数及11个标志名 |

## 审查纠偏与门禁边界

发现报告的“内核只有一处测试名含linux”“全部clone标志解释在用户态”不符合原基线的全部代码：
`kernel/src/syscall.rs::clone_call`从阶段5起就解释11个`CLONE_*`标志，且源码还有Linux布局的注释。
这是原生48号适配器；阶段6门控任务先进入Gate，不调用它。当前整改不删除或改变阶段5已验证的ABI，
也不把实现自动提升为抽象规范；原报告保留，阶段跟踪登记此偏差。
因此S6-X3本次能支持的是**Gate不解释Linux调用且COSIR无新增人格策略**，不能从此证明整个仓库没有Linux形状的适配。

守卫不是关键字全局禁令：剥离注释/字符串，允许ELF事实，拒绝`__NR_*`、`LINUX_*`、典型`SYS_READ`等
Linux专用号名、含linux的syscall/errno/dispatch标识符，以及中立层的`FUTEX_*`/`CLONE_*`。
只对`kernel/src/syscall.rs::clone_call`放行既有11个clone标志名；函数外复用、其它文件出现或新增标志名都失败。
负例覆盖号表、errno转换、futex、clone以及例外扩散；正例覆盖注释/字符串、中立Gate与原生`SYS_CLONE`。
它是基于既有轻量词法扫描器的回归守卫，不是Rust语义分析；匿名数字表、换名后的策略与既有适配器内部语义仍需人工评审。
不据守卫通过扩大S6-X3的证明主张。

## 命令与结果

以下均为本次实跑；原始输出留在`/tmp/prima-stage6-review-*.log`与ignored `target/`，不入库。

| 命令 | 退出与观察 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | 退出0；1122通过、0失败、1默认忽略 | host；包含满32槽/跨代复用、坏游标、D17与守卫正反例 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | 退出0；fmt、Clippy、裸机构建、41文件零unsafe及新personality守卫全部PASS | 编译及源码门禁 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo test --workspace --features xtask/linux-probe` | 退出0；1123通过、0失败、1默认忽略 | host与差分harness负例，不代替guest运行 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets --features xtask/linux-probe -- -D warnings` | 退出0 | Linux特性编译门禁 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo run -p xtask --features linux-probe -- test-linux` | 退出0；`PASS S6REF cases=11 pinned_guest=1 same_elf=1`，11案例×2轮全部`stdout=1 stderr=1 exit=1 audit=1` | pinned Linux/Prima真实运行差分；TCG，每次Prima启动仍为30秒门限 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base b128e4f94c32325f4ce83e018e5dcc572ff57926` | 全部退出0；371文档，目录、元数据与链接检查PASS | 文档、目录、链接、元数据与基线变更检查 |

## 失败、反例与限制

- RV-2的32项回归是生产扫描器的host输入测试；不等于32个真实ring3任务同时运行或并发压力验收。
- RV-5的坏游标测试在原生复制前返回；不把host测试当成真实页表复制证据。
- 未修改内核Gate机制、启动协议或物理帧预算；未重跑普通完整72轮BIOS、G1–G4、七审计停止边界、D14与Lean。
  这些证据仍按前序报告的版本和范围解释。
- RV-1按建议登记上限后收口，历史不可回收的实现限制保留。需要更长运行时须先设计可回收历史/独立计数器，
  不能只扩大常量或删除服务断言冒充解决。

## 阶段/profile影响与重审触发

更新[当前阶段与证据索引](../../../Prima%20OS%20实现阶段.md)及[报告索引](README.md)，
不改变阶段退出勾选或扩大profile。重审触发：家族/线程上限变化、审计事件顺序或同步请求模型变化、
超过登记交付预算的工作负载、外部arena再次变化、新Linux号/errno/clone策略进入中立层，或阶段7共享root/SMP改动。
尤其不能用本次源码守卫替代第二personality的隔离与真实运行验收。
