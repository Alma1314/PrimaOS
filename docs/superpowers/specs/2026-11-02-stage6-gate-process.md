---
id: spec-stage6-gate-process
kind: specification
authority: scoped-mechanism-design
status: implementation-contract-not-exit-signoff
lifecycle: current
updated: 2026-11-02
---

# S6-M4：受管进程复制和映像替换

承接[共享线程](2026-11-01-stage6-gate-threads.md)和[停车装载](2026-11-01-stage6-managed-launch.md)。
本协议只扩展中立Gate机制；Linux fork/execve/wait4的号、flags、参数、fd与退出状态由用户态解释。
1–86与普通原生fork/exec的门控拒绝保持。

- 87 GATE_FORK `(request, context, manager, gate, 0, 0)`：固定服务者持有实际Delivered请求、
  该客户端独立TaskContext与TaskManageVSpace，以及同Gate的GateAttach。仅单线程进程允许复制。
  复用原生fork的COW root、逻辑映射与能力继承；继承父保存GPR/FS/FXSAVE，子rax=0。
  子进程在内核不可调度区间完成注册、显式服务管理/上下文根和Gate绑定后才可运行；返回由服务持有、显式含TaskContext的新Task管理根（与86相同，不扩大旧根）。
  不答复父请求；不登记原生wait4亲缘或僵尸，personality自行保存亲缘与状态。硬件或绑定失败丢弃未发布子root和模型状态，恢复父COW快照；不回退代次/历史计数。
- 88 GATE_EXEC_IMAGE `(request, context, manager, descriptor, 0, 0)`：同一授权与单线程限制；
  descriptor是服务内存中的六字`image,len,argv,argc,env,envc`，沿用80的ELF/字符串/容量检查。
  完整复制/验证/分配成功前不破坏旧映像。提交拆旧映射、安装新私有root与映射，刷新绑定root代次，
  重置FS/FXSAVE，按新入口/初始栈恢复；在同一区间终结当前请求及所有别名、完成私有等待。
  客户端不从旧调用点返回。失败仍保留旧映像和Delivered请求；内核不解析路径、fd或Linux信号。
- 两个操作先检查所有授权、线程/别名/root域及审计/槽位预算。审计记录服务、请求、父子cookie
  或旧新root身份；新映像按现有装载协议记录入口，不声称记录完整可重放ELF。
- 旧管理根和上下文根仍指向同一Task incarnation；旧GateRequest全部失效。映像替换后的下一次
  Gate代理复制只能使用新root代次。跨进程共享root、对多线程exec、SMP及动态链接不在此契约。

用户态进程表限制为有界子进程，保存完整cookie与父子关系；wait4仅-1或实际子pid、WNOHANG，
无子ECHILD，活子阻塞保留Delivered请求，退出后复制32位Linux wait status再答复并回收一次。
execve只接受已登记只读原始ELF路径，先完整复制客户端路径/argv/env，成功后提交close-on-exec、
信号动作/线程状态重置和映像状态；未登记路径ENOENT，坏指针EFAULT，不跌入原生exec。
必须以同一ELF在Prima和pinned Linux guest检查fork的父子返回、私有写、exec后观察和wait状态。
该契约不是实现或运行通过证据；所有测试依作者要求在剩余实现完成后统一运行。

当前家族上限4个进程；普通fork(57)仅单线程，clone(56)仍只实现线程变体，不能把glibc fork的其它clone flags冒称支持。
共享现有OpenFiles owner位维护继承OFD游标与最后关闭；fd/CLOEXEC各进程独立，匿名段管理根按家族owner引用计数回收。
exec路径为`/busybox`、`/bin/busybox`与测试登记的`/bin/M4-child`；不是任意大文件VFS装载。
wait4暂不支持rusage、job control和信号打断，SIGCHLD通知未实现；明确ENOSYS的变体不列supported。
M4-fork见证必须覆盖共享OFD读偏移、子COW写不影响父、两次失败exec保留旧fd、成功exec的CLOEXEC、
argv/env与退出23；另一个子进程验证坏status返回EFAULT且消费退出状态，后续wait为ECHILD。
统一guest验证纠正了此前“EFAULT保留僵尸”的假设：Linux 6.8的
[kernel_wait4](https://github.com/torvalds/linux/blob/v6.8/kernel/exit.c)先do_wait回收，再put_user；
此处跟随参考行为，不把原先实现当成Linux契约。M4静态pthread见证覆盖真实EINTR，
再以SA_RESTART handler读取ucontext的原futex号/指令/地址，改变比较字后由重启得到EAGAIN。
M3逐调用见证打印稳定裸结果/errno；pid/tid、分配地址、utsname其余字段、readlink目标及dirent顺序
有profile声明的差异，只比较明确命名的身份/存在性/字段/成员谓词，不伪称全结构或全参数空间差分。

硬件执行权限遵循COSIR映射契约：通用memseg_map/protect把逻辑X同步到叶PTE的NX，
私有ELF逐PT_LOAD的PF_X设置NX，初始栈始终NX。PROT_NONE独立保留逻辑权限与NX。
仅更新叶执行位，不改变帧号、COW、USER/HIDDEN或写权限；现有bootloader已启用EFER.NXE。
这修正此前“仅逻辑执行权限、未完成硬件NX”的实现边界，旧报告保留其当时范围。

## 审计读取

89 GATE_AUDIT_READ `(gate, cursor, output, 128, 0, 0)`是固定服务者持GateServe的只读查询。
按全局审计下标cursor寻找该Gate的下一条trap/delivered/terminal事件，返回1并输出16字：
`version=1, bytes=128, next_cursor, kind, gate, request, cookie, number, value, flags, args[6]`。
kind=1/2/3；trap保留完整原号/参数/来源flags，delivered的cookie为固定service cookie；
terminal的flags为Replied=0/Aborted=1/Orphaned=2/Cancelled=3，其它不适用字段为0。
无后续匹配返回0，不写输出；cursor超当前审计长度无匹配。输出先完整验证服务自己的可写映射。
该查询不丢弃记录、不暴露其它Gate，不在内核解释终结值或Linux errno。
验收端使用它核对真实trap/交付/终结计数和ENOSYS对应的原始审计记录；用户日志不是内核审计替代物。
