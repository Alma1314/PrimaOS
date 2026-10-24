---
id: spec-asterinas-once-publication
kind: specification
authority: implementation-contract
status: current-contract
lifecycle: current
updated: 2026-10-24
---

# Asterinas 参考续篇：启动信息的一次发布

## 范围与来源

基线 master `26168e52fb267150ff42c254ec8619f8882e3150`。借鉴锁定 Asterinas
`ceb65e3f850096d30283137845ac01ad51dfcdaf` 的 boot INFO/EARLY_INFO、ACPI_INFO、LOGGER
用 Once 整体发布只读信息的设计；本地实现复用既有 IRQ-safe lock，不复制 spin crate 实现，不增加依赖。
[来源登记](../../architecture/asterinas-provenance.md)记录设计输入；参考项目的代理说明不是用户需求。
上位约束仍为[白皮书](../../../Prima%20OS.md)、[原语规范](../../../COSIR%20原语规范.md)§14/W6，
与[当前阶段](../../../Prima%20OS%20实现阶段.md)。

前序 [AE](2026-09-25-framekernel-skeleton-design.md)、[AG 设计](2026-09-25-framekernel-global-state-design.md)、
[AG 报告](../reports/2026-09-25-framekernel-global-state-report.md)、
[AI 报告](../reports/2026-09-25-framekernel-closure-report.md)保留当时的 Once 未接入偏差；
本契约局部后继修订 [OSTD 复用契约](2026-10-24-asterinas-reuse-design.md)中“Once 保持原样”的范围。
只迁移小型自建根描述，帧注册表仍就地初始化，避免历史大对象启动栈溢出。
根构造沿用[自建 CR3 设计](2026-09-24-stage2-self-built-cr3-design.md)与[报告](../reports/2026-09-24-stage2-self-built-cr3-report.md)；
镜像可写/W^X、装载及抽象 image 的细化偏差不由本次一次发布解决。

## Once 成功、失败与借用

- const new、get、get_or_init、set 保留；增加 get_or_try_init。
- 未发布时 get 返回 None（包括初始化正在执行）；发布后 Acquire 读获得完整且不再覆盖的 T，引用不越过 Once 生命周期。
- 慢路径先屏蔽本地 IRQ、取既有锁，再次检查是否就绪；只有胜者执行初始化闭包。
  写入 MaybeUninit 后 Release 发布 ready。就绪快路径无需加锁或改变 IF。
- get_or_try_init 的 Err 不发布值，释放锁/恢复 IF；等待者可以执行自己的初始化。
  unwind 同样释放锁并保持未发布，可重试；本地不引入 poison 状态，依赖 crate 的完整状态机不在复用保证内。
  bare-metal panic=abort 不承诺恢复执行。返回错误或 panic 不回滚闭包的外部副作用。
- set 采用同一锁的 try_lock；已有值或其他初始化者持锁时立即返回 Err(原 value)，不覆盖、不等待。
  未就绪但另一慢路径调用暂持锁时也可返回 Err；失败不代表一定已经发布。返回值所有权属于调用者。
- Once 析构仅在已发布时销毁 T，恰好一次；静态单元通常没有析构时机。
  Sync 仍要求 T: Send + Sync；不提供 reset、共享可变引用或稳定机器布局。
- 初始化闭包不得递归初始化同一单元、阻塞、切换任务或不返回；否则死锁/永久占用。
  NMI/异常重入、跨单元锁环、公平性与真正 SMP bring-up 未解决。
  get 的无等待读仍可供初始化闭包观察未发布状态。

## 自建内核根接线

这里是内部页表根缓存；bootstrap::create_state 的 AlreadyBooted 拒绝与 Authority 初始化仍遵守原契约。
以 Once<KernelRootInfo> 同时发布 root、image_va、image_pages；删除两个独立可覆盖的 StaticCell<Option<...>>。
第一次 build_kernel_root 成功后，后续调用直接返回同一个 root，不执行页表分配/解析，不使用后续参数重建。
首个失败不发布描述，允许重试；普通 Err 沿用 build_self_root 已有的 owned 帧清单与 release_table_frame 回滚。
Once 自身不回滚闭包外部副作用，也没有新增 unwind 页表回收、跨资源事务或失败释放的证明。
读取路径 kernel_root/self_root_witness 从同一不可变描述取得字段；物理页表本体仍受现有可信 VA/权限契约约束。
慢路径锁序为 KernelRoot Once → 帧 REGISTRY → OWNER_ARENA；既有帧入口不回调根初始化。
持初始化锁的代码在返回前完成页表构造，后续 CR3 切换/任务跳转不持此锁。

self_root_ready 添加必需字段 published_once=1：实际运行的见证再次以全零参数调用 build_kernel_root，
要求返回原根、描述不变、前后空闲帧计数相同。该哨兵调用在未就绪时不执行。
xtask 严格拒绝缺字段、0/2 等值；原 root/owned/pages/translated、事件顺序、容量与双轮一致性断言保留。
旧 trace 无此字段不能通过新 validator；无 syscall/能力/磁盘格式变更。

## 验证与边界

host 检查并发只执行一个成功初始化、等待者在错误/异常后重试、初始化中 get/set 不暴露值、
拒绝值所有权、已发布/空单元析构，以及 IRQ 模型下成功/失败/异常的 IF 恢复。
workspace/lint、原完整双轮 BIOS、TCB 与文档检查结果见[报告](../reports/2026-10-24-asterinas-once-publication-report.md)。
不改变 Lean 模型、阶段勾选、profile、BLK-003 或既有恢复细化偏差；没有性能、整体 soundness 或 SMP 证明主张。
修改初始化顺序、IRQ 模型、Once 锁/析构、描述字段、底层页表失败回收或上游版本时重审。
