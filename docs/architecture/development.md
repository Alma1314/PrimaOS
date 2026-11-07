---
id: guide-development
kind: guide
authority: operational
status: maintained
lifecycle: current
updated: 2026-11-07
---

# 开发、构建与验证手册

命令以[xtask 入口](../../xtask/src/main.rs)为实现来源，项目约束见[AGENTS.md](../../AGENTS.md)。以下命令从仓库根目录执行。

## 环境与输入

Rust 工具链由[rust-toolchain.toml](../../rust-toolchain.toml)锁定为 `nightly-2026-07-30`，包含 rust-src、llvm-tools-preview、rustfmt、clippy 以及裸机/UEFI targets。Cargo alias 和 artifact dependency 支持见[.cargo/config.toml](../../.cargo/config.toml)。

镜像/C 探针构建还使用 clang、llvm-ar、objdump、make、Python 3.11+、宿主 C 编译器和 shell；QEMU 验收需要 `qemu-system-x86_64`。确切参数和查找逻辑见[image.rs](../../xtask/src/image.rs)与[qemu.rs](../../xtask/src/qemu.rs)。

musl 快照不入 Git。身份、commit、校验值及获取说明见[musl 锁文件](../superpowers/compat/musl-snapshot.lock.toml)；预期路径 `target/musl-1.2.5`。构建入口[build.sh](../../prima-user/musl/build.sh)调用 Python 3.11+ 标准库实现的[build.py](../../prima-user/musl/build.py)，从锁文件重放八个替换文件并构建 `target/musl-prefix`。缓存命中前也检查快照身份和干净状态；只复制 Git 跟踪文件，跳过 ignored 的宿主生成物。缓存覆盖锁、全部补丁、构建脚本、工具版本和固定参数；构建后用 objdump 检查 libc.a 的原生陷入白名单。默认 `make -j2`，可用 `PRIMA_MUSL_JOBS=1` 等正整数调整；Rust 并行度可另用 `CARGO_BUILD_JOBS` 控制，QEMU 仍可能占满一个核。详见[musl 边界设计](../superpowers/specs/2026-10-24-musl-port-boundary-design.md)。

## 命令与边界

| 命令 | 作用 | 实际覆盖 |
|---|---|---|
| `cargo test --workspace` | Rust 单元/集成测试 | host 模型、逻辑与工具测试；不是 QEMU 启动 |
| `cargo xtask lint` | fmt、Clippy、裸机编译、内核零 unsafe | [lint.rs](../../xtask/src/lint.rs) 定义的规则 |
| `cargo xtask build` | 构建 boot images/ramdisk | 镜像可生成；不能据此声称已运行 |
| `cargo xtask run` | 交互 BIOS QEMU | 人工观察；不替代自动断言 |
| `cargo xtask test-musl --host-only` | C 适配层、取消点汇编、构建护栏与 Linux F1 参考 | 不运行 QEMU、不重建 sysroot，适合快速迭代 |
| `cargo xtask test-musl` | 上述检查 + F1/W4 各双轮 BIOS | 共享原有 30 秒门限、trace、容量和清理断言 |
| `sh prima-user/musl/build.sh --check` | 锁定输入与归档审查；缓存缺失/过期/不完整时失败 | 不构建、不下载；不是运行时兼容证据 |
| `cargo xtask test-vfs [V1|V2|V3|V4]` | V1/V2/V3/V4 的 Linux 参考与 BIOS 双轮 | 与全量 test 共用同一实现、30 秒门限和断言；便于文件服务迭代 |
| `cargo xtask test-process [W4|W5|W6|W7|W7b|W8|W9]` | fork/exec/信号相关探针的聚焦双轮 | 同一 validator、30 秒门限、stdout 和 IPC 基线，无自动重试 |
| `cargo xtask test` | BIOS 双轮与各冻结 C/开源程序探针 | 实现定义的 trace、字节流、退出码、输入注入与 Linux 参考 |
| `cargo xtask compat` | 兼容差分入口 | 当前 cow-write-fault 有界场景；不是全部阶段5 API 验收 |
| `cargo xtask linux-trace` | Linux 参考 trace 工作流 | 参数与再生成语义见 xtask main/linux |
| `cargo xtask tcb` | 特权框架/机制层代码和耦合指标 | prima-ostd/src 与 kernel/src 的限定口径 |
| `cd lean && elan run leanprover/lean4:v4.35.0 lake build` | 编译 Lean 工件与定理 | lean/README 声明的模型范围 |
| `python3 scripts/check_handle_refinement.py [--mutation-test]` | 重新提取实际 Rust 并检查生成物/源码细化/公理 | 独立安装锁定 Charon/Aeneas/Rust 及其 Lean 依赖；范围见 [源码细化报告](../superpowers/reports/2026-10-31-rust-handle-source-refinement.md)，不证明整表或机器码 |
| `python3 scripts/docs.py check` | 文档目录、ID、主题、路径与 Markdown 锚点 | 本地链接；不探测远端网页，不判定技术结论 |

## 共享镜像工件与并发

镜像命令（build/build-linux/run/test、全部test-*、compat、linux-trace）在业务输出或QEMU启动前，
持有工作区`target/prima-os-artifacts.lock`的操作系统独占锁，直到整个命令返回。
`workspace artifacts busy`表示已有合作xtask正在使用此资源；等待其结束后再执行。
锁文件释放后仍保留，不要通过删除它解锁，也没有强制绕过选项。lint/tcb不占此锁；
`test-musl --host-only`保守归入test组。

该锁不监督直接启动的QEMU或异常终止后残留的子进程，也不限制Cargo先行编译、跨工作区CPU/RAM/端口预算。
若QEMU仍占镜像，核对其归属并结束本轮残留进程，再重新验收；不要改断言或延长门限。
契约与验证见[7-0首片设计](../superpowers/specs/2026-11-06-stage7-artifact-exclusion.md)和
[启动报告](../superpowers/reports/2026-11-06-stage7-start.md)。

## CI分组与验收身份

`python3 scripts/ci.py GROUP --plan`查看命令；正式组为host、lint、docs、bios、linux、gate、
gate-audit、domains、formal，本地另有smoke（W5与compat）。直接运行要求已有锁定输入，
Cargo离线且locked；`--prepare`先准备工具与输入，需要网络及Rust/Lean缓存写入权限。
CI附加`--require-clean`；本地脏树会明确标dirty，验证期间不要编辑源码或文档。
Gate组现有六配置各双轮；G4为1000样本pre-iret内核路径，G5为128样本user-wrapper。
单独复现G5可用`CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G5 cargo run -p xtask --features gate-probe -- test-gate`；
测量边界与实际数据见[G5方法](../superpowers/specs/2026-11-07-stage7-gate-user-benchmark.md)及
[基线报告](../superpowers/reports/2026-11-07-stage7-gate-user-benchmark.md)，两种边界/样本数的数据不混算。

同工作区一次只运行一个组；每组内部串行，Cargo/musl各2 worker，组预算不替代原QEMU门限。
准备步骤会预构建锁定bootloader，以获取其构建脚本额外安装的五个BIOS/UEFI包；
仅`cargo fetch`不足以准备干净runner。上游会并行启动五个Cargo，各2 worker并非宿主全局2 worker上限。
继承的Gate/审计/人格桩/Linux ELF选择器与Rust覆盖会被清理，每条命令使用自己的配置；
形式化正式步骤禁用自动缓存下载，并要求锁定Git依赖已准备且干净。
文档PR基线可设`PRIMA_CI_BASE=<commit>`，它会增加docs的基线变更检查。

结果和原始输出保存在ignored `target/ci/GROUP/运行ID/`，失败保留并停止后续命令，
成功才列出本组新建/改写制品的哈希；缺工具不能算通过。Actions按现有保留期上传证据目录。
每条命令结束后，新增/改写的直接guest *.log另存traces/步骤编号/并记录哈希，避免后续配置覆盖。
旧日志和符号链接排除，归档失败不能PASS；成功制品扫描包含Gate/审计/Linux探针的一层子目录。
发布镜像包另按7-0工件要求制作。工作流最多2独立作业并行，host工具/runner实际版本会记录，
不声称跨宿主逐字节相同。具体契约、输入和本地实跑见
[CI契约](../superpowers/specs/2026-11-06-stage7-ci-evidence.md)与
[CI报告](../superpowers/reports/2026-11-06-stage7-ci-evidence.md)；首次远端结果与准备修复见
[CI整改](../superpowers/reports/2026-11-07-stage7-ci-remediation.md)，干净远端九组后继及原始证据修复见
[九组CI与Gate基线](../superpowers/reports/2026-11-07-stage7-ci-green-gate-baseline.md)。

## 变更对应验证

纯文档变更运行 docs 自检、生成与 check。仓库提交规范另要求 `cargo test --workspace` 和 `cargo xtask lint`；记录实际结果。行为变更按邻近用例补测试；启动、ABI 或协议改变还需更新 trace 断言并运行 `cargo xtask test`。

内核句柄/权限的维护模式见[补强设计](../superpowers/specs/2026-10-24-kernel-handle-model-design.md)：机器计数器必须检查溢出，槽与逻辑 ID 在共同入口提交；不能把“无效句柄”当成可复用状态，不能以遍历已知权限的方式忽略未知输入位。新增或改变模型迁移时，更新 [Lean 映照契约](../../lean/README.md)，同时给出失败原子性、边界反例、不变量保持与实际证明范围。

修改内核对象/调度时关注状态原子性、审计容量、资源回收与负例；改硬件入口时检查 prima-ostd 安全不变量；改兼容 API 时补 Linux 同源探针并更新 profile/台账及报告，不以一次 stdout 相同代替所有错误语义。

## 失败定位

| 现象 | 首查 | 避免的误判 |
|---|---|---|
| musl snapshot missing | 锁文件与 target/musl-1.2.5 的身份 | 缺少输入不能记录为编译逻辑失败已修复 |
| Rust 下载/target 缺失 | pinned toolchain、组件、网络限制 | 不改工具链绕过失败而不记偏差 |
| QEMU 超时/退出异常 | qemu.rs 的超时/退出协议、完整 stdout/stderr | 删除断言或延长超时不等于修复 |
| trace 不一致 | protocol.rs、输入程序、相关报告与容量水位 | 不直接刷新期望值以掩盖回归 |
| docs check 失败 | 报错文件/行号、锚点、生成物是否陈旧 | 不把全部历史文档加入忽略列表 |

运行记录模板见[报告模板](templates/report.md)。本手册不保存“最新测试总数”，以避免与运行结果产生第二份权威状态。
