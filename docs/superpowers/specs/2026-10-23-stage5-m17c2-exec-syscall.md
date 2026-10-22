# 阶段 5 / M17c-2：execve 的 syscall 契约（设计）

**日期**：2026-10-23
**状态**：设计（未实现）。前置：[M17c-1](../reports/2026-10-23-stage5-m17c1-elf-plan.md)
（`exec::plan`/`build_stack`）、M17b-4（fork/wait）。
**范围**：`execve` 的内核入口、地址空间替换、argv/envp 与初始栈；不做
动态链接/PIE/`CLOEXEC`/`fexecve`。

## 1. ABI

`SYS_EXEC_REPLACE = 55`（Prima 原生；适配层把 Linux `execve=59` 映射到它）：

```text
exec_replace(image_ptr: u64, image_len: u64, argv_ptr: u64, argv_count: u64) -> ! 或负 errno
```

- `image_ptr`：用户态 ELF 文件字节（适配层经 VFS 读入 arena）；
- `argv_ptr`：用户态 `char *argv[]`（NUL 结尾字符串指针数组）；
- 失败返回负 errno（`ENOEXEC`/`ENOMEM`/`EFAULT`/`EINVAL`）。

## 2. 内核步骤（`exec_call`）

1. **copy-in 头**：用 supervisor 读用户内存，复制 ELF 头 + 程序头（≤ 4×56+64
   字节）到内核栈；`exec::plan` 校验（静态 `ET_EXEC`、无 `PT_INTERP`、
   `PT_LOAD` ≤ 4、页对齐）。
2. **copy-in argv**：逐指针读取用户字符串（长度上限 128，总长上限 1 KiB），
   在栈上构造 `&[&[u8]]`。
3. **拆旧映射**：遍历当前 Task 的 vspace 模型条目 `unmap_4k`（`BadRoot` 视为
   已拆除；页表帧入延迟队列），清空模型条目。
4. **映射新段**：每个 `PT_LOAD` 按页 `map_4k`（user；`writable`/`executable`
   按 flags），帧从当前 Task 的映射段 `allocate_for_segment`（随进程退出
   释放）；逐页从用户 image 复制 `filesz` 字节、`memsz` 剩余部分清零。
5. **初始栈**：在固定用户栈 VA（如 `0x0000_7f00_0000_0000`，16 页）映射
   栈页；`build_stack(top, argv, &[], auxv)` 生成镜像并拷入；`auxv` 最小集
   `AT_PAGESZ`/`AT_ENTRY`/`AT_NULL`。
6. **帧**：`SretFrame` 设为 `rip = e_entry`、`rsp = layout.rsp`、`rax = 0`、
   user CS/SS、`fs_base = 0`（新程序 `_start` 自行初始化 TLS）；随后
   `sched::resume(current)` 直接进入新映像（syscall 不按普通路径返回）。
7. **审计**：`TaskExec`（新事件名，D2 追加）或复用 `TaskStateChanged`；
   失败前不产生部分映射（先拆后建的中间态仅在 `resume` 前，失败时按旧模型
   重建不可行——因此**先校验全部前置再拆**）。

## 3. 适配层 `execve`

- 打开/读取 VFS 文件到 arena（`open`/`read`/`close`）；读取上限（如 64 KiB）；
- 构造 `argv`（含 `argv[0]`）与空的 `envp`；
- 调 `SYS_EXEC_REPLACE`；成功不返回；失败返回负 errno 并恢复现场。

## 4. 证据计划

- **W7**：`stage5_exec.c` 两阶段自 exec——第一次运行（argv 无标记）读取自身
  路径（VFS 中的测试 ELF）并 `execve` 自身带 `"stage2"` 参数；第二阶段打印
  `argv[0..]`/`envp` 后退出；断言 argv 传递与地址空间替换。
- **负例**：非 ELF 文件 → `ENOEXEC`；`PT_INTERP` → `ENOEXEC`；空 argv。
- **Linux 同源**：同一 C 源在 Linux 上 `execve` 一个测试 ELF，输出逐字节
  比较（构建步骤在 xtask）。
- **host**：`exec::plan`/`build_stack`（M17c-1）+ 新的 plan 边界用例。

## 5. 边界与偏差

- 只支持静态 `ET_EXEC`，无 PIE/解释器/动态链接；
- 不做 `CLOEXEC`（无 fd 继承语义的完整实现；适配层 fd 表在新映像中重置）；
- 缓存/帧归属：新段的帧归属当前映射段，进程退出时释放；
- 本 spec 不代表 execve 可用或阶段 5 退出条件满足。
