# 阶段 5 / M16b-1：clone/线程退出的内核基础与 musl 阻塞点

**日期**：2026-10-23
**范围**：M16b 的第一片：内核 clone 语义、线程退出与共享 root 修复。
**结论**：内核已具备 Linux `clone` 语义的共享线程创建（复制调用者帧、`rax=0`、
`rsp=stack`、可选 TLS）、线程退出 syscall 与“线程退出不拆共享 root”的修复；
**musl `pthread_create` 仍失败**，根因是 musl 的 x86_64 `clone.s` 直接执行
`syscall` 指令（56），**绕过适配层的号翻译**，内核按 Prima 号收到未注册的 56
并返回 `ENOSYS`，musl 把任何 clone 失败统一映射为 `EAGAIN`。登记
`ISSUE-PTHREAD-CLONE-001`。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-abi/src/lib.rs` | `SYS_CLONE=48`（复制调用者帧的共享线程）、`SYS_THREAD_EXIT=49`（退出当前线程） |
| `kernel/src/syscall.rs` | `clone_call`：校验 flags 子集（VM/THREAD 必需）、复制 `current_context()`、`rax=0`、`rsp=stack`、`CLONE_SETTLS` 决定 FS base；`thread_exit_call`：非 leader 线程 `task_destroy(root=None)` + `exit_to`；**共享 root 修复**：`task_destroy_call` 对非 leader 线程传 `root=None` |
| `kernel/src/user.rs` | `register_clone_task`（复制父帧、rax=0、rsp=stack、FS base） |
| `kernel/src/bootstrap.rs` | `task_is_thread`、`process_leader` 与 host 用例断言 |
| `prima-user/c/prima_syscall.c` | Linux `clone(56)` → SYS_CLONE 的适配（flags 白名单、parent/child tid 写入、clear_child_tid 表）、`exit(60)` 的线程退出路径与 futex 唤醒 |

## 2. 关键修复：线程退出不得拆共享 root

此前 `SYS_TASK_DESTROY` 对线程也传入 `sched::root(task)`，`task_destroy_self`
会把**共享地址空间**的根帧加入延迟释放队列（W2 之所以“看起来正常”是因为主
线程很快退出）。现在 `task_destroy_call` 对 `task_is_thread` 的 Task 传
`root=None`：只清理 Task 记录/调度槽，地址空间归 Process leader 所有。

## 3. 阻塞点：musl 的裸 syscall

musl 1.2.5 `src/thread/x86_64/clone.s`：

```asm
xor %eax,%eax
mov $56,%al
...
syscall
```

它不经过适配层的 `__syscall*`，因此：
- 56 号进入内核 `dispatch_plain`（未注册）→ `ENOSYS`；
- `pthread_create` 把 clone 失败统一返回 `EAGAIN`（探针输出
  `PTHREAD|mmap=1;create=0;...;err=11`）；
- 适配层新增的 `clone` 翻译在此路径**永远不会被调用**（探针实测无 `[c]` 标记）。

即使改 musl 的 clone.s 直接发 Prima 48，还需：
1. 把参数寄存器的 `tls`（r8）放到内核 clone 期望的位置；
2. 内核写 `parent_tid`（musl 的 `&new->tid`，当前 clone 参数 rdx）；
3. 子线程从 syscall 后继续（我们的帧复制满足）；
4. 线程退出路径（musl `start` → `exit`）经适配层，已支持 clear_child_tid 的
   futex 唤醒。

## 4. 证据

- host：`cargo test --workspace` 全绿（含 `task_is_thread`/`process_leader` 断言）。
- QEMU：`cargo xtask test` **89 条 PASS**（EXIT 0），W1/W2/W3 与既有轮次全量重跑通过（线程自毁不再拆共享 root）。
- pthread 探针未接入套件：`PTHREAD` 探针源码保留在报告描述中，实测 EAGAIN；
  W4 待 musl 快照补丁后接入。

## 5. ISSUE-PTHREAD-CLONE-001（新登记）

musl 的 x86_64 `clone.s` 直接发 Linux 56，绕过适配层。修复路径：
- 在 `prima-user/musl/patches` 增加 clone.s 补丁：改用 Prima 48/49（或调用
  适配层导出的 `prima_clone_syscall`），并保持新栈/参数/`rax=0` 语义；
- 内核 `SYS_CLONE` 写 `parent_tid`（若 rdx 非空），供 musl 的 `new->tid`；
- 子线程退出经 `exit(60)` → 适配层线程退出 + clear_child_tid futex 唤醒；
- 通过后接入 W4（pthread_create/join 同源 Linux 参考）。

## 6. 边界

- 尚无 pthread_create/join 的 QEMU 证据；M16 未勾选。
- 适配层 clone/exit 代码已就绪但当前 musl 路径不经过它（保留为补丁后的入口）。
- `SYS_CLONE` 未写 parent_tid；线程退出不报告进程状态。

## 7. 本报告不代表

- 不表示 pthread 可用；
- 不表示阶段 5 退出条件已满足。
