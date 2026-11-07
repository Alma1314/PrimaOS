---
id: report-stage7-ci-evidence
kind: report
authority: scoped-implementation-and-validation
status: ci-wiring-locally-verified-with-limits
lifecycle: historical
updated: 2026-11-07
---

# 7-0 CI分组、锁定输入与验收身份

> 2026-11-07后继：[首次远端CI整改](2026-11-07-stage7-ci-remediation.md)记录九组真实结果
> （docs/formal通过、七组嵌套bootloader依赖缺失）及准备修复。下文“远端尚未执行”保留为本报告当时状态。

## 结论与范围

承接[首片工件互斥](2026-11-06-stage7-start.md)，本轮完成S7-0.2的九组工作流接线与本地运行器，
以及S7-0.3的验收身份记录部分。host、lint/TCB、W5/compat smoke、Linux同ELF专项和既有形式化证明链已通过统一入口实跑。
远端GitHub Actions尚未执行，不能标记CI九组全绿；7-0与其它八轨退出仍未勾选。
五项基准的清单、方法和实测门槛仍待后继冻结；本片不增加OS语义或扩大profile。

基线为`d171139014ab9b8aa0b67fef3a5d79ae93a00c3a`，分支`codex/stage7-foundation`；
本地验证明确记录dirty，而非将未提交状态冒充干净发布提交。依据为
[实现前CI契约](../specs/2026-11-06-stage7-ci-evidence.md)、
[执行计划](../plans/2026-11-06-stage7-execution.md)及[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)。

## 实现

- [.github/workflows/ci.yml](../../../.github/workflows/ci.yml)分host/lint/docs/bios/linux/gate/gate-audit/domains/formal九组，
  独立Ubuntu 24.04工作区、矩阵最多2作业、每组内部串行；Cargo/musl各2 worker，每作业90分钟。
  checkout/upload-artifact均固定完整commit，权限contents:read，不保留checkout写凭证；失败也上传证据目录。
- [scripts/ci.py](../../../scripts/ci.py)复用既有xtask/验证器，不更改双轮或原QEMU门限；
  单组有额外墙钟预算。`--plan`仅展示，`--prepare`先校验/准备锁定输入，然后Cargo离线且locked执行。
  清除继承的Gate/审计/人格桩/Linux ELF选择器与Rust覆盖变量，再加入每条命令的显式配置。
- [输入锁](../../../scripts/ci-inputs.lock.json)沿用profile v0的sbase commit、D12 busybox ELF与既有参考内核身份；
  新增busybox下载包hash，并复用musl与源细化锁及校验helper。错误缓存/脏输入拒绝，不静默重置。
- `target/ci/GROUP/唯一运行ID/`原子写result.json和逐命令日志；记录revision/dirty/源摘要、命令/退出值/耗时、
  实际工具版本、平台和输入身份。源码在验证期间变化则拒绝通过；CI额外要求干净checkout。
- 仅成功组记录运行期间新建/改写的镜像和ELF哈希；失败不发布旧制品。组级宿主锁防止同工作区两个运行器交叠，
  子xtask仍持镜像锁。超时/中断杀死本命令宿主进程组并等待直接子进程；不构成7-G恢复证明。

## 本轮实跑

原始输出只在ignored `target/ci/`及`/tmp/prima-stage7-ci-*.log`；下表运行ID可对应本地manifest，
不提交日志、JSON运行附件或镜像。

| 命令 | 退出/结果 | 证据范围 |
|---|---|---|
| `python3 scripts/test_ci.py` | 0；8项通过 | 非零即停止、失败保留/旧制品排除、新输出hash、环境隔离、源变化/dirty拒绝、超时杀子进程组、组竞争、错误缓存/脏checkout拒绝、形式化缺依赖不下载 |
| `python3 scripts/ci.py host` | 0；passed，运行ID`9c2657c99a284f80a05afe8167852448` | Rust1135通过/0失败/1默认忽略；musl7、POSIX std3、CI运行器8项Python检查通过 |
| `python3 scripts/ci.py lint` | 0；passed，`91fc423eecbf4f0fa2b9c4d9bab144e3` | fmt/Clippy、裸机构建、41文件零unsafe、personality守卫；TCB 2087/40871=5.11%（限定源码计数） |
| `python3 scripts/ci.py smoke` | 0；passed，`afd8b92bbeb547eea73983c7d2978119` | W5原判据双轮；compat cow-write-fault通过；记录本组新镜像/ELF身份，不是全BIOS72轮 |
| `python3 scripts/ci.py linux --prepare`（允许联网/工具缓存写入） | 0；passed，`dc33cb891a544925952127c13caebbde` | 固定输入准备成功；pinned Linux guest参考及11案例×2轮；stdout/stderr/exit/audit全部为1 |
| `python3 scripts/ci.py linux`（最终缓存核对） | 0；passed，`19b77ab1a2e54ccba348a73ce824e8c4`，239.871秒 | 参考内核缓存字节与固定包导出的hash匹配；11案例双轮及全部原判据通过 |
| `python3 scripts/ci.py formal`（准备前） | 1；failed，`eac55df08b254a7d86c8491048812969` | 缺target/aeneas/aeneas时在证明命令前拒绝；steps为空，没有假PASS |
| `python3 scripts/ci.py formal --prepare`（实际依赖缓存） | 0；passed，`54c268b5268d4ef3b1299932596d4f1e` | 主Lean、有限槽表、生产源码重新提取/新鲜度/5条定理的公理审查及generation重置变异拒绝通过；既有有界证明范围 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base d171139` | 全部0；379文档 | 元数据/目录/链接、基线变更守卫 |
| `PRIMA_CI_BASE=d171139 python3 scripts/ci.py docs` | 0；passed，`a0ab616bde8d4d1894c9e302b8e30bad` | 同一入口四项门禁及源状态核对；后续报告补录后再次检查 |

本地PyYAML既有安装按BaseLoader解析工作流，九组与max-parallel=2检查通过；运行器不依赖PyYAML。
host/lint与最终Linux验证时源摘要为`1e4678a7ad03bcd4e2d5672ed9ef6363d84b3c2bc3ee5a1439652cf9d8d5eca7`；
成功formal验证时为`18414af80fc7fdff38be69a2bad029c0a4a98b916627df71f7b13b7aab41b3d5`；
smoke较早版本源摘要为`32c619227e345f78e22578e8d7fba9bb911786652058a3b60b65e1f6816f4199`。
Linux运行时较早版本源摘要为`58d32b8b9f16dfdf8807e18cc5f37c1dcc61653cf50217934747e8480c1815a7`；
后续只收紧宿主运行器的工具版本记录、准备禁用manager自更新、源执行位摘要、文档检查、
形式化依赖/缓存准备、Linux缓存内核字节核对及测试输出，
OS/参考ELF/11案例判据未改。报告和最终文档自身后续变化不回填历史manifest摘要。

相关输入/输出身份举例（完整清单由各自manifest保存）：

| 项 | SHA-256 |
|---|---|
| 下载busybox deb | `944b2728f53ceb3916cec2c962873c9951e612408099601751db2a0a5d81e0ed` |
| 固定deb内vmlinuz与已验证缓存 | `cee36e261749483e951c8229eb1aeeb768af6c28f493421a52b3916885ce913f` |
| D12同ELF | `dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14` |
| Linux组stage6-matrix.elf | `a6e53a6b28385bb55d4561219687a2d27a381dc82829597fcf451452f1851216` |
| smoke最终prima-os-bios.img | `49fc5d31474a4a7d62ea8aea2eb338a64f936b424a84f38ea4a907ab3a94882b` |

这些哈希是对应本次验收文件身份，不保证不同宿主、绝对路径、编译器补丁或后续覆盖后文件相同；
UEFI镜像被生成不等于UEFI执行验收。manifest只保存组结束时仍存在的新输出，不声称保留全部中间镜像。

## 失败与限制

最终复核发现Linux参考缓存原先只记录vmlinuz摘要，没有比较固定包内字节。
用`dpkg-deb --fsys-tarfile`流式读取已固定SHA512的deb中vmlinuz，冻结上表SHA256；
正式输入预检/结束核对复用下载缓存的共同hash校验。以临时坏内核路径注入校验入口的
`python3 -B`反例退出0，确认`input_identity("linux")`在任何Linux命令前拒绝hash mismatch，
没有改动真实缓存。新增核对后的完整Linux双轮见上表，原ELF与观察判据保持。

正式formal准备为237.113秒，后三项检查分别0.766/5.788/40.721秒，总285.944秒；
9项Git依赖均按根manifest校验。前序整改中因缺工具未重跑源码细化的观察，由本次相同生产输入
重新提取及变异检查补充；证明范围仍为既有构造器/代次核心，不扩展为整内核证明。

1. 首次Linux准备在受限沙箱退出1（`2aaa84d98f3142b4b30d5403053cdc42`）：rustup的wait-timeout
   报`bad error on write fd: Operation not permitted`并abort；后续命令未运行、artifacts为空。
   在授权网络/工具缓存写入环境重跑才获得上表通过结果，不能抹去首次失败。
2. 首次成功准备中rustup默认把manager从1.29.2更新到1.29.3，固定Rust编译器仍为仓库版本。
   后继已为两个toolchain install增加`--no-self-update`；实际manager版本仍如实记录，不伪称锁定全部宿主工具。
3. 首次formal准备（`d7bfe905872b4dd18d3e7f3314b2b638`）在固定归档/二进制已校验、Git依赖已取齐后，
   因默认缓存包含9031项而主动SIGTERM中断；退出1、status=interrupted、子步骤130、artifacts为空。
   未运行证明命令；保留失败/中断记录。后继改为锁定后端的32项直接外部import及传递依赖，
   移除重复的子命令600秒计时器，统一由组预算终止整个进程组；正式步骤固定LAKE_NO_CACHE并预检全部Git依赖。
   离线探测Rust标准库完整prefetch另有退出101（本机缺rand_xorshift），不把现有缓存当成BIOS全量离线准备证明。
4. 没有本次远端工作流运行，也未用新入口重跑完整BIOS、Gate、审计停止、D14；
   阶段0–6历史全量结果仍只属于其原报告。本片验证不等于这九组在干净托管runner上全绿。
5. apt/托管runner镜像会变化，实际版本记录不构成逐字节可复现或供应链签名；
   本地已有缓存不是全新机器安装证明。锁是合作宿主编排，手工外部写入及直接启动进程不在完整保证内。
6. 尚无五项性能基线、发布许可清单或性能门槛，未关闭G6/G8或7-0；下一步先收集远端九组实际证据，
   再冻结五项基准方法与单核基线，按九轨依赖推进真实机制。
