---
id: spec-static-program-contract
kind: specification
authority: proposed-compatibility-contract
status: working-design-scoped-validation
lifecycle: current
updated: 2026-10-27
---

# 静态程序的 persona 与适配层版本护栏

承接 [profile v2](2026-10-25-stage5-profile-v2.md)、[M21](../reports/2026-10-23-stage5-m21-exit-review.md)
及[路线图](../plans/2026-10-22-stage5-completion-roadmap.md) D2/D4/D6。
本设计补齐可审阅的静态护栏，不发布 profile，不替代 S5-D7 组织级评审。

## 程序声明与验证点

静态 C 和 Rust 探针 ELF 携带一个可分配的 `.note.prima.abi`，并通过 `PT_NOTE` 可读。
owner 为六字节 `Prima\0`（补齐到八字节），type 为 `0x5052494d`。
24 字节小端描述依次为六个 u32：布局版本 1、原生 major 1、要求的原生 minor 0、
persona、适配层 major、适配层 minor。persona 0 表示原生程序，1 表示本仓库静态 POSIX 子集；
其它值拒绝。C 使用 persona 1 / adapter 1.0；Rust原生R1/R2使用 persona 0 / adapter 0.0。
[POSIX std后继](2026-10-27-posix-std-backend-design.md)的R3走musl/适配层，要求persona 1 / adapter 1.0。
profile 的提案/冻结状态不编码成 ABI 版本，也不能从 note 推导 API 支持。

构建工具必须拒绝缺失、重复、截断、版本不匹配或与所选程序 persona 不符的声明。
C `execve` / `prima_exec_image` 在分配交接段、提交根替换和关闭 CLOEXEC 之前检查目标声明，
只接受 persona 1 / adapter 1.0；失败返回 ENOEXEC 并保留原进程及 fd。
紧凑汇编 ELF 见证显式声明同一交接布局；这不是实际执行 musl crt 的证据。
完整 C ELF 的新 crt 检查版本化的只读适配层锚点后再导入交接状态、初始化 musl。

## 符号版本

`prima_libc_contract_v1_0` 是 crt 对适配层的强链接依赖。升级破坏性适配层布局时必须改名；
旧 crt 缺少对应锚点会链接失败。`prima_libc.map` 明确列出 `PRIMA_LIBC_1_0` 的适配入口，
其余输出符号局部化。它限制本次静态产物的符号表，不建立动态链接器的运行时 GNU 版本解析。
不宣称存在共享库、`.gnu.version_d`、ld.so 或未来版本的动态二进制兼容保证。
完整 musl 可链接符号仍受逐 API 台账约束；脚本中的入口集合不是 POSIX 支持清单。

## 安全和范围

note 是程序自述及工具/适配层兼容检查，不是身份认证、能力授权或内核 persona 对象。
内核 55/62/69 继续处理 OS 中立 ELF/寄存器/地址空间；直接使用这些原生机制可以绕过 C 护栏。
本设计没有 AS persona、双 personality 根视图、Linux ELF 执行或运行时 persona 切换。
原生 1.0 握手不进行逐功能协商；新调用 59–72 仍要求与本工作树配套的内核和运行时。
无 note 的旧 C 目标不属于新版 C exec 支持集，不能静默当作 adapter 1.0。

## 统一验收入口

Rust 纯解析器负例、C 同格式解析器负例、C exec 失败原子性与新 crt 见证、R1/R2 及构建产物
note 检查已纳入本轮统一测试并通过；W7/W7b及R1/R2双轮通过，具体范围和失败链见[统一报告](../reports/2026-10-25-stage0-5-gap-followup.md)。S5-D7静态护栏边界仍待最终范围确认，不发布profile。
