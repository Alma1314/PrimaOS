---
id: report-stage6-context-signals-threads
kind: report
authority: scoped-implementation-progress
status: built-not-runtime-verified
lifecycle: historical
updated: 2026-11-01
---

# 阶段6续做：独立上下文、Linux信号及共享线程/futex

## 结论与范围

承接[停车装载与LibOS镜像](2026-11-01-stage6-managed-linux-loader.md)，基线仍为
`4335e184ef5dd10679d72ecf6a0189b7672d9cd2`加当前未提交修改。
已接线83–86、独立TaskContext、完整GPR+x87/SSE答复、用户态信号续体、同Gate共享线程、
clone/TLS/futex与P1/P2裸syscall探针；专用BIOS/UEFI构建成功。
**按作者要求仍未执行任何host测试、QEMU或Linux参考程序。** 编译与打包不能证明P1/P2已通过。
阶段6未完成，退出保持0/4；静态pthread、fork+exec/wait、线程信号/中断futex、完整逐调用差分、
G4计量、全部回归及profile冻结/退出评审仍待推进。

契约先于代码：[Gate上下文](../specs/2026-11-01-stage6-gate-context.md)、
[用户态Linux信号](../specs/2026-11-01-stage6-linux-signals.md)、
[同Gate共享线程](../specs/2026-11-01-stage6-gate-threads.md)、
[Linux clone/futex](../specs/2026-11-01-stage6-linux-futex.md)。这是D18预留的第二种答复，
不改变77/78的rax-only语义；普通attach和原生thread_create仍拒绝门控共享绕过。

## 实现与边界

- 权利bit33仅为TaskContext，旧0–32及普通Task管理根保持。83显式从实际TaskManageVSpace授权
  创建独立上下文根并审计来源；它有独立撤销寿命，不假装是权限子集派生。
- 84/85仅在Delivered、固定服务者、Task cookie、实时root/映射及独立权利均匹配时使用。
  160字节恢复GPR，672字节一并恢复512字节FXSAVE；复用现有MXCSR校验。
  用户CS/SS、非零低半区PC/SP、安全RFLAGS、逻辑X/W、实际用户PTE/NX/COW/帧属主都检查。
  整帧/FPU、私有等待、Ready与请求别名终结在一次不可调度提交中完成；失败仍Delivered。
  不提供XSAVE/AVX/CET，不记录完整可重放寄存器轨迹，不修改FS或映像。
- 86显式共享同一Gate/Process/root，继承请求者保存FPU；指定子帧与FS，返回显式含TaskContext的管理根。
  两个可选4字节新Task id输出均先完成逻辑/PTE/COW准备，创建成功后在内核不可调度区间全部写出。
  不向子线程传原生self能力。每次共享copy/context/VM重新扫描全部root共享者，拒绝未管理或异Gate共享者。
  这是单CPU边界，不声称SMP页pin/TLB正确性。旧共享root拒绝范围仅由这个显式操作局部扩展。
- Linux信号布局、mask/pending/动作及rt_sigreturn只在用户态。P1先屏蔽USR1、tgkill排队、
  sigsuspend解屏蔽，handler实际执行后通过Linux帧恢复-EINTR、旧mask、GPR、DF与SSE。
  信号仅本单线程路径；备用栈、实时排队、跨线程/跨进程投递及中断futex尚未支持。
- Linux线程组最多8线程/8futex等待。WAIT保存真实Delivered请求，比较不等EAGAIN；WAKE终结匹配请求并计数，
  PRIVATE与非PRIVATE键分开。非leader退出只销毁自身并按非PRIVATE键清零/唤醒clear_tid；leader先退仍停车至最后线程。
  clone的Linuxflags/父子返回值及地址选择都在用户态。非零timeout、跨独立Process共享futex仍ENOSYS。
- 新host反例涵盖帧/选择子/标志/权限/持有者/目标、一次终结、审计耗尽、共享域与未管理共享者。
  专用服务加入第一次请求上的错误context、CS、IOPL、栈执行与MXCSR拒绝检查。这些均仅编译，未执行。

## 同一探针制品与差分链

`prima-user/linux-probes/p1.S`、`p2.S`通过现有系统cc以`-m64 -nostdlib -static -no-pie`
构建。P1无libc钩子，断言ENOSYS/EBADF/EFAULT/EINVAL与真实信号EINTR，检查handler、mask/GPR/DF/SSE恢复。
P2设置/读取两线程FS，比较getpid/gettid及tid输出，实际WAIT/WAKE=1，检查提前wake、错值、坏地址/对齐、
零bitset/零timeout与退出clear_tid唤醒。所有断言通过才输出`P1 ok`/`P2 ok`并退出0。

最初打包拒绝“native service does not embed the exact P1 ELF bytes”。静态比较发现cc临时对象名进入符号表，
使宿主/目标两个构建生成不同文件；不是运行失败。修复为构建时`--build-id=none --strip-all`，
使同源探针字节确定；没有修改D12 busybox。后续构建逐字核对busybox、P1、P2均嵌入服务，才打包。
参考guest复制这些原始文件，六applet加P1/P2共八项；Prima仍双轮、30秒门限，参考独立180秒。
解析器拒绝缺失/重复/乱序/失败/无真实trap/ENOSYS计数不符，stdout/stderr/status逐字比较。
该链已构建但从未执行，不把探针源码断言当成实际差分结果。

P2静态ELF为4个PT_LOAD（R、RX、R、RW）及非执行GNU_STACK；含8192字节子栈BSS。
这只是readelf观察，不是硬件NX全部验收。

## 命令与实际结果

| 命令 | 结果 | 范围 |
|---|---|---|
| `cargo fmt --all` | 0 | 格式化 |
| `CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings` | 0 | 普通配置编译/Clippy |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo clippy --workspace --all-targets --features xtask/linux-probe -- -D warnings` | 0 | 包含P1/P2、harness与host用例，仅编译 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -p xtask --features linux-probe -- build-linux` | 0 | 专用BIOS/UEFI，输出三个PASS build/image标记；未启动 |

开发期还修正了测试审计访问器名称、固定分块API、闭包索引类型与可派生Default的编译/Clippy错误；
保留原始输出于/tmp，未将日志或二进制入库。没有运行cargo test、xtask test/test-linux/test-gate、
完整lint、TCB、Lean或docs self-test。未以改门限、重试或宿主执行替代所需机器验收。

构建制品SHA-256（仅识别本报告构建，不等于运行证据）：

| 制品 | SHA-256 |
|---|---|
| P1 | `4884b689a0771d60e4018fd0dbd9d29ecefcca3056b5532bec7f601e5d2d9263` |
| P2 | `a0108371f8efb0165595f3ab53f05c1b9ab795fada05c1f80614a51899d262c2` |
| native service | `7b12c484603601d6d31d11c3fa48afca4dd7ecf54d85cd8515b3874c40fc5968` |
| kernel boot ELF | `1ccd033638b9a2c75964ba3396de784536593f9c858c14159e3dfa26e2f155b3` |

后续必须完成剩余实现，再统一执行上述未跑矩阵、记录失败及修复并据实际范围更新证据。
