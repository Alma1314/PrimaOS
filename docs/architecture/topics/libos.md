---
id: topic-libos
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-11-26
---

# LibOS 布局、personality 边界与验证入口

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

新增或修改一个 OS personality 应该动哪个 crate？Linux 与 Windows 的源码、探针和构建资产分别在哪？哪些逻辑必须留在中立层，哪些只能留在服务侧？如何用现有 xtask 入口验证？

检索别名：libos, personality, prima-linux, prima-windows, winpe, PE64, musl, C adapter, 多 LibOS, S6-X3。

## 权威章节与阅读顺序

白皮书§4.3/§7（LibOS 边界与多 LibOS 模型）、[仓库约束](../../../AGENTS.md)与[系统地图](../system-map.md)。先查[阶段跟踪](../../../Prima%20OS%20实现阶段.md)确认 personality 是否属于当前阶段必交付，再读计划/规格与报告链；计划里的“未实现”属于当时基线。

Windows 是独立候选扩展，不替换阶段 7 的 Linux + Prima 原生/POSIX 组合，也不构成第三个必交付 personality。其 M0 尚未通过。

## crate 与资产归属

| 位置 | crate / 资产 | 职责 | 不放入 |
|---|---|---|---|
| `libos/linux/` | `prima-linux`（lib + `prima-linux` bin，`linux-probe` feature） | Linux 号/errno/stat/dirent、代理 Store 会话、C 适配层、musl 补丁与差分探针 | 内核授权判断；其他 personality 的策略 |
| `libos/windows/` | `prima-windows`（`winpe-probe` feature） | PE32+ 解析/映射计划、平铺重定位/IAT、WIN-M2 ring3 原型与 WINPE 探针 | 完整 Win32/CRT 已实现的宣称；未冻结的 NT ABI |
| `prima-user/` | 原生服务与程序 | servicemgr、store/log/ns/tty/driver、代理会话表、原生探针 | personality 语义解释与 fixture 根策略 |
| `prima-runtime/` | 原生 API 与中立协议 | `bulk_exec`、`protocol` 容量常量、vfs、transport | personality 策略与 fixture 名称（S6-X3 门禁） |

依赖方向：`libos/* → prima-runtime → prima-abi/prima-ostd`；`prima-user → libos/linux`（仅代理客户端）与可选的 `libos/windows`（`winpe-probe`）。禁止 `libos/* → prima-user` 的反向依赖。

共享逻辑的放置判据：能被多个 personality 复用、且不含 personality 词与根策略的代码放 `prima-runtime`（如批量执行器、容量常量）；服务侧会话与策略留在 `prima-user`。把含 Linux fixture 的代理表移入中立层会触发 `cargo xtask lint` 的 S6-X3 检查。

## 源码定位

- [prima-linux manifest](../../../libos/linux/Cargo.toml)、[linux.rs](../../../libos/linux/src/linux.rs)、[linux_store.rs](../../../libos/linux/src/linux_store.rs)、[C 适配层](../../../libos/linux/c/prima_syscall.c)、[musl 构建](../../../libos/linux/musl/build.sh)
- [prima-windows manifest](../../../libos/windows/Cargo.toml)、[pe.rs](../../../libos/windows/src/pe.rs)、[load.rs](../../../libos/windows/src/load.rs)、[winpe.rs](../../../libos/windows/src/winpe.rs)
- [批量执行器](../../../prima-runtime/src/bulk_exec.rs)、[协议常量](../../../prima-runtime/src/protocol.rs)、[代理会话表](../../../prima-user/src/proxy.rs)、[服务管理器](../../../prima-user/src/servicemgr.rs)
- [xtask features](../../../xtask/Cargo.toml)、[镜像构建](../../../xtask/src/image.rs)、[S6-X3 检查](../../../xtask/src/lint.rs)

## 构建与验证入口

- `cargo test --workspace`：crate 内联测试，含 PE/ABI、Linux 语义与 S6-X3 personality 边界。
- `PRIMA_WINPE_PROBE=1 cargo run -p xtask --features windows-pe-probe -- test-winpe`：两轮 QEMU，WIN-M2 见证。
- `PRIMA_LINUX_ELF=<pinned busybox> cargo run -p xtask --features linux-probe -- test-linux`；同族还有 `test-linux-proxy` 与 `test-libos-bench`。缺少冻结输入时保持未验证，不用 host 测试替代。
- `cargo xtask test`：完整 BIOS 36 程序/72 轮，含 Linux 同源参考与 C 探针。
- `cargo xtask test-musl [--host-only]`、`cargo xtask lint`、docs 自检。

## 契约、计划与证据

- [阶段 7 范围裁决](../../superpowers/specs/2026-10-31-stage7-scope-decisions.md)：Linux + Prima 原生/POSIX 为已确认组合。[LibOS 数据通路](../../superpowers/specs/2026-11-10-libos-data-path.md)与 [VFS/LibOS 基础](../../superpowers/specs/2026-10-23-vfs-libos-foundation-design.md)限定共享服务与前端职责。
- [Linux profile v1](../../superpowers/compat/stage6-linux-profile-v1.md)：逐行 partial 的冻结范围；不表示完整 Linux/POSIX 兼容。
- Windows：[路线计划](../../superpowers/plans/2026-10-31-windows-libos-roadmap.md) → [WIN-M0 基线](../../superpowers/specs/2026-11-26-windows-m0-baseline.md) → [WIN-M1 PE/ABI](../../superpowers/reports/2026-11-26-windows-m1-pe-model.md) → [WIN-M2 前置平铺](../../superpowers/reports/2026-11-26-windows-m2-preflight.md) → [WIN-M2 ring3](../../superpowers/reports/2026-11-26-windows-m2-ring3.md)。

## 限制与修改注意

- Windows 没有参考 Windows 差分，WIN-D1–D7 未决；WIN-M2 的替代导入是手写桩，不能推出真实应用兼容。
- Linux 现有限制以 profile v1 和阶段 7 报告为准；未修改动态 ELF 与完整 POSIX 不在本主题承诺内。
- 本页是布局与路由，不是实现状态或签署证据；新增 personality 先写规格与冻结门，再动 crate，并保留旧 QEMU 事件或逐条登记变化。
