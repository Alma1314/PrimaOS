# Prima OS

PrimaOS 是研究型操作系统项目，探索以小型对象能力内核支撑可替换的用户态 OS personality。仓库包含 POSIR 抽象规范、cosir 可执行模型、Lean 工件、Rust x86_64 内核、原生运行时与 C/musl 适配原型。模型、测试、QEMU 执行和证明各有不同范围。

版本 1.0.0 之前不承诺向后兼容；`prima-abi` 仍是实验性机器接口。

## 从任务开始

| 目的 | 入口 |
|---|---|
| 了解与使用文档 | [文档中心](docs/README.md) · [首次阅读路线](docs/architecture/getting-started.md) |
| AI 接手、检索和交接 | [AI 检索协议](docs/architecture/ai-retrieval.md) · [llms.txt](llms.txt) |
| 查看当前阶段、阻塞和证据 | [实现阶段跟踪](Prima%20OS%20实现阶段.md) |
| 查设计与实现 | [子系统主题地图](docs/architecture/topics/README.md) · [系统与源码地图](docs/architecture/system-map.md) |
| 查权威契约 | [架构白皮书](Prima%20OS.md) · [COSIR 原语规范](COSIR%20原语规范.md) |
| 构建与验证 | [开发手册](docs/architecture/development.md) · [文档维护](docs/architecture/documentation-maintenance.md) |
| 全库检索 | [全量目录](docs/architecture/catalog.md) · [catalog.json](docs/catalog.json) |

## 仓库结构

| 路径 | 职责 |
|---|---|
| `cosir/` | 平台无关状态机、机器投影与细化设计实验 |
| `kernel/` | 对象能力与 x86_64 机制编排，源码零 unsafe |
| `prima-ostd/` | 特权硬件框架与必要 unsafe 的受控边界 |
| `prima-abi/` | 实验性原生 syscall、boot/trace 共享布局 |
| `prima-runtime/` | 原生用户态 API、传输抽象、服务协议 |
| `prima-user/` | ring3 服务/应用/驱动、C 适配层与 musl 补丁 |
| `prima-initgen/` | init 可执行启动输入生成 |
| `xtask/` | 镜像构建、QEMU 验收、差分与门禁 |
| `lean/` | [Lean 模型与映照契约](lean/README.md) |
| `docs/`、`scripts/docs.py` | 文档、主题路由、离线索引与校验 |

## 常用命令

使用仓库锁定的 nightly 工具链，从根目录执行：

```sh
cargo test --workspace
cargo xtask lint
cargo xtask build
cargo xtask test
python3 scripts/docs.py topic process
python3 scripts/docs.py search 'fork' --limit 5
python3 scripts/docs.py check
```

镜像构建需要额外 C/musl 输入，QEMU 验收需要宿主工具；完整前置与失败定位见[开发手册](docs/architecture/development.md)。

设计说明预期，计划记录步骤，报告保存实际结果。当前退出状态只查阶段跟踪；不能将局部 PASS 外推为完整 POSIX/Linux 兼容、真实硬件普遍保证或完整内核细化证明。
