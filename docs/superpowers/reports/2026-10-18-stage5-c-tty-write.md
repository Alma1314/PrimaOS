# 阶段 5 片 C：tty 字节流、`write(1/2)` 与同源 `_exit` 见证

**日期**：2026-10-18。**范围**：片 C 的第一个纵向闭环——同一份 C 探针源码在 Linux 宿主与
Prima QEMU 上都经 `write(1)`/`write(2)` 各写一个字节，并以返回码 37 退出；不代表 libc、
POSIX fd 表、通用 `_exit`/`wait` 或文件系统。

## 实现

- `prima-user/c/stage5_probe.c`：Linux 分支用宿主 `write`/`exit_group`；Prima 分支无 libc，
  每个字节以 `STAGE5_TTY_WRITE_TAG`（fd、字节、发送方 task id）经 `SYS_SEND` 直投服务管理器。
  `main` 在两端收到相同的 `argv[1]="stack-ready"` 与 `PRIMA_ABI=1`，执行相同的两条 `write`
  后返回 37；失败分支返回 2/3/4。
- `prima-user/src/tty.rs`（新二进制 `prima-tty`，映像 kind `0x102`）：管理器运行期铸一块覆盖
  COM1 的 `Device(IOPortRange)` 并移交本服务；本服务把收到的字节经 `dev_ioport_out` 写到
  `0x3F8`，随后对同一设备做 Scratch 写回读与区间外写拒绝两条自检，最后回 ACK。
- `servicemgr`：按目录 kind 创建 tty 服务（早于 C 探针），记录并转发两条 write，独立打印
  `CIO|` 见证；收尾条件加入 tty ACK。
- `_exit(status)`：C 探针把 `main` 返回码随 `STAGE5_C_PROBE_TAG` 发给管理器，随后
  `SYS_TASK_DESTROY(self)`。这是当前原型的最小退出语义，不是 Process/父子等待。
- tty 服务与管理器各自持有一块独立的 COM1 设备 Capability，互不转移。

## QEMU 见证（`cargo xtask run`，串口原文）

```
CIO|task=17;fd=1;ch=104;fd=2;ch=105
TTYIN|task=17;fd=1;ch=104;fd=2;ch=105
TTYOUT|task=17;in_magic=1;out_of_range=1;out_ok=1
TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=123
CAPACITY|caps_live=123;caps_peak=128;caps_limit=144;segment_slots=32;segment_limit=48;messages_queued=1;messages_peak=14;messages_limit=32;audit_used=686;audit_limit=768
```

- `CIO|`：管理器观察到 C 探针写的 `fd=1,ch=104('h')` 与 `fd=2,ch=105('i')`。
- `TTYIN|`：tty 服务实际收到并写入串口的同一对 fd/字节。
- `TTYOUT|`：tty 服务对 COM1 设备的写成功、Scratch 写回读等于魔数、区间外写被内核拒绝。
- `TASKCAP|`：内核独立确认 C 探针（id 17）已 Dead、调度根已清空。

Linux 宿主侧由 `cargo xtask test` 自动编译同一源码（`-DPRIMA_HOST`）并断言：成功输入退出 37
且 stdout=`h`、stderr=`i`；缺参数退出 2、缺环境变量退出 3。

## 容量与既有证据变化（逐条登记）

| 项 | 旧值 | 新值 | 原因 |
|---|---:|---:|---|
| C 探针 task id | 16 | 17 | tty 服务占用 id 16 |
| 任务表上限 `MAX_TASKS` | 17 | 18 | C 探针需要最后一个 id |
| 逻辑 vspace 上限 `MAX_VSPACE_ENTRIES` | 128 | 144 | 实测 126 条 + C 探针 4 页探测映射 = 130 |
| `ipc_ready` queued/direct/blocks | 45/39/46 | 45/46/54 | tty 注册/设备移交/ACK 与 C 两条 write 的直投/阻塞 |
| `task_ready` / `TASKCAP` 存活 Task | 11 | 12 | 新增 tty 服务 |
| `task_ready` 活用 Capability | 114 | 123 | tty 服务与设备交接 |
| `CAPACITY` caps_peak / segment_slots / audit_used | 119 / 31 / 629 | 128 / 32 / 686 | 同上 |

服务映像共 **78 / 80 页**（7 个 Rust 服务 74 页 + C 探针 3 页 + 目录 1 页）。C 探针仍无
`PT_INTERP`、无动态依赖、无重定位（`readelf -r` 为空）。

## 开发中暴露并保留的反例

- C 探针最初把入口凭据放在可写全局变量中，QEMU 在 `ENDPOINT_CODE_VA + 0x3880` 报
  `#PF`（err=0x7，写保护）——证明代码段按 W^X 只读映射。最终实现改用栈上 `probe_ctx`，
  不使用可写全局变量。
- 自毁会清掉本任务仍在接收队列中未被取走的消息；因此 Prima 的每次 write 后显式
  `SYS_YIELD`，让管理器先取走消息。退出状态发送时管理器已阻塞在 `receive`，走 pending
  直投，无需额外让出点。

## 未覆盖

- 没有 libc、`openat`/`read`/`close`、用户态 fd 表或真实 tty 行规程；只有 fd 1/2 的字节流。
- `_exit` 不建立 Process/zombie/父子等待，退出状态只由服务管理器观察。
- 未验证通用 ELF `PT_LOAD` 权限（BSS 可写、W^X 全路径）；探针通过“不使用可写段”回避，
  仍未实现按 program header 装载。
- 未声明 S5-D2/D6/X3 完成；未引入 musl、sysroot 或动态链接器。
