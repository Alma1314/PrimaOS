---
id: report-stage6-loader-geometry
kind: report
authority: scoped-implementation-evidence
status: verified-scoped-with-failures
lifecycle: historical
updated: 2026-11-01
---

# S6-M2：RAM 复制、四段装载几何与八页栈

## 结论与范围

本轮推进三项：普通RAM输入/exec目标的批量复制、冻结busybox的四段/BSS几何、八页初始栈。
几何探针复制冻结PT_LOAD参数，使用合成字节和自定义入口；不是运行冻结busybox。
Linux同源与两个错误期望变异通过；单次QEMU诊断26.63秒退出33并输出完整geometry=1。
完整套件60/72、最终隔离W7双轮超时，因此W7时限阻塞未关闭。阶段6仍进行中，退出0/4。75/76代理复制、完整G2/G3、VFS大文件与Linux LibOS不在本轮。

## 基线与输入

干净基线 `0c1a3d65f00fd36de98b4518bc6d0d4120224118`。上游为
[阶段6–7计划](../plans/2026-10-27-stage6-7-refinement.md)、
[容量契约](../specs/2026-10-31-stage6-exec-frame-capacity.md)、
[容量与回收失败链](2026-10-31-stage6-exec-frame-capacity.md)、
[控制审计后继的超时记录](2026-10-31-stage6-gate-control-rejections.md)。
工具链nightly-2026-07-30；QEMU BIOS/TCG、pc-i440fx-8.2、qemu64、单CPU、128MiB、原30秒门限。
本树最初缺少musl快照；从主工作区复制到ignored target，既有脚本验证锁定commit、干净checkout
及许可/VERSION哈希后构建；sbase同样复用本机锁定快照。未修改依赖锁或提交快照。

只读取 `/usr/bin/busybox` 的头部及SHA-256，值与白名单一致：
`dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14`。
冻结制品只作解析参考，不装入guest执行。

## 实现

`vspace::read_user_bytes` 仍对完整范围和所有页表层级先检查，再以普通RAM切片按物理页块复制；
不改变显式root、长度上限、跨页预检及失败保持输出的语义。调用者保持root有效，内核缓冲
不得别名用户帧；不用于MMIO。`copy_exec_page` 保留4KiB暂存及退栈边界，使用OSTD的
`write_bytes`将有效前缀复制到独占新帧。新原语的unsafe仅在prima-ostd，明确不重叠和映射契约；
新增4097字节非对齐写入、空写和两侧哨兵测试。复用现有跨页/权限/独立root测试。
不改变COW、权限检查、零帧、资源配额或guard。

W7保留526页逐页marker、真实额度耗尽、四次失败回收及再次exec，新增严格`geometry=1`字段。
合成输入长0x205000，四个PT_LOAD元组如下（align均0x1000）：

| offset | VA | filesz | memsz | flags |
|---|---|---|---|---|
| 0 | 0x400000 | 0x6c8 | 0x6c8 | R |
| 0x1000 | 0x401000 | 0x1a7865 | 0x1a7865 | RX |
| 0x1a9000 | 0x5a9000 | 0x521e5 | 0x521e5 | R |
| 0x1fb820 | 0x5fc820 | 0x9568 | 0x108e8 | RW |

几何与526页需求来自冻结制品；入口改为0x401000。检查第三段全部初始化字节、第四段从页首
0x5fc000到文件尾0x605d88的全部字节为0xa5；检查BSS `[0x605d88,0x60d108)` 为零，
且BSS末尾可写。文件页内多余源字节为0xa5，能发现错误复制到BSS。
入口另检查rsp对齐16、argc=1、argv0起始字符及argv/env终止；从rsp所在页下移七页，
检查到rsp之前的字节为零，并逐页写读页底。原生还核对栈底0x7f0000000000。
成功后原生销毁并drop四个已无映射源段；Linux同源不调用这些原生入口。
`geometry=1`只在子进程正常退出42时产生，几何错误退出131，栈错误132，清理错误也拒绝。
这是有界初始化/读写检查，不是guard、NX、任意参数大小或所有栈峰值证明。

## 失败与诊断

- 未改代码的基线W7本次双轮PASS。因此未稳定复现前序W7失败，不宣称本轮定位了全部超时根因。
- 单次30秒手工monitor诊断中guest达到原W7完整成功字段，但未在截止前完成整个验收；
  地址减去PIE装载基址后，11个内核RIP样本中2个落在read_user_bytes逐字节循环、1个落在
  read_u8_at。另有启动/表查找/原子操作；采样只支持复制路径存在开销，不是性能归因证明。
- 首版扩展探针在host编译并行期间运行：第一轮只到`large=1`、缺少geometry；第二轮无stdout，
  均30秒超时。没有从这些输出推定guest死锁，也不放宽门限。后续统一验证以最终代码串行运行。

- 首次统一命令的Linux N1因沙箱socket限制返回EPERM；中断后重跑沙箱外测试时发现镜像写锁。
  进程核对显示第一次中断留下xtask继续运行；清理本次测试遗留进程后再次串行重跑。
  这些中断轮次不计最终通过，不修改harness退出码或锁策略。
- 两个临时Linux探针变异分别把BSS期望值0改为1、argc期望1改为2；wrapper均退出1，
  子进程分别退出131/132、geometry=0。变异只在/tmp，验证探针对BSS/栈错误敏感，不证明
  原生loader所有变异均被拒绝。

## 命令与结果

| 命令 | 本次结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1091 passed、0 failed、1项既有外部样本ignored |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy、裸机编译、38个kernel文件零unsafe |
| `CARGO_NET_OFFLINE=true PRIMA_ELF_SAMPLE=/usr/bin/busybox cargo test -p kernel --lib exec::tests::plan_accepts_operator_supplied_static_linux_elf -- --ignored` | PASS：1项，只解析冻结制品 |
| `cc -O2 -std=c11 -Wall -Werror -DPRIMA_HOST <临时变异stage5_exec.c> -o <临时程序>`，随后运行 | PASS：BSS/栈两个错误期望均拒绝，child=131/132 |
| `CARGO_NET_OFFLINE=true cargo xtask test`（清理遗留进程后、沙箱外） | **退出1**：Linux参考全部通过，QEMU60/72轮PASS、12轮30秒超时，8组失败；不计为整套通过 |

最终整套失败轮次：E1第2轮、N1第2轮、W8/W9/W7/W7b双轮、M1第1轮、A1第1轮。
A1停在bootloader的`4th Stage`，E1无stdout；其它失败也没有足够证据确认共同根因。
W7两轮均未到达新增geometry成功字段，不能用Linux、旧基线或后续单次诊断代替双轮QEMU验收。

完整套件关键输出（原30秒门限）：

```text
PASS linux exec-linux stdout/exit references
FAIL W7: BIOS W7 run 1 failed: QEMU timed out after 30s; QEMU child terminated
BIOS W7 run 2 failed: QEMU timed out after 30s; QEMU child terminated
PASS qemu bios R3 run=1
PASS qemu bios R3 run=2
ERROR full BIOS suite: 8 failed groups/checks; details in target/prima-os/full-suite-errors.log
```

`CARGO_NET_OFFLINE=true cargo xtask test-process W7`最终隔离运行**退出1**：Linux参考PASS，
QEMU双轮仍在30秒超时，两轮均只到`large=1`，缺少geometry字段。

随后对同一最终镜像独立运行相同QEMU参数（serial=stdio、monitor=none），Python观察器仅将
诊断等待上限设为90秒，未修改harness：**实际26.63秒退出33**，串口为：

```text
EXEC|exec=1;wait=1;bad=1;stream=1;large=1;geometry=1|ok=1;err=0
PRIMA|v=1|seq=43|event=boot_complete|status=ok
```

该运行未调用整套harness的全部协议校验，不计为双轮验收通过；它直接证明该最终镜像中
合成四段/BSS和八页栈探针成功执行了一次，也说明超时并非每次必现。没有据此推导根因、
最坏时间上界或关闭时限阻塞。未重复整套追求PASS。

最终关键源码身份（SHA-256）：

```text
51ac615093b8618618fd1e5fe9d5c2fce4f9d003140e6c4653bc4bdef9cef3a2  kernel/src/vspace.rs
2863fe20b11b3777272947808a8103842391492c20e539988f9bd4dd0dcbe585  kernel/src/syscall.rs
20b46a805adddc88626004ba6302e0e13353664cbf86812543ce72b7823a4df0  prima-ostd/src/mm.rs
2a17a47a15eaedf2e0429da0bc8e571d3ad217b487aaddbed239e4766c6d2516  prima-user/c/stage6_geometry_fixture.h
```

文档按仓库四项命令检查；原始运行输出仅保留/tmp和ignored target，不提交日志附件。

## 后继范围

本报告只补强装载几何和初始栈的合成探针证据；前序失败记录保留。
即使W7本次通过，也不证明冻结busybox指令、TLS/libc/完整Linux启动或一般性能上界。
当前阻塞与阶段条件见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。
