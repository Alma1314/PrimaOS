---
id: report-native-runtime-service
kind: report
authority: evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-24
---

# 原生运行时与服务重构验证

## 结论与范围

修复原生 Hello 字段一致性漏检、服务路径续片覆盖和双路径事务残留；提取三个基础服务
实际共用的纯状态机供 host 验证。减少重复组装与服务循环内的协议状态处理，不承诺未经
测量的吞吐/延迟提升。设计与错误语义见[契约](../specs/2026-10-24-native-runtime-service-design.md)。

## 基线与输入

- 基线 `cf2b0460b26d5cfe20a94239ad4bb881efcedbbb`，工作区初始干净。
- Rust `nightly-2026-07-30` / rustc `1.99.0-nightly (1a833e165 2026-07-29)`。
- QEMU `8.2.2`，BIOS、qemu64、单 vCPU、128 MiB、`-nic none`；保留 30 秒启动门限。
- 前序：[阶段 4 收口](2026-10-16-stage4-closure-report.md)、[退出复核](2026-10-17-stage4-exit-review-spec149.md)、
  [M13a](2026-10-22-stage5-m13a-vfs-links.md)、[M13b](2026-10-22-stage5-m13b-permissions-stdio.md)、
  [VFS 基础](2026-10-23-vfs-libos-foundation-report.md)与[musl 边界](2026-10-24-musl-port-boundary-report.md)。

## 实现

| 位置 | 行为 |
|---|---|
| `prima-runtime/src/protocol.rs` | 共享 HELLO_OPCODE；decode_hello 校验 opcode/flags/长度/头与 offer 版本；增加 InvalidHello，保留其它错误类别 |
| `prima-user/src/lib.rs`、`staging.rs` | no_std/forbid unsafe 的真实服务状态机，复用首片/续片函数；顺序接收、失败取消、MUTATE 未知 op 拒绝、双槽重置、确认后开启槽 1、最终提交消费一次 |
| `prima-user/src/service.rs` | 调用提取模块；无效新 PAIR 也取消旧单路径；新 readlink 清空旧结果；分片错误不触发 VFS |
| `prima-user/src/main.rs` | 使用共享 Hello opcode，合法请求字节不变 |
| `xtask/src/protocol.rs` | 严格检查新 ring3 负例掩码，每个缺失位均有拒绝测试 |

合法服务请求的标签、长度、权限、队列消息数、syscall 号、VFS 镜像 v3、容量常量不变。
无效序列现在拒绝；不支持续片回写/重传。新增 CodecError 变体是 0.x 源码 API 扩充，
穷尽匹配此枚举的外部源码需要调整，不承诺二进制 ABI。

ring3 `negotiation_negatives` 见证从 `0x0000000100000000` 改为
`0x00000001003f0000`：高 32 位仍为 MajorMismatch=1、低 16 位仍为 BadMagic=0，
bits 16..21 必须全为 1，依序代表 opcode、flags、短长度、长长度、major、minor 矛盾被拒绝。
xtask 逐字检查，不接受旧见证；内核继续只转储 opaque trace，不理解服务语义。

## 本次命令与结果

日志保留在未跟踪的 `target/runtime-service-validation/`，不提交生成镜像/依赖。

| 命令/输入 | 结果 | 范围 |
|---|---|---|
| 提取旧生产代码后 cargo test -p prima-user --lib | 退出 101：3 条预期回归失败 | 覆盖被接受、旧槽 1 残留、槽 1 可独立开始；证明原问题可复现 |
| CARGO_BUILD_JOBS=2 cargo test --workspace | PASS，896 项，0 failed | 新增 runtime 4、服务状态机 9、xtask 1；kernel 等既有测试保留 |
| CARGO_BUILD_JOBS=2 cargo xtask lint | PASS | fmt、Clippy -D warnings、裸机 kernel 构建、29 个内核文件零 unsafe |
| CARGO_BUILD_JOBS=2 cargo xtask test（首次） | 退出 1；36 轮 BIOS PASS 后，W9 run 1 超时且 stdout/stderr 均空 | qemu-first-timeout.log 保留，30 秒门限未改 |
| 同一 W9 镜像隔离复跑两轮 | PASS，退出 0 | w9-probe.log；直接调用原 qemu.rs/protocol.rs 的 run_bios_test_program，全部原断言与双轮 IPC 比较 |
| CARGO_BUILD_JOBS=2 cargo xtask test（重跑） | PASS，退出 0，62 轮 BIOS / 115 条 PASS | qemu-retry.log；默认及全部固定程序双轮，含 Linux 参考与 musl host 检查 |
| python3 scripts/docs.py self-test | PASS | 文档工具自身回归 |
| python3 scripts/docs.py build / check / check --base cf2b046 | PASS，295 个文档 / 2888 个章节 | 目录、链接、生命周期与增量校验 |

host 状态机覆盖全部 1..64 B 长度、空 fstat/fchmod、超长/极大参数、覆盖/重复/跳片/完成后追加、
失败后重试、错 op/错槽/未确认/不完整提交、跨事务清理与重复最终提交。
Hello host 测试覆盖版本/feature/尺寸协商、全部结构字段变异、无效 offer 与必需 feature 拒绝。
完整重跑包括默认、E1–E6、T1、F1、V1–V3、N1、W1/W2/W3/W6/W8/W9/W7/W7b/W4/W5、
I1、M1、C1、L1–L3、D1、A1 各两轮；VFS stdout/退出码、IPC 双轮计数及全部既有断言保留。

## 失败与限制

- 首次 lint 发现提取后 service.rs 遗留三个未使用 import；移除后重跑通过，不放宽警告。
- 原始回归失败记录在 staging-before.log；后继通过不改写旧行为。
- 首次完整 QEMU 运行的 W9 run 1 在 30 秒后无输出超时；没有足够证据归因于服务代码、
  sigaction 或宿主调度。未改代码/超时/CPU/断言，快照原 BIOS 镜像隔离复跑双轮通过。
  镜像 SHA-256 为 c841904167cc82fbc20eeb31f35b8e0093ca3bfdba7b87527bba2e2780ed72f3。
  隔离工具仅在 target/：直接编入原校验器并链接现有 runtime/ABI，临时编译配置补齐依赖
  和 panic=abort 后构建成功；没有提交新验收入口或更改正式测试。复跑通过不表示超时根因已修复。
- 路径恶意序列由 host 直接调用**生产状态机**验证；QEMU 运行合法 VFS 分片和 ring3 Hello
  六个结构负例，不宣称已经端到端注入任意恶意 IPC。纯状态机测试不覆盖映射、授权、调度或硬件。
- 暂存状态供受信任服务执行代码读写，不是安全句柄或认证 API。仍限单客户端、单在途请求，
  没有跨客户端隔离、认证会话、网络服务框架、超时/取消、并发重传或自动服务重启。
- 不修改 TTY 输入轮询、QEMU TCG 或 CPU 限速；CARGO_BUILD_JOBS=2 仅限制编译并行。
- 未修改 Lean 模型，本轮不重跑或扩充 Lean 证明；不把协议单测当作 refinement proof。
- 前序 VFS 超时、fork 包装/能力共享/子任务授权、argv 与线程/信号限制继续按原报告解释。

## 阶段与后继

这是已签阶段 4 之后、阶段 5 工作实现上的局部补强。阶段勾选、profile supported 行、
BLK-003 和组织级签署状态均不改变。当前状态仍查[实现阶段](../../../Prima%20OS%20实现阶段.md)。
扩大并发、协议版本、分片重传、共享缓冲或请求身份时重审本报告与契约。
