---
id: spec-gate-request-audit-history
kind: specification
authority: scoped-storage-refinement
status: implementation-contract-not-exit-signoff
lifecycle: current
updated: 2026-11-06
---

# Gate请求根的审计承载历史

## 问题、范围与上位契约

承接[RV-1发现](../reports/2026-11-06-stage6-implementation-review.md)和
[首轮边界登记](../reports/2026-11-06-stage6-review-remediation.md)。
每次Gate交付既写审计又占1280项通用能力历史表，造成单次启动累计交付至多1279次、实际更低。
本修订消除请求根对通用历史槽的重复占用，不改变[原语规范](../../../COSIR%20原语规范.md)
§4.2/§13/W13的历史不可变和只追加义务，也不启动阶段7。
上位机制仍是[Gate协议](2026-10-29-stage6-m1-protocol.md)；73–89布局、授权和一次性终结保持。

## 两处存储、同一逻辑历史

- 每次真实交付仍分配全新单调CapId与带代次句柄，安装同一活动记录和位置。
  根的parent=None，权限仍为Derive/GateServe，目标仍为该次不复用的GateRequest身份。
- 完整不可变根记录（handle、cap_id、parent、holder、rights、target、target_generation、created_at）
  存入既有`GateDelivered`内核审计事件；`CapabilityMinted`事件和created_at的原铸造事件序号保留。
  这是逻辑caps历史的另一物理存储，不是丢弃、覆盖、压缩掉记录或复用CapId。
- 通用历史arena仍为1280项，保存其它创建/派生/继承记录，包括Gate请求的派生别名。
  通用根创建入口保持原路径；只有已预检的生产Gate交付事务使用审计承载根。
- `capability_history_record`查询两处存储；`capability_history_len`返回两者总数。
  初始三根仍由既有InitialCapability表示，len沿用不计这三根的口径，不改其映照。
  私有`cap_history_count`只计通用arena占用，仍用于通用资源预检。
  Namespace历史只在通用arena，原身份不复活规则不变。
- 根drop/完成/取消/孤儿化/销毁后记录仍可查；派生别名可经已drop根验证parent链。
  旧请求句柄在IPC/Namespace容器中的识别复用统一历史查询，不重建位置或授权。
  一个派生链至多含1280个通用记录和一个审计承载请求根，trace上界为1281。

## 事务与资源耗尽

交付前检查活动槽、位置、能力视图和原审计预算，不再检查通用历史槽；新Gate和其它根仍检查通用预算。
安装、出队、容量晋级、铸造/交付审计在同一不可调度提交内完成，不允许暴露缺少历史的中间状态。
事件数量与顺序不变；完整记录嵌入原交付事件，不另占审计条目，不改变89的16字输出。
所有变更前仍须预检全部审计成本；不足时保持队列、旧帧、槽和代次，进入原全局fail-stop。
不得将AuditFull返回为客户端错误或通过请求完成清空审计。

审计池仍为16384条、只追加。解除1279次上限不等于无限交付：调度、陷入、复制和终结均消耗审计，
实际可运行次数取决于事件成本；通用1280项以及活动槽、CapId/句柄代次预算也保留。
本修订不提供审计drain、回收、SMP或长期可用性保证。

## 验收与重审触发

host覆盖1300次真实状态迁移/交付/答复、通用历史已满仍可交付、根drop后别名溯源及旧句柄不可重答。
既有Gate失败原子性、容器退役及七审计停止边界必须保留。
G4保持1000个真实ring3请求和原计量格式，额外校验这些根只占交付审计且首末可查询；宿主拒绝缺失或错误证明标记。
重跑workspace/lint、相关Gate与审计停止QEMU、Linux同源差分及启动回归；host不冒充1300次硬件执行。

重审触发：审计事件被回收、日志布局/大小变化、根新增父链、Gate请求身份复用、历史查询/资源计数规则变化。
实际命令、失败和最终证据由对应后继报告记录；本文不是运行通过或组织级签署。
