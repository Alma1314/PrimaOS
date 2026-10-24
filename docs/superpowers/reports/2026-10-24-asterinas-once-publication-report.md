---
id: report-asterinas-once-publication
kind: report
authority: scoped-evidence
status: partial-validation
lifecycle: historical
updated: 2026-10-24
---

# Asterinas 参考续篇：一次初始化与启动发布验证

## 结论与范围

Once 与小型启动描述按[契约](../specs/2026-10-24-asterinas-once-publication-design.md)完成接线；
Rust929项与lint通过。完整 cargo xtask test 三次均30秒超时，未取得完整命令PASS。
最终30种BIOS用例各2轮取得分段成功覆盖（60/62轮），其中部分用例经过超时重试；这不证明连续双轮稳定性。
缺少完整成功pair的用例：W7；其失败或未完成部分仍是验证缺口。
所有失败与未覆盖范围保留，阶段5未签署、阶段6未启动、BLK-003 Open，阶段退出勾选不变。

## 基线与设计输入

直接修改干净master，起点 `26168e52fb267150ff42c254ec8619f8882e3150`。
参考本地 Asterinas `ceb65e3f850096d30283137845ac01ad51dfcdaf` 的 boot INFO/EARLY_INFO、ACPI_INFO、LOGGER Once用法；
三份文件逐字核对锁定Git blob，哈希见[来源登记](../../architecture/asterinas-provenance.md)。
未改参考仓库、联网抓取、执行其代理说明或复制spin crate；没有新Cargo依赖。
本地明确选择失败/unwind可重试；没有移植spin依赖或宣称其行为完全等价。

## 实现与后继关系

- 复用既有IRQ-safe锁和Acquire/Release发布，替代panic后永久BUSY的协议。
  get_or_try_init失败不发布；get无等待；set争用返回原值；Drop恰好销毁已发布对象。
- root、image_va、image_pages由一个Once<KernelRootInfo>整体发布；后续构建返回原根，跳过闭包与页表分配。
  首次普通Err沿用build_self_root已有owned帧清单回滚；Once不回滚任意外部副作用，也没有新增unwind回收。
- self_root_ready.published_once=1来自真实入口的全零参数重复调用、描述相等及空闲帧计数不变。
  原root/归属/页数/翻译、帧守恒/不交、输出/容量、IPC基线、stack_canary与bounds_rejected门禁保留。
  trace缺字段或非1被拒绝；旧日志不能直接通过新validator。没有syscall/能力/VFS格式/profile/Lean变更。
- [AG](2026-09-25-framekernel-global-state-report.md)的“Once未接入”仅在小型根描述处得到后继；
  REGISTRY/INSTALLED仍原地构造，避免历史大对象启动栈溢出。
  [首轮 OSTD](2026-10-24-asterinas-reuse-report.md)、[POD后继](2026-10-24-asterinas-pod-buffer-report.md)
  的历史失败、人工类型审计及恢复细化偏差不被整体取代。

## 命令、环境与结果

pinned nightly-2026-07-30 / rustc1.99.0-nightly (1a833e165 2026-07-29)，CARGO_BUILD_JOBS=2，musl默认make -j2。
QEMU8.2.2 / pc-i440fx-8.2 / TCG thread=single / qemu64 / 1vCPU / 128M / 无NIC，正式单轮时限仍30秒。
jobs限制不是QEMU CPU利用率限制；没有性能主张。
日志与临时驱动位于gitignored target/asterinas-once-validation/，不提交镜像或临时runner。

| 本次命令 | 结果与实际范围 |
|---|---|
| CARGO_BUILD_JOBS=2 cargo test --workspace | PASS929，kernel437、OSTD41、xtask207，含2个rustdoc示例、5个compile_fail；最后一次在常数时间计数调整后 |
| CARGO_BUILD_JOBS=2 cargo xtask lint | PASS fmt、Clippy -D warnings、裸机构建、29个kernel源文件零unsafe |
| CARGO_BUILD_JOBS=2 cargo xtask test | FAIL三次：default run1、T1 run2、T1 run2超时；第三次前15轮通过 |
| 未提交原后缀驱动 | 60/62轮成功分段覆盖；完整pair必须来自同一个驱动，以保留原IPC双轮基线比较 |
| target/debug/xtask tcb | PASS，见下方源码代理指标 |
| python3 scripts/docs.py self-test / build / check / check --base 26168e5 | PASS302份，最终报告与阶段更新后重新生成/检查；日志docs-*.log |

新增host覆盖并发只发布一次、完整值可见、错误/unwind后另一初始化者重试、未就绪get/set、拒绝值所有权、
空单元/已发布单元析构、IF模型的恢复以及trace缺字段/错误值；compile_fail检查借用不越过单元生命周期。
没有运行Miri、Lean或真实SMP；host测试、单核QEMU和证明属于不同证据层。

### 分段范围

从最终代码日志中逐用例选择同一驱动内的完整run1/run2，程序核对30种用例、60轮成功覆盖；缺口见开头：

| 日志 | 计入的成功轮 | 用例 |
|---|---|---|
| qemu-third-timeout.log | 14 | default, E1, E2, E3, E4, E5, E6 |
| qemu-continuation.log | 12 | T1, F1, V1, V2, V3, N1 |
| qemu-continuation-retry.log | 10 | W1, W2, W3, W6, W8 |
| qemu-continuation-w9.log | 2 | W9 |
| qemu-continuation-w7.log | 18 | W7b, W5, I1, M1, L1, L2, L3, D1, A1 |
| qemu-gap-probe.log | 4 | W4, C1 |

其他已成功的重复轮不叠加覆盖数。第一段完成musl四项host检查及default/E1–E6 Linux参考，后缀保留其余原Linux参考。
临时驱动引用仓库原image/qemu/protocol/Linux模块，从正式xtask二进制提取精确CARGO_BIN_FILE路径；
没有改正式runner、QEMU参数、30秒限制或校验断言。
隔离重试驱动只在超时后最多再试两次；成功仍须原validator通过，非超时错误立即令该组失败。
最后一个后缀驱动在保留组失败的同时运行后面的独立组；任一组失败仍使整个驱动退出1，失败组不输出成功轮。
输入探针沿用同一marker/输入脚本与原capture校验；重试次数/全部错误保存在原始日志。
临时驱动因省略前缀产生dead_code警告，没有压制；正式cargo xtask lint零告警通过。
成功轮覆盖不等于无重试稳定性，尤其不能写成完整cargo xtask test已通过。

### TCB与耦合

```text
TCB trusted_lines=1915 mechanism_lines=24451 total_lines=26366 ratio=7.26%
TCB unsafe_tokens trusted=72 mechanism=0
TCB kernel_service_coupling include_bytes=0 prima_user_dep=false service_name_lines=0 protocol_lines=39
```

该指标包含内联测试，是源码代理；不从比例推导运行时可信面缩小或安全证明。

## 失败与诊断

首次聚焦编译失败：新xtask单测漏导入父模块validator，E0425；补充use后通过，原日志focused-first-failure.log保留。

以下31次QEMU启动均触发原30秒时限。最终ERROR对最后一次失败的重复输出不另计次数；原日志保留每次输出、attempt及run上下文。

| 原始日志 | 超时次数 | 用例范围 |
|---|---|---|
| `qemu-first-timeout.log` | 1 | default run1 |
| `qemu-second-timeout.log` | 1 | T1 run2 |
| `qemu-third-timeout.log` | 1 | T1 run2 |
| `qemu-continuation.log` | 1 | W1 run2 |
| `qemu-continuation-w1.log` | 1 | W1 run1 |
| `qemu-continuation-retry.log` | 6 | W1 × 1, W6 × 2, W9 × 3 |
| `qemu-continuation-w9.log` | 3 | W7 × 3 |
| `qemu-continuation-w7.log` | 14 | W7 × 3, W7b × 1, W4 × 3, C1 × 4, L1 × 1, L2 × 2 |
| `qemu-gap-probe.log` | 3 | W7 × 3 |

第一份超时镜像隔离诊断沿用相同QEMU配置，单独给60秒采集上限，实际21.998秒退出33；
18.134秒进入内核，21.174秒frames_ready，21.979秒self_root_ready且published_once=1，21.989秒boot_complete。
诊断未调用正式validator，不算正式30秒门禁通过。首次冻结ELF误取同尺寸lint产物；
核对FAT内容发现哈希不同，原文件保留为lint-build.elf，再从冻结BIOS的FAT16链提取实际first-timeout.elf，
SHA-256 `920864b67985d623b00fc862e3f112d4775ebff2227dc71356ba773da1a3075e`，过程见frozen-elf-provenance.log。

前两次完整命令最后均到seq37，新增见证原本又做两次完整帧审计；最终改用已有free_frames常数时间计数，
原frames_ready全量守恒/不交检查保留。调整后第三次完整命令仍在T1 run2超时，最后到seq36。
后续另有bootloader或完全无输出的超时。因此删去重复工作不能当成超时根因已解决。
[旧V1/W1](2026-10-24-asterinas-reuse-report.md)、[W9前序](2026-10-24-native-runtime-service-report.md)
的超时仍按各自版本解释，本轮重试成功不追溯抹去它们。

### 冻结镜像身份

| 工件 | SHA-256 |
|---|---|
| first-timeout.img | `1849299d2a9bf9c611bd7e58bc4b895823cfb2d155de788dfb9b5993f7796f62` |
| t1-timeout.img | `2f6c88bb9b11454ad5c0db1f8278cb2aff4489dd5004516f6fc5f85f43bc737e` |
| t1-bounded-timeout.img | `f1857e634d4ad2165d19c211e81de1c48cbec315e29624d7a67f84ced1255e89` |
| w1-timeout.img | `2c1cdc84cd034ec25292d0118106e8a9ffbfeced490662351454057ef82b45f7` |
| w9-timeout.img | `67d29b5391d3334a4fb2a98ea33c2ed4a14d45a9c3e0b9364debb2c44be4973a` |
| w7-timeout.img | `9c7d9af468ba39e133b2064f26fb591a549b62f1a109272d2b607a4019cd4d74` |

## 限制与重审

初始化闭包不得递归初始化同一单元、阻塞、切换任务或不返回；NMI/异常重入、锁环、公平性和真正SMP bring-up未解决。
IF模型不替代真实IF=1中断恢复证据；panic=abort不恢复，Err/unwind不回滚任意外部副作用。
Once只保证描述发布和所有权，不让硬件页表本体不可变，不替代可信VA映射/别名/权限契约，也不构成整个OSTD soundness证明。
非静态Once现在正常销毁所存T；静态对象不会因此在关机时自动析构。
[自建根前序](2026-09-24-stage2-self-built-cr3-report.md)的可写镜像/W^X、装载与抽象image分叉保留。
初始化顺序、锁/IRQ模型、析构、描述字段、页表失败回收或上游版本变化时须重审。
