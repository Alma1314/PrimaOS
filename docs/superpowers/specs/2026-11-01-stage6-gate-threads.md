---
id: spec-stage6-gate-threads
kind: specification
authority: scoped-mechanism-design
status: implementation-contract-not-exit-signoff
lifecycle: current
updated: 2026-11-01
---

# S6-M4：同一 Gate 下受管理的共享 root 线程

承接[独立上下文](2026-11-01-stage6-gate-context.md)，为P2及Linux线程profile提供中立机制。
这局部扩展M1的独占root，不开放普通attach/原生thread_create绕过门控，也不把Linux clone flags带入内核。

## 新操作

86 `gate_spawn_thread(request,context,frame_ptr,bytes=176,fs_base,gate_cap) → manager_cap`。
调用者必须是Delivered请求的固定服务者，持有该客户端TaskContext及同一Gate的实际GateAttach；frame来自服务内存，RSP须8字节对齐。
160字节帧后追加两个可选的客户端输出地址（0跳过），写入新Task id的4字节小端值。
两处逻辑/PTE/COW输出计划先全部准备；创建成功后在同一不可调度内核区间写出，再返回服务。
普通失败不写半份id；COW准备的PTE副作用仍遵循Gate复制边界。
frame按既有ring3 GPR、选择子、RFLAGS、实际PTE/逻辑映射验证；FS为低半区规范基址。
成功创建与请求者共享Process/能力空间/root的新Task，继承请求者保存的x87/SSE，
使用指定GPR/FS，原子安装同一Gate绑定后才能Ready。返回由服务持有、显式含TaskContext的Task管理根（源已有TaskContext授权，不扩大旧根）；
不把原生self capability交给客户端，不答复父请求。由LibOS决定哪些地址需要新Task id，并选择Linux返回值和clear_tid语义；内核不识别clone flags。

## 共享域边界

只有本操作能够将Gate标记为允许共享root。每次copy/context/VM事务重新扫描所有活动root共享者：
须为同Process、同一活动Gate/固定服务者的已绑定Task，每个cookie/root代次匹配；发现任何未管理共享者拒绝。
非共享Gate保留v1独占检查。共享组中的一个Delivered请求允许该服务串行修改整组的逻辑owner映射，
仍需实际TaskManageVSpace。单CPU内核不可抢占，预检至复制提交之间无其它用户Task运行；
不跨原语缓存物理计划。此规则不能外推SMP，7-B须重新建立页pin/TLB/同步协议。

创建须在一个不可调度区间完成：资源、能力、绑定/等待及完整审计预算先预检；失败撤销新Task、
管理根、Process成员、root登记与共享模式变更，不运行子Task。若为原本无Process的客户端创建组，
失败也撤销新组关联。旧root和父请求/帧不变；回滚事件保留，能力/Task generation不回绕。
子线程销毁沿用既有共享root生命周期：先解除绑定，最后共享者才回收root；不能提前销毁仍有线程的leader。
服务死亡/授权丢失仍孤儿化所有绑定，未交付客户端不能落回原生分派。

成功另记GateThreadSpawned（gate/request、服务/父/子cookie）；普通拒绝及耗尽遵循全局Gate审计协议。
Lean/COSIR模型尚无该迁移，不从旧W17有界检查推出共享组形式证明。

## 验收

host反例及机器P2须覆盖同Gate共享复制、未管理共享者拒绝、错误权限/帧/FS、创建失败不发布、
两个Task独立FS、futex比较与登记串行化、真实等待/唤醒、先wake再wait不丢唤醒及clear_tid退出唤醒。
原G1–G4与普通线程路径须回归。按作者要求待全部剩余实现完成后统一运行测试。
