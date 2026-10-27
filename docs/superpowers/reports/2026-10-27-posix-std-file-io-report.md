---
id: report-posix-std-file-io
kind: report
authority: scoped-evidence
status: validated-with-limits
lifecycle: historical
updated: 2026-10-27
---

# POSIX std 后继：文件I/O与IPC接收归账

## 范围与基线

基线be3d6dd，加codex/posix-std-backend中未提交的[初始后端](2026-10-27-posix-std-backend-report.md)。
本次续做文件I/O与I1失败分析，调用链及验收修订见[工作契约](../specs/2026-10-27-posix-std-backend-design.md)。
nightly-2026-07-30；QEMU qemu64、单CPU、TCG；不修改容量、阶段勾选或profile发布状态。
本版取得单次完整36组/72轮PASS；workspace1020、三种Rust模式Clippy、lint、compat、TCB及相关host检查均通过。
下列支持限制仍在，不据此称为全量POSIX符合性。

## 实现与诊断

新增Store文件op6/7/8，真实OFD read_at/write_at/resize，经musl支撑File::set_len及FileExt。
保留dup/fork游标、打开权限及unlink后OFD；增长清零、容量失败不暴露旧数据。
超大正偏移饱和到树外哨兵，负值EINVAL，长度不会窄化回绕。
pwrite按POSIX忽略O_APPEND；V4在host显式检查Linux的追加行为，在Prima检查偏移写，分别记录差异。
每RPC最多2字节，多RPC非原子；fsync保持ENOSYS。

I1新归账将用户SEND之外的接收出口逐类记录。诊断双轮分别观测
(queued,direct,blocks)=(83,340,353)与(84,339,353)，
对应(kernel,uart,interrupted,retired,faults,live)=(2,2,2,1,0,8)与(3,3,2,1,0,8)。
内核完成均为UART，说明原先只看用户SEND分类会漏掉IRQ直投。
旧69/70失败未保存这些分类，不能事后为其354次阻塞逐条重构；该失败不改记PASS。
后继每轮严格归账所有阻塞，并固定跨轮的非UART出口及用户总消息数，明确修订原blocks固定假设。
新增丢失/重复、漏归账、错子集、异常出口、字段歧义及溢出反例；两轮原始串口分别保留。
完整命令中的I1又取得两轮PASS，实际为：

| 轮次 | queued | direct | blocks | kernel/UART | interrupted | retired | faults | live |
|---|---|---|---|---|---|---|---|---|
| run1 | 84 | 339 | 353 | 3/3 | 2 | 1 | 0 | 8 |
| run2 | 85 | 338 | 353 | 4/4 | 2 | 1 | 0 | 8 |

每轮归账严格相等，两轮用户消息总数423、非UART完成0和其余出口一致。
`target/prima-os/I1-run1.log`和`I1-run2.log`保留本次原始记录。

构建器还修复用户RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS被覆盖的实质缺陷：
优先encoded（包括空值），否则保留带引号参数；后端必需的panic=abort在用户参数后追加。
三个host回归检查带空格cfg/路径、encoded优先级/空值、破损引号拒绝。
普通无LTO标准main加入编译期断言：review_marker必须等于“two words”，panic策略必须abort；
传入该自定义cfg及panic=unwind后构建PASS，确认实际编译保留cfg并强制abort。
此修复在完整命令的R3构建前完成；此前的C/原生组不读取该POSIX构建脚本。
最终R3及POSIX Clippy使用修复后的overlay摘要重新编译，不能复用初始覆盖层的结果。
本版overlay为e47f9fe72c2bbe75f977cb3e99d21d762914e60adf201fd4784b13ba08d8afd7；
普通应用ELF无INTERP/PT_TLS，仅RELATIVE重定位；不是该普通示例的机器执行证据。

## 本次命令与结果

日志在/tmp/prima-posix-followup-*、/tmp/prima-posix-file-*、/tmp/prima-posix-i1-*及target/prima-os/。

| 命令 | 当前实跑结果 |
|---|---|
| cargo xtask test-vfs V4 | Linux同源参考与BIOS两轮PASS；file-ext-qemu |
| cargo xtask test-rust R3 | Linux同源参考与BIOS两轮PASS；file-std-qemu-final |
| cargo xtask test-tty | 新归账检查双轮PASS；i1-accounting-final |
| python3 scripts/test_posix_std.py | 三项host回归PASS；followup-builder-flags |
| 普通main应用带自定义cfg/冲突panic参数 | plain/encoded真实构建均PASS；encoded时RUSTFLAGS故意给错误cfg仍正确选择encoded；generic-flags/generic-encoded；最终空参数保留版generic-final/generic-elf |
| cargo test -p xtask interactive_ipc | 两项PASS；含接收归账反例与歧义字段拒绝 |
| python3 prima-user/musl/test_port.py | 五项PASS；followup-musl |
| cargo test --workspace | 首次FAIL：缺少ftruncate台账行；已补17/18/77，最终1020项PASS；followup-workspace-final |
| 原生R1/R2 Clippy | 本轮重跑均PASS；followup-r1-clippy/r2-clippy |
| cargo xtask lint及POSIX Clippy | 均PASS；fmt、Clippy -Dwarnings、裸机kernel、32文件零unsafe；修复参数覆盖后POSIX Clippy再PASS（followup-clippy-final） |
| cargo xtask test | 单次完整36组/72轮PASS，退出0；22项Linux参考（含R3）；followup-full；errors文件0字节 |
| cargo xtask tcb | PASS；trusted2072/mechanism30223/total32295，6.42%，unsafe78/0；followup-tcb |
| cargo xtask compat | PASS cow-write-fault；followup-compat |
| docs self-test/build/check/check --base be3d6dd | 四项全部PASS，319份文档；followup-docs-self/build/check/base-final |

R3最终stdout仍44B、FNV-1a=1614292895；退出后Task17 dead/root_cleared=1，tasks12/caps134。
R3本轮queued/direct/blocks=88/254/263，归账为kernel0、interrupted0、retired1、live8、faults0。
资源为caps_peak147/192、segment_slots39/48、messages_peak14/32、audit1993/16384。

## 失败与限制

R3同源参考首次沙箱socket EPERM，授权沙箱外重跑PASS；不归为Prima问题。
workspace的台账守卫首次捕获新入口未登记，保留失败并修正台账，不删除断言。
初始后端69/70及更早68轮记录保留各自版本范围。

Command创建子进程、TCP/DNS/IPv6、wall-clock、安全熵、文件持久化/fsync仍未实现。
新增普通文件I/O不等同完整POSIX符合性，不新增SMP、Miri或Lean证明。
本报告仅在I1归账及文件扩展主题补强初始报告；阶段5未签署、阶段6未开始。
