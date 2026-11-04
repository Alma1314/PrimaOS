---
id: spec-stage6-gate-context
kind: specification
authority: scoped-mechanism-design
status: implementation-contract-not-exit-signoff
lifecycle: current
updated: 2026-11-01
---

# S6-M4：独立授权的 Gate 用户上下文

## 范围

按[D18 的重裁触发](2026-10-29-stage6-m1-mechanism-decisions.md#d18答复宽度v1-只写-rax)，
信号续体需要第二种答复。本文在[停车装载](2026-11-01-stage6-managed-launch.md)之后
追加83–85；73–78的rax-only、不可警报、一次终结和孤儿返回完全保留。
这是实现契约，不是阶段签署、Linux 信号实现或运行证据。原版共享root及门控fork/exec未开放；该限制后来由[同Gate线程](2026-11-01-stage6-gate-threads.md)与[受管进程](2026-11-02-stage6-gate-process.md)显式局部扩展，下文独占检查按原版范围保留。

## 权限与布局

- `TaskContext`追加为权利bit33，只用于Task；0–32不变。旧Task管理根和初始Authority不自动增加该权利。
- 83 `task_context_authority(task_manager,0,0,0,0,0)`：实际TaskManageVSpace持有者可为同一活动Task
  显式创建独立的`{Derive,TaskContext}`根能力。记录CapabilityMinted和源管理能力、调用者、目标cookie。
  这是显式授权扩展；不是权限子集派生。新根可单独派生、移动、撤销；撤销管理根不隐式撤销新根，
  目标Task销毁/代次变化使之失效。无TaskContext不能读取或替换保存帧，单有TaskContext不能操作请求。
- 84 `gate_read_frame(request,context,output,bytes=160|672,0,0)`。
- 85 `gate_reply_frame(request,context,input,bytes=160|672,0,0)`。
- 输入/输出8字节对齐。160字节格式为20个小端u64，沿用SretFrame线序：
  `rax,rbx,rcx,rdx,rsi,rdi,rbp,rsp,r8,r9,r10,r11,r12,r13,r14,r15,rflags,rip,cs,ss`。
  672字节格式在相同160字节帧后追加512字节FXSAVE镜像；不支持XSAVE/AVX扩展。
  替换时复用FpuArea::from_user按目标硬件MXCSR掩码校验，保留只读mask；所有校验和审计预检在一次提交前完成。
  这是本项目机器帧，不是Linux ucontext。其余参数、长度、对齐不符返回原生Invalid。

## 身份、校验与提交

两入口每次同时验证请求GateServe、固定服务Task及代次、Delivered、客户端cookie、独占root及其
绑定代次，以及调用者实际持有指向该客户端的TaskContext。复制服务本地缓冲前先验证上述授权。
不允许借其它客户端或旧代次的上下文能力使用当前请求。读帧不消费请求。

替换只接受非零低半区用户RIP/RSP，CS=当前用户代码选择子、SS=当前用户数据选择子。
RFLAGS必须置bit1与IF；允许CF/PF/AF/ZF/SF/DF/OF/ID；其它位（包括TF/IOPL/NT/RF/VM/AC）拒绝。
不能用面向ring0的旧exc恢复校验器代替。DF可恢复给用户，两个真实trap入口在调用Rust前均已有cld。
必须逐次检查当前逻辑Segment与实际用户PTE：RIP可执行且非NX，RSP-1可写（可为逻辑可写COW）。
拒绝设备/隐藏页、无映射、越界、共享root；不要求用户栈16字节对齐，因为中断续体可以在任意指令。
160字节格式不改FPU；672字节格式一并恢复x87/SSE，不能先单独改FPU再答复。
本切片不改FS、XSAVE、待决信号或映像。

完整读取服务输入、验证目标帧与预检所有审计后，才能在一个不可调度事务内替换保存帧、完成私有等待、
发布Ready、撤销所有请求别名并终结。不得先普通reply再修改帧。重复/迟到答复沿用Stale。
读帧先完成整个输出预检再写出，失败不写半帧、不消费请求；COW准备的PTE副作用沿用现有输出边界。

成功上下文访问另记gate/request、两端cookie、context句柄、RIP/RSP/RFLAGS、是否含FPU及方向；
GateTerminated保留新rax作为不透明值。该记录不含全部通用寄存器，不宣称可重放完整用户执行。
普通拒绝沿用原始参数失败审计；审计耗尽在业务提交前全局fail-stop。
丢弃TaskContext不孤儿化Gate；原有GateServe/请求权利丢失仍按v1处理。

## 验收义务

host反例覆盖无独立权利、非持有者/错误客户端、撤销后使用、Queued/终结后使用、非法选择子/地址/标志、
整帧一次终结、旧rax-only保持、审计边界。机器见证还须覆盖真实syscall与int80、跨页坏缓冲、
NX/只读/设备/隐藏PTE、COW、DF及保存寄存器恢复；编译不能替代这些证据。
按作者要求，测试与QEMU待阶段6剩余实现全部完成后统一运行。
