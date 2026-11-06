---
id: report-gate-request-audit-history
kind: report
authority: scoped-implementation-and-validation
status: verified-with-bounded-remediation
lifecycle: historical
updated: 2026-11-06
---

# RV-1后继：解除Gate交付的通用能力历史上限

## 结论与范围

承接[RV-1发现](2026-11-06-stage6-implementation-review.md)与[首轮边界登记](2026-11-06-stage6-review-remediation.md)。
本次按作者继续修复的指令落实[审计承载存储契约](../specs/2026-11-06-gate-request-audit-history.md)：
将每次交付的完整不可变根记录存入已有GateDelivered事件，不再重复占1280项通用历史槽。
1300次host交付已通过，通用历史全满仍可交付；workspace/lint/特性Clippy、G1–G4、七审计停止边界、
Linux11案例双轮、完整BIOS72/72及文档四项检查全部通过。最终G4物理槽排除检查已双轮重跑通过。
这是存储细化，保留只追加历史、CapId单调、句柄代次、授权、请求一次性及原审计fail-stop。
审计池16384条及其它能力资源仍有界；本报告不主张无限服务、历史回收或新的Lean证明。

## 基线与契约

- 基线：`e37c46f39e3d6dc6ba5924aaab63e7f6607c4661`，开始时工作树干净。
- 上位：[原语规范](../../../COSIR%20原语规范.md)§4.2/§13/W13、
  [Gate协议](../specs/2026-10-29-stage6-m1-protocol.md)、
  [阶段6–7计划](../plans/2026-10-27-stage6-7-refinement.md)。实现与前序运行链保留于
  [统一验收](2026-11-02-stage6-process-and-differential.md)，不把本次细化视为阶段7启动。
- 工具链仍为仓库pinned nightly；Linux输入仍为D12原字节busybox，
  SHA-256=`dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14`。

## 实现与不变量

- `bootstrap.rs`保留通用root铸造的完整预检/历史路径，仅抽出安装活动记录的私有共同函数。
  Gate交付预检不再依赖通用历史余量；新Gate/上下文根/派生等仍检查通用1280槽预算。
- `GateDelivered`携带完整CapabilityHistoryRecord；原CapabilityMinted仍在原序号写入。
  根parent=None、固定权利、同一单调CapId与带代次句柄；存储位置变化不产生缺失、覆盖或复用的逻辑历史。
  完整记录嵌入既有事件，不增加事件数或更改89输出、复制/答复布局。
- 历史查询查两处，历史长度返回总数；Namespace身份选择仍只需通用历史，因新后端仅含GateRequest根。
  原1280槽资源计数保留，trace上界补齐为1281（通用记录加一个根）。
  父根drop后别名仍可溯源；完成撤销全部别名；IPC/Namespace退役token依统一历史识别，不恢复授权。
- G4保持1000真实请求及原计量时点，验证根的目标/权限/parent/铸造事件和首末历史查询，
  并直接扫描通用arena确认没有任何请求根副本（G4不派生请求别名），避免长度统计掩盖重复存储；
  输出`S6HISTORY|deliveries=1000;audit_roots=1000;dedicated_slots_saved=1000;verified=1`。
  xtask拒绝缺失、重复或错误标记，原时间戳/校准判据保持。
- 本机host基线/当前rlib的`size_of`测量均为AuditEvent=128 B、BootstrapState=13312 B、
  CapabilityHistoryRecord=56 B；新字段未增大事件或状态结构体。测量使用临时Rust程序分别链接两份host库，
  不据此声称其它架构或未来布局不变；原16384条审计及1280项通用历史的容量未增加。

## 命令与结果

原始日志留在`/tmp/prima-gate-history-*.log`和ignored target，不入库。

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib bootstrap::gate::tests` | 退出0；35通过 | host；新增1300次交付、通用历史已满、根drop后别名溯源/一次性终结；既有容器与失败预检回归 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | 退出0；1125通过、0失败、1默认忽略 | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | 退出0；fmt、Clippy、裸机构建、41文件零unsafe及personality守卫PASS | 编译/源码门禁 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo test --workspace --features xtask/linux-probe` | 退出0；1126通过、0失败、1默认忽略 | host与差分harness负例 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets --features xtask/linux-probe -- -D warnings` | 退出0 | 特性编译 |
| `PRIMA_GATE_CASE=G4 CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets --features xtask/gate-probe -- -D warnings` | 退出0 | 最终Gate特性编译及物理槽排除检查 |
| `PRIMA_GATE_CASE=G4 CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-probe -- test-gate` | 环境准备后退出0；最终物理槽排除检查源码双轮复跑，各1000请求，S6HISTORY与时间戳校验PASS | QEMU TCG；首轮缺快照的构建失败见下文 |
| `CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-probe -- test-gate` | 退出0；G1双轮 | QEMU；固定服务/原始参数/拒绝及一次性答复 |
| 上条分别设`PRIMA_GATE_CASE=G2`、`G2-death`、`G3` | 均退出0；各双轮 | QEMU；队列/销毁、跨root复制与COW/失败矩阵 |
| `PRIMA_GATE_AUDIT_SITE=<site> CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-audit-probe -- test-gate-audit`，site依次为`int80-entry`/`dispatch`/`int80-return`/`fast-return`/`pick-next`/`resume`/`irq` | 七条命令均退出0，各双轮预期QEMU退出35、`unchanged=1` | 真实审计停止边界，预检及保存状态未放宽 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo run -p xtask --features linux-probe -- test-linux` | 退出0；pinned Linux参考11项及Prima每项双轮，stdout/stderr/status/审计全部相符 | 同一原始ELF、TCG；Prima仍为每次30秒门限 |
| `CARGO_NET_OFFLINE=true cargo xtask test`（允许本机UDP回环的执行环境） | 退出0；单次完整36程序×2轮=72/72，0失败；`PASS full BIOS suite: 36 programs, 72 runs` | 最终普通启动/阶段5回归；原30秒门限未放宽 |
| `target/debug/xtask tcb`（本次完整BIOS命令编译的默认特性xtask） | 退出0；trusted=2087、mechanism=38580、ratio=5.13%，unsafe=79/0；耦合0/false/0/52 | 源码度量，不等于完整内核证明 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base e37c46f39e3d6dc6ba5924aaab63e7f6607c4661` | 全部退出0；373文档，目录、元数据、链接、基线变更检查PASS | 文档门禁 |

原始计量时点、每次Prima启动30秒门限、白名单输入与差分/审计断言未放宽；没有拼接不同版本的完整回归结果。

## 失败、反例与限制

- G4首次构建退出1：当前工作树缺少`target/musl-1.2.5`锁定快照，未启动QEMU。
  从本机主工作区的既有快照以本地`git clone --no-hardlinks`恢复musl
  `0784374d561435f7c787a555aeab8ede699ed298`和sbase
  `c546c3a5724c81cee9a11d816a38ccdf17472129`，并复制锁定musl归档后重跑成功。
  构建脚本原commit/干净状态/许可与归档哈希校验保持，缓存均在ignored target，未放宽或跳过校验。
- 1300次是host真实对象/调度状态迁移，不是1300次ring3执行。
- 审计承载后端的历史查找对审计长度线性扫描；活动授权仍先查活动表，常规代理复制不以历史扫描代替授权。
  不据G4通过承诺一般性能或无限服务。
- 通用历史满时请求别名派生仍返回CapacityExceeded；继续交付只消除了请求根对这一配额的依赖。
- 没有回收审计或普通能力历史；审计满仍全局停止。未来若修改drain/保留规则，须重新评审原语规范与查询证明义务。
- 本次不新增形式证明；前序Lean结论按其既有范围解释。

## 后继与阶段影响

更新[profile v1](../compat/stage6-linux-profile-v1.md)的资源边界、
[当前阶段](../../../Prima%20OS%20实现阶段.md)与报告/规范索引，保留首轮1279上限的发现与登记。
不新增Linux入口/变体、不扩大白名单或改阶段勾选。
重审触发见上位存储契约；尤其是回收审计、请求身份复用、历史后端/计数规则或共享root/SMP变化。
