---
id: spec-stage6-linux-signals
kind: specification
authority: scoped-personality-design
status: implementation-contract-not-exit-signoff
lifecycle: current
updated: 2026-11-01
---

# S6-M4：用户态 Linux 信号与 P1 续体

依赖[独立 Gate 上下文](2026-11-01-stage6-gate-context.md)。Linux号、信号集合、sigaction、
sigcontext和errno只在prima-user解释；内核不识别Linux信号或自动取消Gate等待。
该切片覆盖有界线程组信号和P1的“先阻塞USR1、发送形成pending、sigsuspend临时解屏蔽、
实际运行handler、rt_sigreturn后观察-EINTR”。不能用固定返回-EINTR代替这一因果链。

## ABI与有界语义

- rt_sigaction(13)：内核ABI的32字节`handler,flags,restorer,mask`，sigsetsize=8；
  不允许修改SIGKILL/SIGSTOP。支持SA_RESTORER、SA_SIGINFO、SA_NODEFER、SA_RESETHAND、SA_RESTART；
  SA_ONSTACK在备用栈禁用时使用普通栈；sigaltstack仍未实现，其它flags变体返回ENOSYS。
  handler地址/栈由最终中立上下文接口校验。
- rt_sigprocmask(14)：BLOCK/UNBLOCK/SETMASK，8字节集合；SIGKILL/SIGSTOP不能屏蔽。
  先复制完整输入和旧输出，再修改服务内状态；坏输出不提交mask/action。
- kill(62)/tgkill(234)：仅本线程组（kill另支持pid=0的本单进程域）；sig=0只检查身份。
  普通信号合并pending，优先选最小号。忽略动作不排队；默认CHLD/CONT/URG/WINCH忽略；
  默认终止产生128+signal的服务观察状态。停止/继续和实时信号排队不在该profile。
- rt_sigsuspend(130)：临时替换mask并等待可交付信号；无信号则保留Delivered请求，服务不伪造返回。
  handler帧保存旧mask及-EINTR续体；返回后恢复旧mask。它不受SA_RESTART改变。
- rt_sigreturn(15)：从restorer后RSP-8定位活动帧，读取用户可修改的GPR/mask/FXSAVE，
  经中立接口整体校验和恢复。坏帧终止为SIGSEGV，不能回落到原生sigreturn。
- 每组共享64动作槽，每线程独立64位pending/mask、至多4层活动帧；clone继承mask，不继承pending或活动帧。
  kill先选未屏蔽的线程，否则在组中保留进程pending，解屏蔽时选择一次交付；tgkill选择实际tid。
  本组身份查找使用完整cookie记录，不接受其它Process。此切片仅交付1–31普通信号；
  不支持的实时/stop/altstack/跨进程变体明确ENOSYS，不能宣称完整Linux信号兼容。
- 真实Delivered futex/sigsuspend可由本组信号唤醒；被屏蔽或忽略的信号不虚假中断等待。
  普通futex handler续体为EINTR；SA_RESTART则验证原陷入指令为syscall/int80，保存原号并退回两字节，
  sigreturn后重新比较并登记WAIT。sigsuspend始终EINTR。Ready线程在下一次Gate边界交付，
  不声称任意用户指令处的异步抢占。信号排队、帧写入、答复是不同原语，非跨原语回滚事务。

## 帧与事务

按Linux v6.8 x86_64布局：pretcode在0，ucontext在8，sigcontext在48，sigmask在304，siginfo在312。
sigcontext依次保存r8–r15、di/si/bp/bx/dx/ax/cx/sp/ip/flags、四个16位选择子、err/trapno/oldmask/cr2/fpstate。
留出128字节red zone，handler入口栈满足RSP mod16=8；512字节旧式FXSAVE区64字节对齐，magic1=0。
只支持现平台x87/SSE，不宣称XSAVE/AVX/CET或备用栈兼容。siginfo按SI_TKILL/SI_USER保留来源pid/uid。
投递完整复制帧后，经672字节Gate答复提交GPR及FPU；handler入口清DF、rax=0、rdi=signal、
rsi=&siginfo、rdx=&ucontext。返回读取用户帧，保留用户对合法GPR与mask的修改。

服务状态的pending、mask、活动帧栈只在对应Gate答复成功后提交；失败保持请求可审查，
不在未知部分副作用后重放客户端调用。帧写入和最终答复是两个原语，若后者拒绝，已写用户字节
不会回滚；此边界与Gate代理复制相同，不能声称跨原语内存回滚。

验收需要纯布局/状态反例、P1裸syscall在Prima与同一pinned Linux guest的同一ELF、
mask恢复、handler实际执行、errno、坏指针、未知号及信号期间SSE保存。全部测试按作者要求留待最后统一执行。

布局来源：[Linux sigcontext](https://github.com/torvalds/linux/blob/v6.8/arch/x86/include/uapi/asm/sigcontext.h)、
[rt_sigframe](https://github.com/torvalds/linux/blob/v6.8/arch/x86/include/asm/sigframe.h)、
[signal_64](https://github.com/torvalds/linux/blob/v6.8/arch/x86/kernel/signal_64.c)、
[ucontext](https://github.com/torvalds/linux/blob/v6.8/include/uapi/asm-generic/ucontext.h)。
