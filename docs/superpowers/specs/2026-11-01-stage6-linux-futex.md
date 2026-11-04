---
id: spec-stage6-linux-futex
kind: specification
authority: scoped-personality-design
status: implementation-contract-not-exit-signoff
lifecycle: current
updated: 2026-11-01
---

# S6-M4：用户态 clone、TLS 与 futex 等待

依赖[共享Gate线程](2026-11-01-stage6-gate-threads.md)。Linux clone/futex位与errno只在prima-user。
这是单CPU、每进程至多8线程/8等待的有界profile契约，非完整Linux兼容或执行证据。

- clone(56)仅接受VM|FS|FILES|SIGHAND|THREAD|SYSVSEM，及SETTLS/PARENT_SETTID/CHILD_SETTID/
  CHILD_CLEARTID/历史DETACHED可选位；其它合法但未实现变体ENOSYS，组合不自洽EINVAL。
  从父Gate帧构造rax=0、指定子栈；FS独立，未SETTLS则继承父FS。用中立spawn的两个可选输出
  在同一内核区间写出tid；父返回子tid。客户端不接触原生clone分派。
- getpid返回组leader，gettid返回当前Task id；所有生命周期查找使用完整cookie。
  arch_prctl与set_tid_address按线程记录；退出尝试清零clear_tid并唤醒同进程匹配futex。
- futex(202)支持WAIT/WAKE、WAIT_BITSET/WAKE_BITSET及PRIVATE位；键为本Process的共享root域+4字节对齐VA。
  普通/PRIVATE在本域内均可使用，不声称跨独立Process共享futex。bitset须非零。
  WAIT先完整读当前u32，不相等EAGAIN；相等保留真实Delivered请求并登记，不答复0模拟等待。
  WAKE按计数/bitset终结已登记请求，返回真实唤醒数量。服务单线程串行化比较、登记和wake处理；
  客户端的wake trap只能排队，不能越过服务正在提交的登记。没有“提前wake令牌”。
- 非空timeout目前仅支持有效零timespec立即ETIMEDOUT；非零时限ENOSYS，不伪造定时器。
  非法timespec/计数/对齐EINVAL，坏指针EFAULT，未知op ENOSYS；CLOCK_REALTIME只容于WAIT_BITSET。
- 非leader exit(60)只销毁该线程；leader先退出则保留其Delivered终止续体直到其它线程结束。
  exit_group(231)清理全组，先子线程后leader/root；服务清理期间不再向该组处理普通业务。
  fd/匿名段按组回收；线程退出不单独卸载共享映像。

P2须使用同一原始ELF在Prima和pinned Linux guest：父子不同FS值均立即读回、真实WAIT/WAKE计数、
先写/唤醒再等待得到EAGAIN、错值EAGAIN、坏地址/对齐、bitset，以及clear_tid退出唤醒。
S6-M4静态pthread/fork+exec与信号打断futex仍须后续矩阵；P2不替代它们。
信号打断及SA_RESTART续体依照[信号契约](2026-11-01-stage6-linux-signals.md)，不调用原生信号分派。
pthread栈允许匿名PRIVATE|ANONYMOUS加MAP_STACK（该位不改变映射语义），PROT_NONE复用原生68禁用用户位；
恢复保护先以41校验/设置非空逻辑权限，再以68启用用户位，保留物理页与COW。两步非原子事务，
后一步失败不会伪造成功；匿名页仍只有RW授权，执行映射EACCES。每段149页上限保持。
所有测试按作者要求在全部实现后统一执行。

语义来源：[FUTEX_WAIT](https://man7.org/linux/man-pages/man2/FUTEX_WAIT.2const.html)、
[FUTEX_WAKE](https://man7.org/linux/man-pages/man2/FUTEX_WAKE.2const.html)、
[clone](https://man7.org/linux/man-pages/man2/clone.2.html)。
