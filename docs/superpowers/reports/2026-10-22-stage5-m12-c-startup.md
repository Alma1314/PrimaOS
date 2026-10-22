# 阶段 5 / M12：C 启动栈与退出清理（M12a/M12b）

**日期**：2026-10-22
**性质**：实施证据报告。M12a 覆盖启动栈，M12b 覆盖退出清理（自动关闭 fd、
存储服务句柄计数、`getppid`、`O_CLOEXEC` 记录）。报告不改变 profile 冻结边界，
也不勾选任何 S5-D*/S5-X*。
**上位计划**：[阶段 5 收尾与阶段 6 入口实施路线图](../plans/2026-10-22-stage5-completion-roadmap.md)
M12；设计见[M12 设计](../specs/2026-10-22-stage5-m12-c-startup-exit-design.md)。

## 1. 结论

- 服务管理器把 C 启动栈从「固定 argc/argv + 单 envp + 单 auxv」推进为：
  - 参数块支持 M5 legacy 格式与版本化 `[0xa5][2][argc][argv...][envc][envp...]`；
  - argv 上限 16、envp 上限 8，字符串按块从参数载体流式复制到 C 栈；
  - auxv 冻结 `AT_PAGESZ`/`AT_PHDR`/`AT_PHENT`/`AT_PHNUM`/`AT_ENTRY`/`AT_BASE`/
    `AT_EXECFN`/`AT_NULL`；
  - 固定探针 envp 扩为 `PRIMA_ABI=1`、`M12_ENV=one`、`M12_EMPTY=`，探针逐项取值
    并检查 auxv；Linux 参考与 Prima QEMU 默认探针轮均通过。
- `xtask` 增加参数块 host 单测（legacy/版本化/PRIMA_ABI 追加/上限拒绝）。
- **M12b 已完成（F1）**：`_exit` 前遍历本地 fd 表发同步 CLOSE；存储服务新增只读
  句柄计数查询；管理器在 C 退出后写 `EVENT_STAGE5_CLEANUP`，F1 要求
  `open_handles == 0`；`getppid` 返回管理器标识；`O_CLOEXEC` 写入本地 fd 位。
- **未完成**：独立新探针 + `ECHO_ARGS` 参数载体的组合仍触发启动回归
  （ISSUE-START-ARGS-001），故任意 argv 的 M12 端到端轮次未启用。

## 2. 变更

| 位置 | 变更 |
|---|---|
| `prima-user/src/servicemgr.rs` | `ArgsImage`/`CArgMeta`/`CArgSource`；legacy 与版本化解析；流式写栈；auxv 组装；固定探针 `FIXED_ENV` 三项 |
| `prima-user/c/stage5_probe.c` | `main` 校验三项 envp、`envp` 终止符与 auxv 字段；`stage5_entry` 布局断言随冻结布局更新 |
| `prima-user/c/prima_syscall.c` | M12b：退出前 `prima_close_all_fds`；`getppid` → 管理器标识；本地 fd 表新增 `cloexec` 位并在 `open`/`dup`/`dup3` 维护 |
| `prima-user/c/stage5_fd.c` | M12b：校验 `getppid`、`O_CLOEXEC` 读写，占满 8 个服务端句柄后直接退出 |
| `prima-runtime/src/protocol.rs`、`prima-abi/src/lib.rs` | `SERVICE_VFS_STATUS_TAG`、`EVENT_STAGE5_CLEANUP`；共享 trace 容量 15→16（清理记录置于末位，既有下标不变） |
| `prima-user/src/service.rs`、`prima-user/src/servicemgr.rs` | 存储服务句柄计数应答；管理器退出后查询并写清理 trace |
| `xtask/src/image.rs` | `pack_program_args(args, env)` 版本化块、`PRIMA_ABI=1` 自动追加、上限校验与 host 单测；Linux 宿主探针补 env |
| `kernel/src/bootstrap.rs` | `MAX_SEGMENT_FRAMES` 144 → 149：管理器 26 页，log/ns/store 各 21 页，`ls` 用例服务映像合计 149 页（含目录页），旧上限在 `install_boot_image` 早期拒绝 |
| `docs/superpowers/specs/2026-10-22-stage5-m12-c-startup-exit-design.md` | M12 设计 |

未新增内核 syscall、未改 COSIR、未改 `prima-abi` 号段。唯一的容量变更是
`MAX_SEGMENT_FRAMES` 144 → 149：M12a 的服务管理器参数解析使该映像到 26 页，
M12b 的句柄查询/清理协议使 log/ns/store 各到 21 页；`ls` 用例的 boot manifest
服务映像段实测 149 页（目录页 1 + 148）。只按实测扩容，仍按 `>` 拒绝越界，逐页归属
检查不变。`SegmentFrames` 的内联数组因此每实例增加 24 B，远低于 M8 试过的 192 页配置。

## 3. 证据

### 3.1 host

```
cargo test --workspace
PASS（全部 test result: ok，含 xtask 203 个测试）

cargo xtask lint
PASS fmt --all --check
PASS clippy -D warnings
PASS kernel builds under deny(unsafe_code) for x86_64-unknown-none
PASS kernel kernel/src has zero `unsafe` (27 files scanned)
```

新增/更新的 host 断言：

- legacy 无 envp 参数块仍为 `[argc][len, bytes]...`；
- 带 envp 的参数块有 `0xa5, 2` 前缀，envc 含自动追加的 `PRIMA_ABI=1`；
- 超过 16 个 argv、8 个 envp 或单串 255 B 时返回错误，不截断；
- Linux 宿主探针以 `PRIMA_ABI=1`/`M12_ENV=one`/`M12_EMPTY=` 运行，退出码 37，
  既有 `h`/`i`/`xyz`/`M4STDIO` 输出不变。

### 3.2 QEMU

```
cargo xtask test
（结果见 §3.3；默认探针两轮 + 既有 E1–E6/T1/F1/V1/M1/C1/L1–L3/D1/A1 全部重跑）
```

M12b 的 F1 输出（Linux 与 Prima 逐字节一致）：

```text
FD|ok=1;pid=1;ppid=1;open=1;dup=1;share=1;dup2=1;dup3=1;close=1;self=1;reuse=1;bad=1;cloexec=1;held=1
```

F1 的 `EVENT_STAGE5_CLEANUP` 必须为 `[17, 0, 0, 0, 0]`：8 个句柄在退出前由适配层
关闭，存储服务查询返回 0。F1 的共享 trace 实测第 16 条记录为
`(8, 17, 0, 0, 0, 0)`，即清理事件、C task 17、open_handles 0。默认探针的退出也会关闭遗留 fd，`ipc_ready` 的
direct/blocks 因此从 158/166 变为 160/168（queued 不变）；清理查询新增 10 条审计，
`CAPACITY` 的 `audit_used` 从 1312 变为 1322。共享 trace 容量从 15 提到 16，
清理记录固定追加在容量记录之后，既有记录下标与校验序列不变。

默认探针轮的关键见证保持：

```
CIO|task=17;fd=1;ch=104;fd=2;ch=105
PRIMA|seq=12|event=user_cow_ready|status=ok
FAULT|task=15;addr=0xa1000000000;err=0x6;faulted=1
```

容量 trace 与 `TASKCAP` 未变；固定探针 envp/auxv 检查失败会以退出码 10 或 3
结束，QEMU 验收会拒绝。

### 3.3 全量回归

容量修正后 `cargo xtask test` 全量 PASS：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 run=1 / run=2
PASS qemu bios T1 / F1 / V1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 C main / echo / time / fd / vfs / meta / cat-ls / mut / mem
PASS stage1 boot slice
```

其中 Linux 参考包含更新后的 `stage5_probe.c`（退出 37/2/3、`h`/`i`/`xyz`/
`M4STDIO`、文件 API 与 stdio）与 `stage5_fd.c`（新增 `ppid`/`cloexec`/`held`）。
默认探针的 `ipc_ready` 因退出清理变为 `53/160/168`，`CAPACITY.audit_used` 为 1322；
`segment_slots=41`、`segment_limit=48` 等其余高水位保持。F1 两轮均要求
`EVENT_STAGE5_CLEANUP=(8,17,0,0,0,0)`。

## 4. 已知回归：ISSUE-START-ARGS-001

把独立 C 探针（`stage5_start.c`）与 `ECHO_ARGS` 参数载体同时加入启动映像后，QEMU
在 `user_cow_ready` 之前失败：

- `SYS_RECV: no Ready task to switch to`（`kernel/src/syscall.rs`）；
- 或用户页故障收尾 panic（`kernel/src/cow.rs`）；
- 去掉参数载体（仅 C 探针）后默认启动通过；
- 同一参数载体配 `stage5_time.c` 在短参数块下通过，换成 M12 版本化参数块后失败。

因此问题不在单一 C 源码。初步对照排除了「固定 init 首字节被覆盖」这一假设，但
根因尚未定位。登记为 ISSUE-START-ARGS-001；不通过放宽调度/容量断言绕过。实验用的
独立探针源未接线，已从源码树移除；复现需按本节描述恢复该探针与 `ECHO_ARGS` 条目。

## 5. 边界

- 固定探针仍是 2 项 argv；M12 的任意 argv 端到端证据未完成（见 §4）。
- envp 上限 8、argv 上限 16、单串 255 B；无 `AT_RANDOM`，SSP 仍关闭。
- 退出状态由管理器在 `EVENT_STAGE5_CAPACITY` 观察；M12b 增加退出自动关闭与
  `EVENT_STAGE5_CLEANUP`。关闭是同步 IPC，存储服务不可用时退出可能阻塞；这是
  M12b 的已知边界，正式进程/退出服务留 M15/M17。
- `getppid` 是占位父标识；`O_CLOEXEC` 只记录，无 `exec` 语义。
- 未勾选 P5-BOOT-01 为 `supported`；profile v1 delta 只追加 `partial` 边界说明。

## 6. 复现命令

```sh
cargo test --workspace
cargo xtask test
# 仅 host 侧参数块/探针参考：
cargo test -p xtask
```

QEMU 产物与日志在 `target/prima-os/`；日志不是 Git 工件。
