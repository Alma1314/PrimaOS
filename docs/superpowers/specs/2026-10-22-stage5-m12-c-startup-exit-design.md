# 阶段 5 / M12：C 启动、退出与 fd 清理基座设计

**日期**：2026-10-22
**状态**：部分落地。M12a 的 auxv/envp 通道与固定探针证据已通过；M12b 的退出
自动关闭、存储服务句柄查询、`getppid` 与 `O_CLOEXEC` 记录已实现并由 F1 覆盖。
独立的「新 C 探针 + 参数载体」组合仍会触发内核启动回归（见 §2.5），暂不宣称
任意 argv 端到端完成。
**上位计划**：[阶段 5 收尾与阶段 6 入口实施路线图](../plans/2026-10-22-stage5-completion-roadmap.md)
M12；[musl 移植计划](../plans/2026-10-18-musl-port-plan.md)。
**范围**：把 M5 的固定探针形态推进为通用 C 入口，并让退出成为可观察、可清理、
可重复的路径。不引入 Process/Thread 对象，不实现 `execve`/`CLOEXEC` 语义、
不改变内核 syscall 号段。

## 1. 切片划分

| 切片 | 内容 | 状态 |
|---|---|---|
| M12a | 参数块格式、任意 argv/envp（冻结上限内）、冻结 auxv、栈布局 | 参数/auxv 代码已落地；固定探针证据通过；独立探针受阻 |
| M12b | 退出状态可观察、退出时关闭本地 fd 与服务端句柄、`getppid` 最小子集、`O_CLOEXEC` 记录 | 已实现（F1） |

两个切片共用同一设计边界：只改用户态服务管理器、C 适配层、xtask 与探针，不新增
内核对象或 syscall。

## 2. M12a：启动栈

### 2.1 参数块

M5 的参数块为 `[argc][len, bytes]...`，上限 8 项/32 字节，envp 写死
`PRIMA_ABI=1`，auxv 只有 `AT_PAGESZ`。M12 保留该 legacy 格式的**解析**路径，
并在需要 envp 时使用版本化格式：

```text
[0xa5][2][argc][len, bytes]... [envc][len, bytes]...]
```

- `0xa5` 与合法 legacy `argc`（≤16）不可能混淆；`2` 是格式版本。
- 上限 `MAX_C_ARGS=16`、`MAX_C_ENV=8`，单串长度由 u8 长度字节约束；参数载体仍是
  单页 ET_DYN，超出页边界在 xtask 构建期拒绝，解析期再次校验。
- 带 envp 时 xtask 自动追加 `PRIMA_ABI=1`（若未显式提供）；legacy 格式仍由管理器
  注入该变量。这样 M5/M6 的历史构建路径不变。
- 管理器只保留长度/页内偏移元数据，字符串从参数载体**按块流式**复制到 C 栈；
  不把参数内容复制进管理器自己的 4 KiB 栈。参数载体映射到独立临时 VA
  `0x0000_0A00_0010_0000`，与代码/数据探测页分离。

### 2.2 初始栈布局（冻结）

`rsp` 指向：

```text
[argc, argv[0..argc], NULL, envp[0..envc], NULL, auxv..., AT_NULL]
```

- 指针表从栈顶页内偏移 0 开始；字符串从表尾按 16 字节对齐后连续排列（不逐串对齐）。
- `AT_EXECFN` 指向 `argv[0]`；`argv`/`envp` 的 NULL 终止符必须存在。
- 固定探针（无参数载体）注入 `argv=["stage5-probe","stack-ready"]`、
  `envp=["PRIMA_ABI=1","M12_ENV=one","M12_EMPTY="]`；legacy 参数载体仍只追加
  `PRIMA_ABI=1`。

### 2.3 auxv

| key | 值 | 说明 |
|---|---|---|
| `AT_PAGESZ` (6) | 4096 | 页大小 |
| `AT_PHDR` (3) | `ENDPOINT_CODE_VA + e_phoff` | 程序头在映像内的绝对地址 |
| `AT_PHENT` (4) | `e_phentsize` | 至少 56 |
| `AT_PHNUM` (5) | `e_phnum` | 非零且 ≤16 |
| `AT_ENTRY` (9) | `ENDPOINT_CODE_VA + e_entry` | 入口绝对地址 |
| `AT_BASE` (7) | 0 | 静态 PIE，无解释器 |
| `AT_EXECFN` (31) | `argv[0]` 绝对地址 | 可执行名 |
| `AT_NULL` (0) | 0 | 终止 |

`e_phoff + e_phnum * e_phentsize` 必须落在映像映射页内，否则管理器拒绝创建 C 任务；
不得把不可读地址写进初始栈。`AT_RANDOM` 不在本切片：当前没有可信熵源，写入可预测
字节会伪装成安全随机数；SSP 仍关闭。

### 2.4 证据（固定探针）

`prima-user/c/stage5_probe.c` 的 `main` 在 Linux（glibc）与 Prima（静态 musl +
适配层）上验证：

- `argv` 仍为固定的 `["stage5-probe","stack-ready"]`；
- `envp` 为 `PRIMA_ABI=1`、`M12_ENV=one`、`M12_EMPTY=`，逐项取值；
- auxv 的 `AT_PAGESZ`/`AT_PHDR`/`AT_PHENT`/`AT_PHNUM`/`AT_ENTRY`/`AT_EXECFN`
  非零，Prima 侧 `AT_BASE==0`。

Linux 参考命令与 QEMU 默认探针轮（两轮）均要求退出码 37 或 QEMU 33，且既有
`CIO|`/`M4STDIO`/容量 trace 不变。`xtask` 的 `program_args` 版本化块另有 host
单测（legacy/版本化/上限拒绝）。

### 2.5 已知回归：独立 C 探针 + 参数载体

把一个新的 C 探针（`stage5_start.c`）与 `ECHO_ARGS` 参数载体组合后，QEMU 在
`user_cow_ready` 之前出现 `SYS_RECV: no Ready task to switch to` 或用户页故障
（`kernel/src/cow.rs`）。对照实验：

- 同一探针**不加**参数载体：默认启动通过；
- 同一参数载体配 `stage5_time.c`、且参数块很短：通过；参数块换成 M12 版本化块：
  失败；
- 因此不是单一源码问题，而是「新 C 探针/参数载体组合」触发既有启动路径的
  帧/任务调度回归。根因尚未定位，登记为
  [ISSUE-START-ARGS-001](../reports/2026-10-22-stage5-m12-c-startup.md)，
  未通过放宽内核断言绕过。

在该问题关闭前，M12 的任意 argv 端到端证据只到 M5 的 `echo` E1–E6；M12 自身的
固定探针覆盖 envp/auxv。不得据此宣称任意 argv/args 载体已完成。

## 3. M12b：退出与清理（已实现）

1. `prima_exit_group(status)` 在发送退出消息**之前**遍历本地 fd 表，对文件/目录项
   执行与服务端 `CLOSE` 的同步请求，最后释放 tty/null 项；错误不改变退出状态。
   实现见 `prima-user/c/prima_syscall.c` 的 `prima_close_all_fds`。
2. 存储服务新增只读状态 tag `SERVICE_VFS_STATUS_TAG`（`0x5EDC…`），应答
   `tag | client(32..39) | open_count(0..7)`。管理器在收到 C 退出消息且 tty 同步
   完成后发送查询，把 `open_handles` 写入新 trace `EVENT_STAGE5_CLEANUP`
   （args `[task_id, open_handles, 0, 0, 0]`）；查询失败保留 `u64::MAX`，验收拒绝。
   不使用“未验证即通过”。
3. `getppid` 最小子集：C 适配层把 `getppid()` 映射到管理器标识
   （`prima_ctx.manager_id`），仅保证稳定、非零、不等于 `getpid()`；正式父子关系
   留 M17。
4. `O_CLOEXEC` 在本地 fd 表新增 bit，`open`/`dup`/`dup3` 复制/设置该位；本阶段
   无 `exec` 语义，探针只验证 `open` 接受该 flag 且不影响读写。
5. F1 探针占满 8 个服务端句柄后直接退出；清理 trace 必须为 0，本地 fd 表在退出
   路径全部释放。

## 4. 安全与边界

- 不新增内核 syscall，不把 Linux 语义写入内核；参数解析与 auxv 组装都在 ring3。
- C 参数载体按只读映射；管理器写完栈后立即卸载临时映射，不长期持有参数页。
- 生成栈前校验 ELF program header 表边界，避免越界读。
- 已知边界：`MAX_C_ARGS`/`MAX_C_ENV` 是冻结上限而非无限；无 `AT_RANDOM`；
  无 `execve`，`O_CLOEXEC` 只记录；无 Process/Thread；`getppid` 是占位父标识；
  独立参数载体的启动回归未关闭。

## 5. 本设计不代表

- 不表示阶段 5 退出或 S5-D* 已满足；
- 不表示任意 POSIX 程序可用；参数/环境仍有冻结上限；
- 不表示进程/线程、signal、网络、动态链接已实现。
