---
id: spec-stage7-ci-evidence
kind: specification
authority: scoped-tooling-contract
status: reviewed-for-ci-slice
lifecycle: current
updated: 2026-11-07
---

# 7-0 CI分组、锁定输入与验收身份契约

## 范围与实现前复核

承接[阶段7计划](../plans/2026-11-06-stage7-execution.md)的S7-0.2及0.3中的制品身份部分，
复用[镜像互斥](2026-11-06-stage7-artifact-exclusion.md)。本片在实现前确认：不改变OS机制、
profile、参考ELF、双轮断言和单轮30秒门限；不新增性能门槛，不关闭任何子轨。

CI使用GitHub Actions的独立Ubuntu 24.04工作区，host/lint/docs/普通BIOS/Linux/Gate/审计停止/
D14/formal分为九组。每组内部串行；矩阵最多两个作业并行，Cargo与musl构建各限2个worker。
每作业90分钟是编排上限，不替代QEMU原门限。托管runner镜像和apt补丁会变化，必须记录实际
工具版本/平台；本片只承诺锁定构建输入与可重放命令，不声称宿主或输出跨路径逐字节相同。

共同入口为`scripts/ci.py GROUP`；`--plan`只输出结构化命令，不能生成PASS证据。
`--prepare`在同一记录中先准备工具/输入；离线调用缺输入必须失败。
工作流Actions引用固定完整commit，checkout不保留写凭证，权限为contents:read。
依据：[GitHub工作流语法](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)、
[checkout固定版本](https://github.com/actions/checkout/commit/11bd71901bbe5b1630ceea73d27597364c9af683)、
[upload-artifact固定版本](https://github.com/actions/upload-artifact/commit/ea165f8d65b6e75b540449e92b4886f43607fa02)。

## 输入与状态

- Rust来自rust-toolchain.toml；源细化Rust/Lean/Aeneas来自既有toolchain.lock.json。
- musl复用原快照锁及校验helper；sbase身份沿用profile v0；Linux参考内核沿用原provenance；
  busybox包与解包ELF分别校验，ELF必须仍为D12字节；缓存vmlinuz须匹配固定deb导出的SHA256。CI下载地址与必要机器值置于`scripts/ci-inputs.lock.json`。
- 网络准备与离线验收分开。下载先在临时位置验证哈希，再安装；已有错误缓存拒绝，不静默重置或更换版本。
- 每个命令以清理后的环境独立执行，移除旧Gate case/审计site/domains/Linux ELF及Rust覆盖变量，
  再加本条显式配置；locale固定C，Cargo验收离线且locked。准备步骤才允许下载。
- formal包含主Lean、有限表与生产源码细化/变异，不允许缺Aeneas时静默成功或用生成物哈希代替证明。
- 2026-11-07干净runner整改：除docs/formal外，准备阶段在`cargo fetch --locked`后执行
  `cargo build --locked --package bootloader`。锁定的bootloader 0.11.17构建脚本另行调用
  `cargo install --version 0.11.17 --locked`取得五个BIOS/UEFI包，普通fetch不会获取它们。
  联网只发生在准备阶段，正式命令继续离线；不使用dummy bootloader或跳过镜像构建。
  Cargo/musl的2 worker限制不等于宿主全局2 worker：上游bootloader并行启动五个子Cargo，
  本片不修改上游构建器，组预算和进程组终止仍覆盖这些子进程。

## 证据契约

1. 每次运行在ignored `target/ci/GROUP/唯一运行ID/`写result.json及逐命令日志，保留旧失败记录。
2. 初态running；仅所有命令成功、输入验证及源状态核对成功时为passed。
   非零、启动异常、超时或中断均为非通过且整体非零；失败后不继续后续命令。
3. 状态原子替换；记录源revision、dirty、源文件摘要、命令argv/cwd/配置、退出值、耗时、
   实际工具版本、runner平台与并行预算。不转储整个环境或凭证。
4. 仅记录本组期间确有变化且最终成功的镜像/ELF的路径、大小、SHA-256；失败运行不能发布旧制品为本次通过。
   扫描覆盖target/prima-os根目录及Gate/审计/Linux探针使用的一层子目录，不能遗漏feature镜像身份。
   这些是验收身份记录，非签名供应链证明；最后磁盘文件可能被后续命令替换，不能把所有中间镜像当成保留制品。
5. CI要求干净checkout；本地脏树允许验证但明确标dirty。运行过程中源文件变化则拒绝判PASS。
6. 每工作区另有组编排锁；同工作区两个组不能相互穿插。镜像命令继续各自使用xtask锁；
   非CI手工任务仍须遵守独占工作区假设，不能以此主张对外部任意写入安全。
7. 超时/中断终止本命令的整个宿主进程组并等待直接子进程；QEMU自身门限保持。
8. GitHub上安装失败/缺工具/缺包应使作业失败；本地通过不记成远端CI通过。
9. 2026-11-07原始证据后继：每条命令后按[逐步串口归档契约](2026-11-07-stage7-gate-baseline.md#逐命令原始证据保留)
   保留新增/改写的guest *.log和哈希，避免下个配置覆盖同名文件。失败日志仍属诊断，
   不进入成功artifacts清单；归档I/O错误阻止组PASS。

## 验证

host测试必须覆盖：失败后停止与旧制品排除、成功的新制品身份、环境污染隔离、源变更拒绝、
宿主超时清理和同工作区组竞争。以相同入口实际运行host、lint、docs及聚焦smoke，
并检查一次缺失形式化工具的确定失败。完整BIOS/Linux/Gate/formal只在实际执行后登记PASS。
本片的工作流接线与本地验证不构成7-0退出，远端九组及基准门槛仍须独立完成。
实际通过、失败/中断及环境偏差见[实现报告](../reports/2026-11-06-stage7-ci-evidence.md)。
首次远端九组结果、嵌套bootloader准备修复及本地复核见
[2026-11-07整改报告](../reports/2026-11-07-stage7-ci-remediation.md)。
1921757干净远端九组通过，以及原始串口/探针子目录身份后继的本地证据见
[九组CI与Gate基线](../reports/2026-11-07-stage7-ci-green-gate-baseline.md)。
