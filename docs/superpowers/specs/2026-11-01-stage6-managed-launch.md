---
id: spec-stage6-managed-launch
kind: specification
authority: scoped-mechanism-design
status: implementation-contract-not-exit-signoff
lifecycle: current
updated: 2026-11-06
---

# S6-M2/M3：受管理的停车、装载与 TLS

## 范围与后继关系

用户已授权完成阶段6全部剩余工作。本切片承接[Gate v1 协议](2026-10-29-stage6-m1-protocol.md)
§1的可信停车义务、§4允许串行化映射事务的边界，以及[D17](2026-10-29-stage6-m1-mechanism-decisions.md)
追加79+的规则。不改变73–78、普通Task创建、rax-only答复、独占root、信号保留或fork/exec的v1立场。
这份实现契约不构成组织级签署或执行证据；测试统一完成前不得勾选阶段条件。

## 停车与初始装载

- `task_create_held(request)`追加79，沿用11字请求，只接受空initial_caps；成功返回管理能力。
  新Task在同一不可调度事务内进入`Prepared`并标记`launch_held`。它不是prepared fork，不产生fork ticket。
  普通scheduler不能运行Prepared；信号不能解除停车。创建失败沿用task_create的失败边界。
- `task_load_image(task_cap, descriptor)`追加80。调用者需实际持有TaskManageVSpace，目标须为
  本切片创建的Prepared停车任务、无Gate、无线程。descriptor为6个u64：
  `image_ptr, image_len, argv_ptr, argc, env_ptr, envc`，全部源内存属于装载服务。
  复用通用ELF计划、初始栈和私有exec Segment；参数和env各≤16条、每字符串≤127字节，
  映像含八页栈≤640页。先完成全部验证和资源分配，再替换目标映像；普通失败保留旧目标root/帧/停车状态。
  失败清理必须在大构造栈退出后进行。成功仍保持Prepared。
- `task_start(task_cap, gate_or_zero, orphan_value, flags)`追加81。flags=0启动原生任务，后两项须0；
  flags=1先按现行Gate授权/容量协议挂接，再发布Ready。gate_or_zero=0创建Gate，否则加入已有Gate。
  成功返回Gate能力（原生启动返回0）。挂接与Prepared→Ready不可调度；失败恢复Prepared且不执行客户端。
  所有已有root共享、待领取原生返回和服务环检查保持；保存帧必须已有合法用户入口/栈映射。
- Prepared停车任务可由管理者显式销毁，复用正常root/能力/私有映像清理；prepared fork仍只能由其ticket协议处理。
  停车任务的普通map/unmap/protect允许管理者在启动前准备内存；不开放prepared fork映像。
- `task_info(8,0)`返回当前Task结构性Namespace能力的本地句柄，复用已有Authority，不铸造新权利。

## TLS与请求期间的VM事务

通用boot manifest另接受v3：`magic,version=3,count,flags`后接原有三字目录项。
flags bit0声明首个服务预留旧平台Task4见证区；未知位拒绝。v2保留历史隐式预留约定。
Linux装载服务使用v3 flags=0，通用加载器不得在其首个服务页写入Task4机器码。
这不按kind解释服务或personality，不改变v2输入，也不让只读制品数据承担隐含探针空间。
聚合boot服务的外部帧向量预算192→768以容纳原始2MiB级制品载体；普通Segment149页、
私有exec640页、全局数据2048帧和页表256帧不变，不扩内核栈值的内联帧向量。

**后继容量修订（RV-3，2026-11-06）**：上段768页是本切片初始预算。
[统一验收](../reports/2026-11-02-stage6-process-and-differential.md)所用服务制品已达3 159 672 B，
需772页，超过768页（3 145 728 B）；实现已将boot服务外部帧arena扩到1024页。
当前契约以1024为该项预算，保留192→768的历史与此次契约滞后记录；普通段149页、私有exec640页、
全局物理预算及内核栈内联向量均不变。这不是扩大单个ELF装载额度，后续再变更须更新本契约并重验。
发现与整改见[审查](../reports/2026-11-06-stage6-implementation-review.md)、[整改报告](../reports/2026-11-06-stage6-review-remediation.md)。


- `task_set_tls(task_cap, fs_base)`追加82；其余参数0。调用者需TaskManageVSpace。
  base只接受低半区规范地址，包括0。目标只能是Ready、launch-held Prepared，或固定服务者正在处理的
  Delivered Gate请求的客户端；拒绝运行中他人、旧代次、孤儿绑定和prepared fork。此操作不答复请求。
  设置保存的FS base；调度时由既有平台路径恢复。记录调用者、目标cookie和base。
- Gate客户端的map/unmap/protect/access-enable只有在固定服务者实际持有该Task管理能力且该客户端的请求
  为Delivered时才允许。每次操作与既有真实页表/Segment/审计提交串行化；其它管理者仍拒绝。
  不更换root，不允许共享root。代理复制每次重新验证实时逻辑映射、PTE和root代次，物理计划不能跨调用保存。
  这是对前序“所有管理者均拒绝”的局部扩展，不是临时映射授权或撤销机制。
- 79–82的普通拒绝记录原始参数、调用者cookie和原生错误；审计容量不足时在业务变更前全局fail-stop。
  start须一次预检挂接与发布的完整成本；不得在挂接成功后因缺一条状态日志留下半启动。

## 验收义务

新增host用例须覆盖停车期间不可调度/不可被通知发布、装载失败保留root/帧、重复启动、错误管理者、
prepared fork混淆、挂接容量失败保持停车、销毁回收、TLS地址和身份、Delivered前后及孤儿VM修改拒绝。
增加真实ring3装载/停车/TLS与双root探针；冻结busybox真实执行另由用户态LibOS验收。
计量、host、QEMU与Linux参考测试按作者要求在剩余实现完成后统一执行；编译不替代上述运行证据。

共享root、整帧答复、门控fork/exec与Linux信号续体不在本切片；它们仍须按D18/D20和协议§6单独定义后继。
