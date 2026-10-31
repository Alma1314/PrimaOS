---
id: report-stage6-main-merge
kind: report
authority: scoped-integration-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# 阶段6主分支整合：输入、容量与冻结台账

## 基线与范围

整合工作分支 `e238a74`（其前序为 `06f40dc`）与主分支 `e69c2b9`，共同基线为
`b0d78cf`。保留主分支的许可、VFS 内容隔离、musl 检查护栏、现有 ABI/权限台账、
共享 auxv 键源及 Handle 源码细化；保留工作分支的按需 ELF 输入、退栈回收与私有640页容量。
本报告只记录整合冲突及验证，不完成 Gate、Linux LibOS 或阶段6退出条件。
前序失败与证据分别见[冻结台账](2026-10-30-stage6-freeze-ledgers.md)、
[按需输入](2026-10-31-stage6-streamed-elf-input.md)、
[容量与回收](2026-10-31-stage6-exec-frame-capacity.md)、
[VFS 整改](2026-10-31-stage1-5-regression-remediation.md)及
[源码细化](2026-10-31-rust-handle-source-refinement.md)。

## 冲突处理

- `exec_call` 保留按需输入契约，不重新引入主分支当时的120 B～4 MiB字节窗口。
  文件声明长度与物理装载页数不同；地址加法溢出仍返回EFAULT，格式/声明边界错误仍为
  ENOEXEC，实际读取不可达页仍为EFAULT。旧窗口的辅助函数和测试一并删除，避免死台账。
  ELF格式、8段、W^X、私有640页（含8页栈）及全局资源准入继续有效。
- 保留 `prima_abi::auxv` 的共享常量、键序和取值口径，两条装载路径继续引用同一源；
  这不合并两条路径的整个栈布局。现有调用号/权限位序冻结；73–78及新Gate权利仍未发布。
- 可选真实ELF解析检查保留流式/完整输入一致性，并将旧“526页大于普通149页”
  的阻塞断言改为“526载入页+当前8页栈可容纳于私有640页”；不把解析当成实际运行。
- 阶段跟踪与白名单合并两侧证据，保留所有历史报告；生成目录从源文档重建。

## 命令与证据

| 命令/检查 | 本次实跑结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1066 passed、0 failed、1 ignored（外部ELF样本） |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机编译及36个内核文件零unsafe |
| `python3 prima-user/musl/test_port.py` | PASS：7 tests |
| `PRIMA_ELF_SAMPLE=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo test -p kernel --lib plan_accepts_operator_supplied_static_linux_elf -- --ignored` | PASS：1 test；样本SHA256仍为冻结值 `dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14`；仅解析/预算断言 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：单次完整36程序、72/72轮BIOS QEMU，含V3/W7/W9/R3及Linux参考；无失败/超时 |
| 文档 self-test/build/check/check --base e238a74 | PASS：346文档；最终目录与链接检查 |

结果汇总见上表，必要的原始判据见[关键输出摘录](#关键输出摘录)。该轮所有命令退出0，无整合测试失败。

Rust使用仓库固定nightly-2026-07-30；依赖离线。
QEMU沿用BIOS/TCG/qemu64/单CPU/128 MiB及30秒门限。

Handle生产源码、提取包装器、生成Lean及证明与主分支完全相同。已核对清单中的5个
源码/工具链/生成物哈希；当前工作区未安装固定Charon/Aeneas，因此本次不重新提取或
重跑Lean，证明结果仅引用前序源码细化报告，不将哈希核对宣称为新证明。

## 阶段影响与限制

阶段6仍进行中，5项交付及4项退出条件均不勾选。大文件VFS来源、冻结制品的实际
4段/BSS装载与8页初始栈、可信停车入口、Gate/G1–G3及Linux LibOS仍待完成。
审计停止14轮与D14 opt-in原型未在本整合重跑；对应结论仍限前序报告。

## 关键输出摘录

以下片段来自本报告原验证记录，仅内嵌支撑结论的输出；本次文档整理未重跑这些验证。
完整旧附件的恢复版本及目录见[日志整理记录](2026-10-31-documentation-log-inline.md)。

原 `validation.txt` 的关键行：

```text
PASS kernel kernel/src has zero `unsafe` (36 files scanned)
test exec::tests::plan_accepts_operator_supplied_static_linux_elf ... ok
PASS full BIOS suite: 36 programs, 72 runs
```
