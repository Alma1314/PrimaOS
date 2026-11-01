---
id: report-stage6-w7-timing-diagnosis
kind: report
authority: scoped-diagnostic-evidence
status: diagnosed-scoped-with-failures
lifecycle: historical
updated: 2026-11-01
---

# W7：30秒预算的分段定位

## 范围与输入

承接[装载几何后继](2026-11-01-stage6-loader-geometry.md)的完整60/72及隔离W7双轮30秒超时。
本报告记录本次定位，不抹去前序失败，也不修改`xtask::qemu::TEST_TIMEOUT`。
Gate代理复制/生命周期的实现与统一回归另见[本轮Gate报告](2026-11-01-stage6-gate-copy-lifecycle.md)。

起始诊断使用`76fc397`所构建W7 BIOS镜像及其冻结输入，QEMU8.2.2、pc-i440fx-8.2、TCG单线程、
qemu64、1CPU、128MiB。QMP每秒`info registers`，串口按收到的字节记录monotonic时间。
这只是诊断运行，未调用完整harness协议校验，不计双轮验收。

## 第一次分段采样

| 相对QEMU启动 | 观察 |
|---|---|
| 1.013–13.009秒 | 每秒样本均仍在实模式；主要EIP=0xad26/0xaa41，另有BIOS内部位置 |
| 14.115秒 | 收到`INFO : 4th Stage` |
| 14.542秒 | `event=boot_start` |
| 16.186秒 | 开始收到`EXEC` |
| 19.243秒 | `large=1` |
| 20.835秒 | `geometry=1` |
| 21.436秒 | `event=boot_complete` |
| 21.473秒 | QEMU退出33 |

该样本约三分之二时间在内核入口之前；不能把30秒总超时直接归因于exec复制。
缓存bootloader0.11.17源码`bios-stage-2/src/main.rs::load_file`在实模式装入整个kernel和ramdisk，
`disk.rs::DiskAccess`调用BIOS读盘；ELF包含调试节也要读完整文件。样本内核文件7,988,864字节，
ramdisk约7MB。后半段内核样本分散在页表构建、Arena、审计等位置，没有固定自旋点。

**结论边界**：已定位总预算中明确的引导输入成本；这一个成功样本不能解释历史每次超时，
也不能证明宿主争用、BIOS读盘或某一内核函数是全部失败的唯一根因。
本次最终隔离`CARGO_NET_OFFLINE=true cargo xtask test-process W7`退出0：Linux参考与QEMU两轮均PASS，
原30秒门限保持。最终普通完整回归72/72通过，W7/W7b各双轮PASS；详见本轮Gate报告。

## 临时去调试节反例

为验证输入体积，只在`/tmp/`生成去调试节的内核和BIOS镜像，未改变生产构建流程。
用pinned工具链的`rust-objcopy --strip-debug`从同一原镜像提取的ELF生成临时制品，长度
7,988,864→2,457,232。四个PT_LOAD描述保持一致，代码/数据负载一致，首段只有ELF头中
section-table元数据的五个字节变化；entry不变。

这次诊断约10.917秒到4th Stage、11.456秒到boot_start，随后在cow_ready后异常退出0，
没有EXEC或boot_complete，不能记为性能修复或PASS。源码显示自建root当前按文件image_frames
复刻内核VA范围；较短ELF不能据文件长度保证覆盖PT_LOAD的整个BSS虚拟跨度。
原ELF的`max(p_vaddr+p_memsz)=0x528060`，strip后的600个文件页只到`0x258000`；
自建root的文件页计数不足以覆盖该BSS跨度。这是缩小内核文件时需解决的额外自映射缺口，
与cow_ready后异常的观察一致；本轮未提交该缺口的整改，不能靠直接strip投入生产。
该方向仅记录诊断及明确限制，不偷偷调整正常镜像、内核栈或30秒门限。

早先一次去调试节尝试误取了并发cargo检查产生的Gate特性内核，与W7 ramdisk组合，
也在cow_ready后退出0；该次输入不同，不用于A/B推断。随后改为直接从原W7 BIOS镜像
提取内核后取得上面的受控反例。两次失败均保留，未据它们宣称W7已修复。

## 诊断方法与结果位置

诊断命令与正常QEMU参数一致，只追加本地QMP套接字：

```sh
qemu-system-x86_64 -machine pc-i440fx-8.2 -accel tcg,thread=single \
  -cpu qemu64 -smp 1 -m 128M -display none -monitor none \
  -qmp unix:/tmp/stage6-w7-profile.sock,server=on,wait=off -serial stdio \
  -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -drive format=raw,file=target/prima-os/prima-os-bios.img -boot order=c,strict=on
```

QMP握手后先发送`{"execute":"qmp_capabilities"}`，每秒发送
`{"execute":"human-monitor-command","arguments":{"command-line":"info registers"}}`。
串口读取用monotonic时间戳；64位内核RIP减本次slide `0x10000000000`后，用对应**同一镜像**ELF
`addr2line -f -C -e <elf> <relative-rip>`映照。超出观察期限显式kill并wait回收；不把诊断期限
替换验收门限。QMP本地套接字在沙箱内被拒绝，实跑经授权在沙箱外执行。

原始输出在`/tmp/stage6-w7-profile*.log`、`/tmp/stage6-w7-stripped-correct*.log`，不入库。
输入SHA-256见下节；最终普通回归结果见本轮Gate报告。

## 阶段影响

W7时限阻塞是否闭合以[阶段跟踪](../../../Prima%20OS%20实现阶段.md)及本次实际完整回归为准；
单次诊断成功、临时缩小输入或增加诊断观察时间都不构成关闭依据。

## 输入身份与最终验证

起始受控诊断的输入身份：

```text
24a9a392778d0b4e20c39f2c8472affd95928d2aa227dad9c5345157f04b0459  stage6-w7-kernel-original
983a2b500607ff544e9b855e98202b5eca85594cc55c164c984480cf2f84feaf  stage6-w7-ramdisk-original.img
6dd67b76be133de4df4bc4ab68fcf22d5755f9f21496ae1b7823edcbb54b12e0  stage6-w7-kernel-stripped-correct
```

隔离W7验收最终输出（命令退出0，非QMP诊断）：

```text
PASS linux exec-linux stdout/exit references
PASS qemu bios W7 run=1
PASS qemu bios W7 run=2
```

该PASS是本轮范围，不倒推前序失败的因果；首次完整71/72（I1差异）、最终完整72/72均保留。

本轮首次完整回归退出1、71/72通过，唯一I1跨轮消息总数不一致；W7/W7b均双轮PASS，
没有30秒超时。之后追加孤儿trap审计的最终源码另跑完整回归，72/72通过、命令退出0。
I1最终双轮总消息数均423，但不将其通过归因于Gate审计变化。W7本轮时限验收通过；
历史超时的唯一根因仍未确认，不能据此声明一般性能上界。
