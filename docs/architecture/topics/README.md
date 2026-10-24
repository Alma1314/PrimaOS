---
id: topics-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-23
---

# 按子系统检索

主题包把“规范 → 设计/计划 → 报告链 → 代码 → 测试 → 限制”放在一起。它们是解释与路由，不产生新的抽象语义或阶段签署。返回[文档中心](../../README.md)。

| 主题 ID | 入口 | 常用关键词 |
|---|---|---|
| `semantics` | [抽象语义与形式化](semantics.md) | POSIR, COSIR, CDT, Lean, 良构 |
| `boot` | [启动、硬件与可信边界](boot.md) | bootstrap, framekernel, GDT, IDT, trap |
| `memory` | [页帧、映射、TLB 与 COW](memory.md) | MemorySegment, memseg, vspace, pagewalk, TLB |
| `capability-ipc` | [Capability、句柄、Task 与 IPC](capability-ipc.md) | cap, rights, handle, generation, holder |
| `namespace-time-audit` | [Namespace、Timer 与 Audit](namespace-time-audit.md) | ns_bind, ns_mount, ns_resolve, timer, audit |
| `runtime` | [原生运行时、ring3 服务与用户态 I/O](runtime.md) | prima-runtime, servicemgr, init, service, protocol |
| `vfs` | [用户态 VFS、文件描述符与路径](vfs.md) | rootfs, fd, OFD, openat, statx |
| `process` | [进程、线程、fork、exec 与信号](process.md) | Process, Thread, ShareGroup, pthread, clone |
| `synchronization` | [等待、唤醒、futex、时间与 TTY](synchronization.md) | WaitObject, wait_objects, signal_object, futex, BlockedWait |
| `compatibility` | [POSIX profile、musl、网络与 Linux 路线](compatibility.md) | POSIX, Linux, musl, libc, profile |

## 选择多个主题的场景

- fork/COW：process + memory + capability-ipc。
- TTY 阻塞读：synchronization + runtime + compatibility。
- VFS 路径授权：vfs + namespace-time-audit + capability-ipc。
- boot ABI：boot + runtime；号段另查[接口边界](../interfaces.md)。
- 模型/真实内核差距：semantics + 对应机制主题。

## 机器读取

```sh
python3 scripts/docs.py topic process
python3 scripts/docs.py search 'fork' --kind report --limit 5 --json
```

[topics.json](../topics.json)是人工维护的精选路由；[catalog.json](../../catalog.json)是全量目录，覆盖未被精选进主题包的历史材料。新增工件即使未进入精选包，也会在重建目录后可检索。检索结果按词法相关性排列，不代表权威程度或最新状态。
