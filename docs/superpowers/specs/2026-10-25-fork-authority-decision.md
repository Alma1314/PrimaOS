---
id: decision-fork-authority-inheritance
kind: specification
authority: proposed-security-contract
status: user-selected-working-contract
lifecycle: current
updated: 2026-10-27
---

# fork 隔离所需的能力继承裁决

上位：[COSIR §4/§8](../../../COSIR%20原语规范.md)、[M17 工作假设](2026-10-23-stage5-m17-fork-exec-design.md)、
[路线图 D3 决策门](../plans/2026-10-22-stage5-completion-roadmap.md#3-决策门)。
适用于替换 ISSUE-FORK-CAPS-001 的共享 cap_space；用户已于2026-10-25明确选择A，本轮有界实现与技术验收已完成。该选择不是阶段/profile签署或测试证据。

## 为什么需要裁决

现有 fork 让子进程沿用父 leader 的能力空间。直接拆成新空间后，子进程必须得到自己的 token，
否则继承的句柄全部失效。只复制句柄整数、增加同一 token 的多个 holder 都违反当前单一位置契约。
单纯调用 cap_derive 也不够：服务发送凭据与结构根通常没有 Derive；现行 §4.4 必须拒绝这些派生。
此外，父 self-cap 的目标是父 Task，不能把它的普通派生目标改成子 Task（违反 W2）。

## 选项 A：为受控 fork 增加显式继承规则（推荐）

1. 新 Process 拥有独立的句柄视图；复制父视图的外部句柄布局，每个有效槽指向独立的新 token。
2. fork 只复制调用者当前持有的权限，目标与权限不增加。为普通继承 token 保留父记录链接，
   明确新增一种受控 fork 继承审计事件；不伪装成需要 Derive 的普通用户 cap_derive。
3. **契约代价**：允许 fork 继承不含 Derive 的 token，构成现行非委托语义的明确例外。
   必须先在权威语义中记录这个例外，不能声称安全策略完全不变。
4. 父 self-cap 对应的子视图槽绑定新 Task 的 self-cap。它经新 Task 的管理根铸造/派生，
   不创建目标不同的 CDT 边；其来源另由 fork 创建事件记录。
5. 同 Process 的线程仍共享一个视图；fork 的父子视图互相独立，drop/移动/退出不影响对方 token。
6. OFD 保持共享、descriptor/holder 分离。子 close 只释放子 holder，最后一个 holder 才关闭 OFD。
7. 视图项、能力记录、历史/审计、Task/Process、页表及服务会话先统一预检；失败不发布子任务。
   子会话与回复权限按内核记录的来源建立，不能只相信 payload 中的 Task id。
8. wait/退出清理每个新 token、会话与资源；外部 token 和已发消息按 §8.2 保留。

适合保持 POSIX fork 行为；需要显式修改能力委托边界并补模型、host、负例和机器证据。
此选择不批准动态链接、NIC、全部 POSIX 或阶段晋级。

## 选项 B：严格保留 Derive 门槛

仍使用独立视图，但只允许在全部待继承能力都可派生时成功。存在不含 Derive 的必需能力时，
fork 在发布前确定性失败。若为工作负载开放 fork，必须逐项改变其初始授权，增加 Derive，
并重审最小权限与全部消费者。不得静默省略不可派生能力或只在子进程中制造坏句柄。

适合优先维持现行委托模型；当前服务凭据/结构根需要重新授权，无法承诺一般 POSIX fd/能力继承。

## 两个选项都必须保留

单 token 单位置、权限不增加、不可复活的 stale handle、父历史保留、失败原子性、内核零 unsafe。
冻结及安全边界评审与实现验收分开；用户选择只决定实施路径，不构成测试通过。

## Task 槽复用的配套要求

Task 的物理编号可按现有有界调度器复用，但每次创建分配单调 incarnation。
Task Capability 的不可变描述保存目标 incarnation，派生链同时保持目标编号与 incarnation。
授权、派生、发送和Namespace导出按完整目标身份判断存活。复用物理槽不全局删除旧 token，
以免破坏消息/Namespace 载体；旧 token 可接收、审计、丢弃，但不能授权新 Task。
消息创建时保存发送者 incarnation，新接收入口把 `(incarnation, TaskId)` 打包返回；Kernel 为全零。
这不是密码学证明，也不保护同一Process内故意修改接收缓冲区的恶意线程。

## 受控发布的工作细化

用户选定A后，服务交接使用[64准备/交接/发布事务](2026-10-25-process-boundary-followup-design.md#fork-的准备交接发布)。
新增ticket是新Task的创建管理根，沿用Task创建者取得管理权的边界；不改变普通派生的Derive门槛，也不重绑定其它显式Task目标。
Prepared不代表fork已经对调用者成功；服务回复权限与OFD holder先交接，随后才进入Ready和wait可见集合。
这部分已取得本轮host及W6/N1机器有界见证，见[统一报告](../reports/2026-10-25-stage0-5-gap-followup.md)；它是受控发布的实施细化，不扩大本次用户选择到阶段冻结或签署。
