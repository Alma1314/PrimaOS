---
id: guide-development
kind: guide
authority: operational
status: maintained
updated: 2026-10-24
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
| `sh prima-user/musl/build.sh --check` | 锁定输入与缓存检查；命中时审查归档 | 不构建、不下载；不是运行时兼容证据 |
| `cargo xtask test-vfs [V1|V2|V3]` | V1/V2/V3 的 Linux 参考与 BIOS 双轮 | 与全量 test 共用同一实现、30 秒门限和断言；便于文件服务迭代 |
| `cargo xtask test` | BIOS 双轮与各冻结 C/开源程序探针 | 实现定义的 trace、字节流、退出码、输入注入与 Linux 参考 |
| `cargo xtask compat` | 兼容差分入口 | 当前 cow-write-fault 有界场景；不是全部阶段5 API 验收 |
| `cargo xtask linux-trace` | Linux 参考 trace 工作流 | 参数与再生成语义见 xtask main/linux |
| `cargo xtask tcb` | 特权框架/机制层代码和耦合指标 | prima-ostd/src 与 kernel/src 的限定口径 |
| `cd lean && lake build` | 编译 Lean 工件与定理 | lean/README 声明的模型范围 |
| `python3 scripts/docs.py check` | 文档目录、ID、主题、路径与 Markdown 锚点 | 本地链接；不探测远端网页，不判定技术结论 |

## 变更对应验证

纯文档变更运行 docs 自检、生成与 check。仓库提交规范另要求 `cargo test --workspace` 和 `cargo xtask lint`；记录实际结果。行为变更按邻近用例补测试；启动、ABI 或协议改变还需更新 trace 断言并运行 `cargo xtask test`。

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
