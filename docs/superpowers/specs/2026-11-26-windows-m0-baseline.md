---
id: spec-windows-m0-baseline
kind: specification
authority: windows-libos-boundary
status: bounded-baseline-m0-open
lifecycle: current
updated: 2026-11-26
---

# WIN-M0 有界基线：PE64 宿主模型与未决冻结门

## 1. 身份、来源与效力

作者在 2026-11-26 要求“推进 windowslibos”，本条构成启动
[Windows LibOS 计划书](../plans/2026-10-31-windows-libos-roadmap.md) 下一执行切片的指令来源。
计划书的路线、profile 与工作包仍为提案；本文只冻结启动 M1 宿主模型所需的输入，并把其余
WIN-D 门保持为未决。**这不是 WIN-M0 的通过或签署**：参考 Windows build、SDK/工具链、
程序白名单与逐 API 期望尚未固定，M0 的退出条件仍全部未满足。

2026-11-27 阻断处置：M0 保持开放并设 M4 硬门，M0 签署前只允许原型通道；D3b 用户 GS
与 D4 共享 root 的裁决见
[阻断处置规格](2026-11-27-windows-blocker-decisions.md)。本文不因此改变退出条件。

本文不改变[阶段跟踪](../../../Prima%20OS%20实现阶段.md)的阶段状态或勾选，不把 Windows
LibOS 变成阶段 7 必交付 personality，也不替换 Linux + Prima 原生/POSIX 组合。

## 2. 本切片冻结的输入

| 项 | 冻结值 | 依据与限制 |
|---|---|---|
| 目标文件格式 | PE32+（AMD64）、控制台子系统优先 | 计划书 §4；本轮只解析，不装载 |
| 宿主验证平台 | Linux x86_64，仓库固定 nightly `nightly-2026-07-30` | 只证明宿主 Rust/汇编行为，不外推 Windows 或 Prima guest |
| 新组件 | `libos/windows/`，`#![no_std]`、无第三方依赖、`forbid(unsafe_code)` | 库内零 unsafe；ABI 汇编只存在于集成测试 |
| M1 范围 | 头部、节表、数据目录、RVA 映射、导入/重定位/TLS 目录解析、映射计划校验 | 不含导出、delay/bound import、重定位应用、TLS 初始化 |
| 失败策略 | 越界、溢出、重叠、W^X、虚拟填充读取全部返回错误 | 不以零填充、截断或永久 W+X 掩盖坏 PE |
| ABI 探针 | `extern "win64"` 与最小汇编验证寄存器/栈参数、32 字节 shadow space 槽位、非易失寄存器 | 不验证 NT syscall、APC、SEH 或异常上下文 |
| 复用边界 | 自有薄解析层；不引入 Wine 或 Windows SDK 制品 | WIN-D2 对后续复用仍开放 |

## 3. 成功与失败契约（PE 模型）

成功：`PeImage::parse` 只接受 `MZ`、`PE\0\0`、AMD64、PE32+ 且可选头能容纳声明目录的输入；
节表与声明数据目录必须完整落在文件内。`validate_mapping` 另行要求文件/节对齐为合法 2 的幂、
节虚拟范围不重叠且不侵入头部或越过 `SizeOfImage`、入口在映像内、原始数据在文件内，并拒绝
任何同时可写可执行的节。

失败：所有结构读取都先做加法溢出与文件范围检查。RVA 位于节虚拟范围但超出原始数据时返回
`RvaNotInFile`，不得返回零字节；不在任何映射范围的 RVA 返回 `RvaNotMapped`。导入描述符、
thunk 名称、重定位块与 TLS 回调数组的越界分别报告，不静默截断。解析器不分配、不写入输入、
不执行任何字节。

## 4. 未决冻结门

| 冻结门 | 当前状态 | 启动 M2 前需要 |
|---|---|---|
| WIN-D1 版本/profile | **未决**：参考 Windows build、MSVC/CRT 版本、API-set 与程序白名单未固定 | 固定可复现参考镜像、SDK、编译器与首批导入闭包 |
| WIN-D2 复用 | **未决**：本切片只用自有薄层 | 决定是否评估选定 Wine 组件及其来源/工具链 |
| WIN-D3 装载/GS | **未决**；D3b 用户 GS 方向已裁决、实现未做 | 停车/发布、映射权限、PEB/TEB 与用户 GS 安全设计；见[阻断处置](2026-11-27-windows-blocker-decisions.md) |
| WIN-D4 共享与复制 | **未决** | 共享 root 的 Gate 绑定、参数快照与失败原子性 |
| WIN-D5 同步/异常 | **未决** | WaitAll、APC、SEH 状态机与证明范围 |
| WIN-D6 文件/服务 | **未决** | 路径、共享、删除、锁与最小 SCM 语义 |
| WIN-D7 硬化/签署 | **未决** | 配额、撤销、恢复、性能门槛与独立签署 |

本轮没有参考 Windows 环境，也没有运行任何窗口或服务程序；差分证据保持未完成，不用 Wine
代签。M0 的“每项 API 有文档、期望、限制及参考用例”义务不因本切片而缩小。

## 5. 验证与反例

已提交的验证是宿主测试：合成 PE 夹具覆盖头部、RVA、导入、重定位、TLS 与坏输入负例；
Win64 ABI 探针覆盖 8 参数、浮点位置分配、shadow space 第 5 槽与非易失寄存器保存。
另有一次性真实 PE32+ 夹具（clang + lld-link 生成，不随仓库提交）通过解析示例，
其命令、哈希与限制记录在[M1 报告](../reports/2026-11-26-windows-m1-pe-model.md)。

这些结果只证明宿主解析器与 ABI 子集，不构成 PE 装载、DLL 生命周期、Win32 API、Prima
ring3 运行或 Windows 参考差分的证据。

## 6. 后继与重审触发

- 固定 WIN-D1 参考环境与首批程序/导入闭包，补齐 M0 的逐 API 行；未完成前不把 M1 结果当作 M0 通过。
- 启动 M2 前评审用户态装载器所需的停车/发布、映射权限与用户 GS 设计（WIN-D3）。
- 若真实程序要求导出、delay import、异常目录或重定位应用，先在 M1 扩展解析并补负例，不能以成功桩替代。
- 若发现必须让内核识别 NT syscall/Windows 对象才能继续，按计划书 §9 记录反例并重审路线。
