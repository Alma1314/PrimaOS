---
id: spec-event-authority-model
kind: specification
authority: scoped-implementation-contract
status: working-contract-scoped-validation
lifecycle: current
updated: 2026-10-27
---

# 位置模型与 Timer、异常、等待和远端数据通路的组合

承接[位置/溯源契约](2026-10-25-capability-location-model-design.md)、
[发现及后继链](../reports/2026-08-17-capability-derivation-model-gap.md)、
[原语规范](../../../COSIR%20原语规范.md)§8.5/§9/§10/§12/§21。
本片是cosir可执行host状态模型，非NIC、分布式内核或Rust机器细化证明；新增host用例已纳入本轮workspace并通过，见[统一报告](../reports/2026-10-25-stage0-5-gap-followup.md)。

## 共同载体和通知来源

Message及MessageSent保存MessageOrigin，区分Task与KernelEvent；用户send只产生Task来源。
本地send、Timer到期及异常fan-out使用同一enqueue_or_deliver：FIFO或BlockedRecv直投，
直投移动Capability、设置pending_return、转Ready并记录接收。内核通知不携带capability。
mechanisms_well_formed组合位置、通知出处、Faulted/pending、远端及等待一致性；
页表/设备/GPU池及CPU核心数调度约束仍由各自细化模型承担，不能把组合谓词称为完整W1–W17。

## Timer 与等待

timer_set只为caller创建，权限Derive/Cancel/Destroy；过去期限同一操作进入Fired。
advance_time拒绝倒退，按(deadline,id)触发一次；Fired通知记录MsgId，cancel状态返回保持状态。
destroy Pending先取消；Fired通知不撤回。Task死亡删除所有recipient Timer，外部token保留为stale。
Timer对象与cap/message IDs先预检，耗尽拒绝且不提交；对象ID不复用。

WaitObject保持generation与双向waiters，请求检查等长/非空/重复/观测超前/授权。
Any按输入最小满足下标、All按全部条件；满足优先于alert，alert优先于立即timeout。
signal、对象销毁、期限和task_alert共用完成/解除全请求登记；upcall独立FIFO且alert不消费upcall。
advance_time先Timer后等待期限；每批按稳定顺序。Task死亡只解除等待关系，creator死亡不销毁其WaitObject。
有界u64 generation耗尽拒绝，不能回绕；不解释POSIX futex或信号的personality语义。

## ExceptionSource

显式创建/订阅/取消/销毁，最后一个HeldBy+ExcSubscribe token丢失立即退订。
raise_exception先记录发生，再按TaskId稳定fan-out，最后对硬件执行异常提交Faulted/pending。
SyscallTrap/ExternalInterrupt/TaskExit只递送，不更改Task状态；源类型不符为内核前置缺陷。
源销毁不撤回旧通知、不恢复Faulted Task。恢复授权查询locations，owner仅是历史投影。
源签名支持单个构造子或GPF/DivideError/InvalidOpcode/DoubleFault的集合；拒绝空集和不兼容类别混合。
值参数变化不改变签名，重复构造子归一化；真实硬件trap重播仍不能由fan-out host测试证明。

## 远端纯数据

NodeIdentity与本地NodeId分离，平台known_nodes限定可连接身份；每peer至多一个活动Node。
node_attach复用(peer,remote_id)代理；已有代理但caller无TaskSend时返回NotHeld，不能再铸无父根。
RemoteProxy恒Ready、无vspace/结构根/邮箱/等待/异常，不得执行用户操作。
send拒绝所有cap传递，成功只追加transport handoff及发送审计，不建源端Message或改变cap位置。
接收方先检查活动本地CPU目标，无效即记录异步丢弃且不建代理；有效时建立/复用发送者代理，
以其Task来源进入本地共同载体。transport_msg连接两端审计，不充当全局唯一ID。
身份由可信transport输入，用户payload不能指定身份；不建模密码学、NIC、重传、去重或跨节点cap委托。
node_destroy清其peer代理并使旧cap stale；已经交给transport的数据不回滚。

## 内核生命周期配套

TimerTable持久保存next_id，销毁释放物理表槽但不复用opaque TimerId。
IpcQueue消息ID使用u64耗尽哨兵，u32末代可用一次，之后拒绝；send、Timer及异常fan-out先预检整批。
到期Timer批次按每接收者第一条直投、其余排队预算；分别预留4/2条审计，避免少预算或过度拒绝。
Segment普通149页和boot聚合192页边界保持前片定义；本片不改变这些帧存储上限。
CPU #AC新增独立event4路由，由C personality映射SIGBUS/BUS_ADRALN；其他路由编号不变。
无CR2可用的CPU异常仍报告指令地址，不能声称准确的对齐数据地址或已执行#AC机器见证。

## Lean 映照和验收

Lean Object加入Timer/Node/WaitObject引用、rights改UInt32，State.locations独立于历史owner；
最小excResume查询HeldBy，derive同时登记新位置。Formal.TokenLocation有独立carriers索引，
transfer/drop保持双索引一致，唯一载体、结构根拒绝、当前持有者守卫及不删除历史/后代记录有证明草稿。
该通道不证明对象存活/完整send/namespace/Timer/远端/页表，也不是Rust代码提取。
精确映照以[lean/README](../../../lean/README.md)为准；本轮已执行lake build和既有Rust对拍；只新增Lean README声明的有界通过结论，不扩大为完整迁移证明。

本轮workspace/lint、完整BIOS双轮、Lean、compat及文档检查结果见统一报告；Miri仅覆盖本轮authority模型21项，未宣称全包通过。
host MODEL-TEST、Lean独立证明与QEMU机制执行分开记账，历史失败不被本文件覆盖。
