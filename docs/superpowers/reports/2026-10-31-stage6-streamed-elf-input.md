---
id: report-stage6-streamed-elf-input
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# S6-M2：按需 ELF 输入与 exec 失败退栈回收

## 结论与范围

基线 `b0d78cf`（工作区干净）上，关闭原生 exec 的 `image_len ≤ 1 MiB` 与首个 4 KiB
程序头前缀限制；新增按需读取器与 W7 稀疏输入，并修复新故障负例暴露的回滚栈越界。
这是[输入契约](../specs/2026-10-31-stage6-streamed-elf-input.md)的有界实现证据。
完整回归结果在下表记录，不替代阶段签署。

段数据在本轮基线已经由 `copy_exec_page` 逐页复制，因此不宣称本次新增了段数据流式
路径。[早期装载报告](2026-10-28-stage6-loader-slice.md)与[启动报告](2026-10-29-stage6-start.md)
保留历史事实并追加后继说明。149 页映像上限、全局资源预算、VFS 大文件来源、8 页初始栈
验证及 Gate/Linux LibOS 仍待完成；本轮不勾选 S6-D1、S6-D2 或任何退出条件。

## 基线与实现

上游为[阶段 6–7 计划](../plans/2026-10-27-stage6-7-refinement.md)、
[M17c-2 契约](../specs/2026-10-23-stage5-m17c2-exec-syscall.md)及上述装载报告。
使用 `nightly-2026-07-30`、现有离线依赖、BIOS QEMU TCG/qemu64/单 CPU/128 MiB；
30 秒门限、退出码、容量与输出校验保持原规则，无 KVM、SMP、性能或 Lean 主张。

- `kernel/src/exec.rs`：`plan_read` 只读取 64 字节 ELF 头及每次 56 字节程序头。
  `phoff + phnum * 56` 在读取程序头前检查溢出及声明文件边界；已有 `plan_len`/`plan`
  共用同一验证器。`PlanReadError` 区分格式错误与底层读取错误。
- `kernel/src/syscall.rs`：原生 55/62/69 的共享输入路径移除 1 MiB 上限与 4 KiB 前缀
  缓冲；先检查地址加长度溢出，逐条通过既有用户页权限检查读取。保留 parser 的
  `inline(never)` 退栈边界。ENOEXEC 与源页 EFAULT 分开返回。
- `prepare_exec_image` 失败时先释放暂存页表，返回原错误及“已建暂存 Segment”的事实；
  `exec_call` 等构造栈帧退出后才回收该 Segment。前置失败不走无依据的回收；成功提交
  路径不变，没有扩栈、取消 guard 或放宽资源上限。

## W7 输入与故障见证

合成文件声明长度 `0x203000`（2,109,440 B），`phoff=0x2ff0`，一个 56 字节程序头跨
两个非首页源页；两页 RX 段的文件偏移为 `0x201000`，映射到 `0x400000/0x401000`。
Prima 的源缓冲只映射五个物理页，文件空洞未映射。新程序从第二载入页执行、读取第一
载入页的字节 42，并以 42 退出；原生入口先调用已无映射源 Segment 的销毁接口。
这能发现整块读取、首个 4 KiB 假设、1 MiB 拒绝、漏拷首/末页和错误恢复旧映像。

同一子 Task 在成功 exec 前还必须通过以下原生负例：

1. 地址加声明长度溢出返回 EFAULT。
2. 解除跨页程序头后半页，读取返回 EFAULT，恢复原页。
3. 解除第二段数据页，使装载先完成第一页再失败；连续两次返回 EFAULT，旧程序继续
   执行，栈 sentinel 与源字节保持；恢复页后最终 exec 成功。

最终 stdout 必须完整等于 `EXEC|exec=1;wait=1;bad=1;stream=1|ok=1;err=0` 加换行，
同时保留已有 exec/argv/wait/坏 ELF 判定。Linux 同源在连续虚拟缓冲中构建同形文件，
经 memfd/既有临时文件回退运行；只有合成 ELF 的成功装载是同源比较，三个原生负例
不在 Linux 分支执行，不表述为 Linux errno 差分。重复失败及最终成功也不等于全部资源
耗尽情况下的无泄漏证明或逐项帧计数回滚证明。

## 失败与整改链

- 首版探针把源缓冲放在 `0x00000c0000000000`，与现有 C_HEAP_VA 重叠。
  内核以 EACCES 拒绝，子进程退出 111，`stream=0`，严格验收失败（[原始输出摘录](#关键输出摘录)）。
  临时错误序号诊断确认 `MemsegError::Overlap`，随后撤销；最终源区为 `0x00000e0000000000`。
- 修正地址后，两个 QEMU 轮次退出 0，被要求退出 33 的校验拒绝（[复位输出摘录](#关键输出摘录)）。
  开启既有 CPU 日志并定位到 guard 内的 supervisor 写故障，随后 double/triple fault
  （[CPU 输出摘录末尾摘录](#关键输出摘录)）。诊断时对该临时产物
  `addr2line` 定位为 `Arena<SegmentObject,48,1>::set` 的 closure；该构建产物未入库。
  暂存回收嵌套在大构造栈帧之下；将回收移到退栈后，并保留 parser 边界后，W7 双轮通过。
- 临时分段输出也使 Linux 严格 stdout 比较失败，随后全部移除。没有把临时诊断构建
  计为最终验证；最终代码重新运行 workspace、lint 与完整套件。

## 命令与实际结果

| 命令 | 结果与证据范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1053 passed、0 failed、1 个既有外部 ELF 样本 ignored |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机编译，kernel/src 35 文件零 unsafe |
| `CARGO_NET_OFFLINE=true cargo xtask test-process W7` | PASS：Linux 同源与 QEMU 双轮（原日志逐轮覆盖，非两轮原始串口合集） |
| `CARGO_NET_OFFLINE=true PRIMA_ELF_SAMPLE=/usr/bin/busybox cargo test -p kernel --lib exec::tests::plan_accepts_operator_supplied_static_linux_elf -- --ignored` | PASS：完整字节解析与文件 seek/read 的逐条解析结果一致，每次元数据读取≤64 B |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：最终单次命令 exit=0，36 程序/72 轮及全部 Linux 参考 |
| 文档四项与 Git 空白检查 | PASS：self-test、build、check、check --base b0d78cf，337 文档/3310 章节；Git 空白检查通过 |

可选样本 SHA-256 与[D12 白名单](../compat/stage6-static-whitelist.md)冻结值相同；不携带
二进制。样本测试只解析头部，不分配其 526 页、不运行其代码。
全套件在沙箱外执行，因为既有 N1/R3 Linux 参考需要主机套接字。下文只内嵌关键输出；
CPU 故障只列用于定位的字段，不作为完整执行日志。

## 阶段影响与后继

本报告只关闭上述原生文件输入限制，并补强现有 exec 的读取失败路径。原生内存 ELF
不等于大文件 VFS 来源，稀疏文件长度不等于物理映像容量。白名单、profile v2、73–78
候选号及 Gate 权利分配均未改变。S6-M2 后继需先解决真实映像的帧向量、全局归属与映射
容量，再验证初始栈和完整 Linux 启动；S6-M1 的 Gate 能力/陷阱/rax 答复与 G1–G3 仍为
当前阻塞。当前状态只见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

## 私有帧容量后继

[526页映像与再次exec](2026-10-31-stage6-exec-frame-capacity.md)扩展私有映像与相关预算，
并保留压力探针先撞段表、旧回收超时的失败证据；本文原有输入/退栈修复证据仍有效。
不从该后继推断完整VFS来源、冻结busybox运行、LibOS或阶段退出。

## 关键输出摘录

以下片段来自本报告原验证记录，仅内嵌支撑结论的输出；本次文档整理未重跑这些验证。
完整旧附件的恢复版本及目录见[日志整理记录](2026-10-31-documentation-log-inline.md)。

原 `initial-failure.txt` 的关键行：

```text
ERROR BIOS W7 run 1 failed: W7 stage5 capacity trace mismatch: [Record { id: 7, args: [17, 90, 1, 1, 1] }]
EXEC|exec=1;wait=1;bad=1;stream=0|ok=0;err=28416
```

原 `reset-failure.txt` 的关键行：

```text
ERROR BIOS W7 run 1 failed: unexpected QEMU exit code: expected 33, got 0; panic exit code is 35
BIOS W7 run 2 failed: unexpected QEMU exit code: expected 33, got 0; panic exit code is 35
```

原 `guard-tail.txt` 的关键行：

```text
   156: v=0e e=0002 i=0 cpl=0 IP=0008:000001000006eca7 pc=000001000006eca7 SP=0010:000001000016bf10 CR2=000001000016bf18
Triple fault
```

原 `W7.txt` 的关键行：

```text
PASS qemu bios W7 run=1
PASS qemu bios W7 run=2
```

完整套件 W7 末轮实际输出；旧串口文件逐轮覆盖，不是双轮合集：

```text
EXEC|exec=1;wait=1;bad=1;stream=1|ok=1;err=0
```

只作解析的冻结输入身份：

```text
dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14  /usr/bin/busybox
```

原 `full-suite.txt` 的关键行：

```text
PASS full BIOS suite: 36 programs, 72 runs
```
