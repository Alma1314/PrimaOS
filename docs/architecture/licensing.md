---
id: licensing
kind: reference
authority: license-policy
status: maintained
lifecycle: current
updated: 2026-11-08
---

# 许可证：GPLv3 与本仓库的许可分布

本文件说明 PrimaOS 的许可选择、**哪些文件不适用该选择**、第三方材料的归属，以及分发时的
额外义务。许可事实以根目录 [`LICENSE`](../../LICENSE) 全文与各 crate 的 `Cargo.toml`
`license` 字段为准；本文件是解释与登记，不改变任何文件的许可归属。

## 项目许可

PrimaOS 整体以 **GNU General Public License v3.0 或任何更新版本** 发布：
`SPDX-License-Identifier: GPL-3.0-or-later`。

| 项 | 值 |
|---|---|
| 许可全文 | [LICENSE](../../LICENSE)（逐字复制 gnu.org `gpl-3.0.txt`，674 行，`sha256=3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`） |
| Cargo 标识 | 除 `prima-ostd` 外 7 个 crate 的 `license = "GPL-3.0-or-later"` |
| 为何 "or-later" | 作者于 2026-10-29 选择 GPLv3 起、允许后续 GPL 版本（GPLv3 §14 的"or any later version"）。下游可按 GPLv3 或 FSF 发布的任何更新版本分发 |
| "or-later" 的法律含义 | GPLv3 §14：被许可方可在 GPLv3 与 FSF 发布的任何更新版本之间选择；这不改写 `LICENSE` 的条款，也不增加额外限制 |
| 如何改为 "only" | 把各 crate 的 `license` 改回 `"GPL-3.0-only"` 并在本表登记；`LICENSE` 全文不变 |

GPLv3 不要求每个源文件带许可头；本仓库以 `LICENSE` + `Cargo.toml` + 本文件三处声明为
准（声明为 "or-later"，所以 `LICENSE` 全文保持 FSF 原文，不改写任何条款）。逐文件 SPDX 头是可选加固（见[后继](#后继与未覆盖)）。

## 例外：`prima-ostd` 的三个 MPL-2.0 文件

`prima-ostd` 中移植自 [Asterinas](https://github.com/asterinas/asterinas) 的文件保留
**MPL-2.0**，不改为 GPL：

| 文件 | 许可 | 来源 |
|---|---|---|
| [`prima-ostd/src/align.rs`](../../prima-ostd/src/align.rs) | MPL-2.0 | `ostd/libs/align_ext/src/lib.rs` |
| [`prima-ostd/src/sync/lock.rs`](../../prima-ostd/src/sync/lock.rs) | MPL-2.0 | `ostd/src/sync/spin.rs` 等 |
| [`prima-ostd/src/pod.rs`](../../prima-ostd/src/pod.rs) | MPL-2.0 | ostd-pod 类型边界（重实现） |

- 上述文件首行带 `SPDX-License-Identifier: MPL-2.0`；许可全文见
  [`prima-ostd/LICENSE-MPL`](../../prima-ostd/LICENSE-MPL)（从锁定上游提交逐字复制，
  `sha256=86cf9656…`）。逐文件来源与输入的 SHA-256 见
  [Asterinas 来源与修改](asterinas-provenance.md)。
- **为什么可以混**：MPL-2.0 允许把 Covered Software 组合进按其他条款发布的 Larger Work
  （MPL-2.0 §3.3），且这些文件**没有**附带 Exhibit B 的 "Incompatible With Secondary
  Licenses" 通知（本仓库原文核对：该字样只出现在许可全文的模板段落）。因此整个产物可按
  GPLv3 分发，而 MPL 文件的源码形式仍按 MPL-2.0、且已随仓库提供。
- 其余 `prima-ostd` 文件为 PrimaOS 原创，适用 GPL-3.0-or-later。

## 第三方材料

| 材料 | 许可 | 仓库内状态 | 义务 |
|---|---|---|---|
| [musl](https://musl.libc.org/) 1.2.5（构建期快照） | MIT | **不随仓库分发**：快照在 `target/musl-1.2.5`，由 `docs/superpowers/compat/musl-snapshot.lock.toml` 锁定并校验 | 已把 MIT 通知逐字入库为 [`prima-user/musl/COPYRIGHT`](../../prima-user/musl/COPYRIGHT)（`sha256=f9bc4423…`，与锁定值一致），覆盖 `prima-user/musl/patches/` 中源自 musl 的 `clone.s`、`restore.s`、`syscall_arch.h` 等修改文件 |
| Rust 依赖（`bootloader_api` 等） | 各自许可（MIT/ Apache-2.0 等） | 不 vendored，由 `Cargo.lock` 锁定 | 若将来 `cargo vendor`，必须随之引入各自 NOTICE |
| [busybox](https://busybox.net/) 1.36.1（阶段 6 白名单） | **GPLv2** | **不随仓库分发**：只登记 `sha256` 与 ELF 属性，见[白名单台账](../superpowers/compat/stage6-static-whitelist.md) | 见下节 |
| Ubuntu 参考内核（差分 guest） | GPLv2 | 不随仓库分发（`target/linux-ref`，仅差分时下载） | 不进入仓库；不要在发行物中打包 |

## 分发约束（现在不影响仓库，影响将来的发行物）

- **GPLv2 的 busybox 一旦被放进启动镜像**，镜像就受 GPLv2 约束：必须提供对应源码（或书面
  offer）。当前仓库只引用其 `sha256`，不构成分发；发布带 busybox 的镜像前，需在发行说明
  中附源码获取方式。这与本项目的 GPLv3 不冲突（GPLv2-only 与 GPLv3-only 不能互相并入同一
  作品——因此**不把 busybox 代码拷进 PrimaOS 源码树**）。
- 本项目的 GPLv3 覆盖 PrimaOS 自身源码；`prima-ostd` 的 MPL 文件在再分发时仍需保留 MPL
  声明与源码可得性。
- 任何"安全/加固"表述不因许可选择而改变：见
  [阶段 6 启动报告](../superpowers/reports/2026-10-29-stage6-start.md)对 `AT_RANDOM` 的限制。

## 后继与未覆盖

- **逐文件 SPDX 头**：尚未添加。若添加，必须跳过 `prima-ostd` 的三个 MPL 文件与
  `prima-user/musl/patches/` 下的 musl 派生文件，并在本文件登记批次与例外。
- **发行物许可清单**：随 7-0（工程底座）的 release artifact 一并产出（源码 offer、第三方
  NOTICE、镜像内许可清单）。
- 本文件不构成法律意见；它只把仓库里已经存在的许可事实写清楚。

## 7-0 源码与验收证据包首片

[打包契约](../superpowers/specs/2026-11-08-stage7-evidence-package.md)与
[实际核验](../superpowers/reports/2026-11-08-stage7-evidence-package.md)记录首片：
指定Git提交的tracked源码、九组验收清单/日志、根LICENSE、MPL全文和musl COPYRIGHT随包保留，
并记录Cargo license原值与三个MPL文件的SPDX/内容哈希。独立Rust探针缺少license字段时原样记null，
不由工具补填；crate字段、逐文件声明与本页政策分别保留，不据字段登记裁决法律归属。

该包是源码/验收材料，未包含镜像字节，也没有纳入全部二进制组件的第三方NOTICE、
外部对应源码及构建材料，不能作为完整二进制发布许可验收。后继仍须完成这些材料、
镜像内许可清单及实际镜像身份绑定；不改变本页既有归属、分发义务或阶段退出条件。

## 7-0 根Cargo锁定输入材料

[材料契约](../superpowers/specs/2026-11-08-stage7-cargo-materials.md)及
[实跑记录](../superpowers/reports/2026-11-08-stage7-cargo-materials.md)补充根工作区68包图：
60份原始注册表.crate逐个匹配Cargo.lock，110份文本候选与原归档字节一致，并保留声明和VCS线索。
归档与材料包只落在ignored target，不作为vendor源码入库，不修改原始归档或选择许可分支。

bootloader_api、bootloader-boot-config、r-efi三个逐归档文本缺口仍保留；
前两个包与bootloader的同一VCS根文本没有被脚本自动继承为适用材料。
该图包含条件/未链接输入，且未覆盖嵌套bootloader构建图、Rust工具链/标准库、独立探针图、
musl/sbase/glibc/busybox/Linux等其它材料，不是完整镜像组件/NOTICE或二进制发布许可结论。

## 重审触发

更换许可或改成 `-only`、新增 vendored 第三方代码、发布包含第三方二进制的镜像、
或 `prima-ostd` 新增上游移植文件时，更新本文件与
[Asterinas 来源](asterinas-provenance.md)，不要静默改归属。
