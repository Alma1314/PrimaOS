---
id: guide-interfaces
kind: reference
authority: explanatory
status: maintained
updated: 2026-10-23
---

# 接口、号段与版本边界

本页提供定位，不创建新的 ABI。源定义见[prima-abi](../../prima-abi/src/lib.rs)、[原生传输接口](../../prima-runtime/src/transport.rs)、[C 适配层](../../prima-user/c/prima_syscall.c)。

## 四种契约分开读取

| 契约 | 来源 | 作用 |
|---|---|---|
| 抽象操作 | COSIR 原语规范§4–15/§21 | 参数、权限、迁移、错误与不变量 |
| Prima 原生机器接口 | prima-abi::syscall、kernel/syscall | 当前 x86_64 调用号与分派 |
| 原生源码 API / 服务协议 | prima-runtime API_MAJOR/API_MINOR、protocol | safe 封装与服务协商 |
| POSIX/Linux 风格 C 边界 | musl 补丁、prima_syscall、profile 与台账 | Linux 风格号到原生机制/服务的适配 |

原语存在不意味着已有用户 syscall；原生 syscall 存在也不意味着同名 POSIX API 语义完整。

## 容易误读的号段

[compat-state](../superpowers/compat/compat-state.md)记录实现状态；当前常量以 `prima-abi::syscall` 为定义源。42–46 依次为 `SYS_WAIT_CREATE`、`SYS_WAIT_OBSERVE`、`SYS_WAIT_OBJECTS`、`SYS_WAIT_SIGNAL`、`SYS_WAIT_DESTROY`。51 是 `SYS_FORK`，53 是 `SYS_WAIT4`，54 是 `SYS_PROCESS_EXIT`，55 是 `SYS_EXEC_REPLACE`，56–58 是 SIGNAL/SIGACTION/SIGRETURN；52 未分配。

历史 M17 设计曾提议 `SYS_EXECVE=52`，后续[M17c-2 契约](../superpowers/specs/2026-10-23-stage5-m17c2-exec-syscall.md)采用 55 的 exec_replace。旧设计是前序方案，不能作为当前号表。

Linux 风格 `syscall(57)` 在 C 边界表示 fork，Prima 原生 57 表示 sigaction。报告中的续执行问题指前一种路径；W6 使用原生 51。检索和日志必须注明号空间。

## 版本不互相替代

- `native_abi` 的 1.0 握手只验证原生机器接口契约。
- runtime 的源码 API_MAJOR/API_MINOR 独立维护，当前仍处于 0.x。
- boot manifest、boot info 与用户 trace 布局有独立字段与版本。
- profile v0/v1/v2 是兼容范围版本；候选 `NT_RUXOS_ABI` 等名称不因出现在文档中就成为 Prima 已采纳接口。

## 变更流程

先确认抽象语义与归属，再同步号段定义、内核分派、runtime 封装、C 头/适配层和 musl 裸 syscall 补丁，补充正负例与 trace。号段只追加、不复用空洞；若工作决定仍待组织级确认，保留该状态并在阶段跟踪登记。

编写新接口设计使用[设计模板](templates/spec.md)，记录前置授权、成功/失败状态、容量影响、阻塞/唤醒与兼容偏差。
