---
id: report-stage6-process-and-differential
kind: report
authority: scoped-implementation-and-validation
status: verified-with-bounded-exit-review
lifecycle: current
updated: 2026-11-04
---

# 阶段6：进程、线程信号、逐调用差分与统一验收

## 当前结论

承接[上下文与P1/P2构建](2026-11-01-stage6-context-signals-threads.md)。基线仍为
`4335e184ef5dd10679d72ecf6a0189b7672d9cd2`加工作树修改。
本轮补齐剩余验收所需源码：线程信号/中断futex/SA_RESTART、静态pthread、受管COW进程复制与
映像替换、用户态wait4、逐调用矩阵、原始内核审计读取和差分计数核对。
最终源码统一验收通过，阶段6技术退出条件4/4已满足；冻结有界Linux profile v1及D12八成员运行结论。
范围只覆盖登记变体、命名谓词和白名单，不构成完整Linux兼容或外部组织认证；历史失败与整改链保留。
测试遵照作者“全部完成后统一运行”的顺序，开发期间只编译、Clippy和镜像构建。

## 当前统一检查表

| 命令 | 实际结果与范围 |
|---|---|
| `cargo test --workspace` | 退出0；1118通过、0失败、1默认忽略 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox cargo test --workspace --features xtask/linux-probe` | 退出0；1119通过、0失败、1默认忽略 |
| `PRIMA_ELF_SAMPLE=/usr/bin/busybox cargo test -p kernel --lib -- --ignored` | 退出0；上述外部ELF演练单独1通过 |
| `cargo xtask lint` | 退出0；fmt、Clippy、裸机构建、41文件零unsafe |
| `PRIMA_LINUX_ELF=/usr/bin/busybox cargo clippy --workspace --all-targets --features xtask/linux-probe -- -D warnings` | 退出0 |
| `cargo run -p xtask --features gate-probe -- test-gate` | 退出0；G1双轮 |
| 上条分别设`PRIMA_GATE_CASE=G2/G2-death/G3/G4` | 均退出0、各双轮；G4各1000实请求，非空循环 |
| `PRIMA_GATE_AUDIT_SITE=<site> cargo run -p xtask --features gate-audit-probe -- test-gate-audit` | 七site均命令退出0、各双轮预期QEMU退出35且unchanged=1 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -p xtask --features linux-probe -- test-linux` | 退出0；pinned Linux参考11项；Prima11项各双轮，stdout/stderr/status/审计均相符 |
| `cargo xtask test`（允许本机loopback的执行环境） | 退出0；单次完整36组、72/72轮；首次退出1及11项失败保留于下节 |
| `cargo run -p cosir --example stage6_domains` | 退出0；两根/两空间、跨域拒绝、单侧销毁后另一侧继续；模型非机器 |
| `PRIMA_STAGE6_DOMAINS=1 cargo xtask test-domains` | 最终退出0、双轮通过；前序覆盖失败与整改保留于下节 |
| `elan run leanprover/lean4:v4.35.0 lake build`（lean目录） | 退出0；11 jobs；不增加新Gate/进程协议的证明主张 |
| `cargo xtask tcb` | 退出0；2087/38424、5.15%、unsafe79/0、耦合0/false/0/52 |
| `python3 scripts/docs.py self-test`、`build`、`check`、`check --base 4335e184ef5dd10679d72ecf6a0189b7672d9cd2` | 全部退出0；生成目录与新增文档元数据/链接校验通过 |

日志留在/tmp/prima-stage6-unified-*与ignored target，未提交日志/镜像/哈希附件。
本机无`/dev/kvm`，计量只有TCG；没有实机数据或延迟门槛承诺。

## 行为与边界

- [87/88](../specs/2026-11-02-stage6-gate-process.md)同时要求独立上下文、实际Task管理权限和Delivered身份。
  fork复用现有COW快照/失败恢复，复制保存GPR/FS/FXSAVE，子绑定先于可调度；原生fork/exec门控拒绝保持。
  受管复制不进入原生wait4亲缘/僵尸，用户表以完整cookie维护亲缘与状态。
  exec复用完整ELF构建与提交，替换root后刷新Gate绑定代次并一次终结旧请求，FS/FXSAVE重置。
- 家族最多4进程、每组8线程；OpenFiles现有owner机制共享继承OFD游标，fd/CLOEXEC逐进程独立。
  服务持有的匿名段根按家族owner位计数，最后一个映射家族成员释放后才销毁。
  wait4仅实际子pid/-1及WNOHANG；状态输出EFAULT仍消费已退出子进程（按参考修正）；无rusage、SIGCHLD或wait信号打断。
  exec仅登记/busybox、/bin/busybox和/bin/M4-child；不是任意大文件VFS执行或其它clone进程变体。
- 线程独立mask/pending/活动帧，共享sigaction；clone线程只继承mask，进程复制保留活动帧但清pending。
  kill/tgkill只在本组，进程pending只交付一个未屏蔽成员。真实futex/sigsuspend等待可中断，
  SA_RESTART验证原syscall/int80指令后保存原号/PC，sigreturn后重新比较；Ready线程在Gate边界交付。
  SA_ONSTACK在未配置备用栈时走普通栈；备用栈、实时队列和任意指令异步交付未实现。
- PROT_NONE复用68的真实用户位禁用/恢复，MAP_STACK不改变匿名映射语义。复核发现旧通用map/protect
  和私有ELF只记录逻辑X，已把X同步到硬件NX，ELF栈始终NX；保留其它PTE/COW位。
  现有bootloader启用EFER.NXE。此次改变需要统一重跑阶段5回归，不能只据静态代码宣称硬件验收完成。
- 原始制品载体加入静态glibc pthread与其它探针后达到3159672字节，越过旧768页（3145728字节）。
  仅boot服务外部帧arena扩为1024页；普通段149页、私有ELF640页与全局物理预算保持。
  这不是扩大单个Linux ELF装载额度；运行服务复用一个Gate，家族均经其固定服务者处理。
- 89仅按GateServe读取本Gate的下一条trap/delivered/terminal原始记录，不解释Linux号或errno。
  服务逐案例从内核审计导出trap/交付/终结计数，以及abort对应的原号、参数、cookie和不透明值；
  宿主核对ENOSYS服务记录与原始审计一一计数，拒绝缺失、重复、未知cookie和未闭合终结。

## 同一制品矩阵

| 范围 | 制品与检查 |
|---|---|
| D12白名单 | pinned busybox原字节六applet、P1与P2；定义不扩大 |
| S6-M4 pthread | 普通Linux头文件/静态glibc的pthread.c，无Prima钩子；create/join、独立TLS、真实futex EINTR；第二阶段从handler ucontext验证SA_RESTART原号/指令/地址，再观察重启EAGAIN |
| S6-M4进程 | process.S与process-child.S；父子返回/私有写/共享OFD/CLOEXEC/argv/env/退出23/wait状态；第二子进程EFAULT后ECHILD |
| S6-M3逐调用 | matrix.c打印稳定裸返回和errno；pid/tid/地址及声明差异字段按命名谓词比较，非全参数空间/全结构等价 |

11个顶层案例使用同一已构建ELF进入Prima与pinned Linux 6.8.0-31 guest，逐字比较stdout/stderr/status；
M4-child只由exec启动。Prima每案例独立启动双轮、每次30秒；参考独立180秒，栈限制32KiB。不使用宿主执行替代guest。
探针编译中的strip-all仅用于新构建的探针；D12 busybox未重链接、修改或剥离。

## 验证记录（按执行顺序保留中间状态）

`cargo fmt --all`与Linux特性Clippy已通过；新增host用例只编译，涵盖双授权、单线程、root代次、
审计读取范围及原始值、线程/进程不同信号继承和NX保留位。开发期修正遗漏Shared实参与固定分块Clippy错误。
原始日志留/tmp和ignored target；未提交日志或制品。统一测试结果、失败及修复将在下方按实际执行补入。

待统一执行：workspace测试、普通lint、Linux特性host测试、G1/G2/G2-death/G3/G4、七个audit-stop边界、
11项pinned guest差分、完整普通BIOS回归、Lean/TCB及文档self-test/build/check/base-check。
只有这些实际结果、profile逐行范围裁定与退出复核完成后，才能更新阶段勾选。

### 统一验证：首轮失败与修复

- 首轮普通和Linux特性workspace均退出101：共享root负例使用Task 18，而调度数组合法索引小于18，
  `set_root(18)`不建立别名，断言错误期待Busy。改用17后，内核551通过、1忽略；未放宽生产检查。
- 次轮退出101：`Shared::new`在公共VFS默认只读根下创建fixture，得到EACCES。
  仅用户态测试根在初始化期间改755，构建目录/文件后恢复555；公共VFS规则不变。
- 修复后`CARGO_NET_OFFLINE=true cargo test --workspace`退出0，共1115通过；
  `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo test --workspace --features xtask/linux-probe`
  退出0，共1116通过。原始输出分别在`/tmp/prima-stage6-unified-workspace-r3.log`和`linux-host-r3.log`。
- 普通`cargo xtask lint`退出0：fmt、Clippy、裸机构建与41文件零unsafe均通过。
  `elan run leanprover/lean4:v4.35.0 lake build`退出0，11 jobs；仍只证明Lean映照列出的模型范围。
- `cargo xtask tcb`退出0：trusted=2087、mechanism=38192、ratio=5.18%，mechanism unsafe=0；
  内核include_bytes=0、prima-user依赖=false、service_name_lines=0，protocol_lines=52。
- pinned guest首次因沙箱DNS失败退出1，按权限流程联网重跑下载固定包，仍强制既有SHA512校验。
  Linux镜像使用自己的严格S6解析器，不套用普通服务协议；普通捕获校验保持原状。
  Gate/Gate-audit镜像分别置于独立ignored输出子目录，避免参考下载期间重建探针覆盖等待中的Linux镜像。

上述为中间结果，后续源码修复后仍需相应重跑。QEMU/差分尚无通过结论。

- G1与G2各双轮通过；初次Gate构建因本工作树缺musl快照失败，恢复本机缓存后验证锁定commit
  `0784374d561435f7c787a555aeab8ede699ed298`与干净状态，重新构建sysroot通过。
- 参考guest首轮M4-fork/M3退出1：原探针错误期待wait4的EFAULT保留僵尸，以及未挂载proc。
  根据[Linux 6.8 kernel_wait4](https://github.com/torvalds/linux/blob/v6.8/kernel/exit.c)的do_wait/put_user顺序
  修改用户态语义与契约，探针分开验证正常wait状态和第二子进程的EFAULT消费；参考fixture挂载proc。
  下一次pinned guest全部11案例退出0，包含pthread的EINTR/SA_RESTART与完整M3。
- Prima独立诊断及首轮正式运行均在30秒到达cow_ready后超时，未取得Linux案例通过。
  排查通用boot ELF载入发现逐字节重复查询外部arena帧，3MiB制品载体放大该成本；
  改为逐页解析帧、复用既有普通RAM write_bytes/fill_zero，保持源范围/页边界/非连续帧检查。
  增加非页对齐跨三页/非连续物理帧/BSS清零/前缀不变host反例；不增加时限。
- 后续CPU轨迹否定“该复制成本导致首个超时”的因果推断：init写了未映射的`+0x7000`，
  故障后Task2永久等待旧协议。改为复用prima-initgen导出的可写USER_Y_VA（+0x3000）。
  批量复制只作为大载体的实现改进保留，不充当该故障的修复证据。
- 到达S6BEGIN后出现kernel栈保护页→double/triple fault。反汇编定位create_segment_object及Arena::set，
  调用链同时持有控制入口输出scratch、load_managed_image、prepare_exec_image页数组与分配返回值。
  将装载操作移出其它控制操作的scratch帧；仍越界后将初始栈暂存页移入显式单CPU、不可调度的事务scratch，
  保留32KiB内核栈/保护页。之后实际执行true、echo、cat、ls、uname；临时boot诊断用于定位，不作为退出见证。
- ls实际openat使用O_NONBLOCK|O_DIRECTORY|O_CLOEXEC。有限RAM文件/目录无阻塞，增加该合法变体并在共享OFD保留GETFL位；
  不添加异步设备语义。89漏读已有GateCancelled事件，导致终结计数少1；已映射为terminal/Cancelled，新增销毁后的读取反例通过。
- 单次启动顺序11案例于uname审计输出时达到30秒。改为每案例独立启动、每案例双轮，仍用同一ELF和30秒门限，
  原始stdout/stderr/status、内核失败审计及计数仍逐项严格核对。选择项只经现有TaskSpec不透明name/入口rcx传入服务，内核不认识案例。
  各组镜像输出隔离，普通72项回归同时启动；不以超时放宽或删除检查解决。
- G4首轮异常：初次把G1故意传入的非法保留位误作准入标志，返回EINVAL；恢复零保留位并清除平台Task 2的残留恢复帧后挂接成功。后继仍在服务循环末尾异常，继续诊断，尚无计量通过主张。
- P1首先在溢出地址write上失败：运行时Gate复制封装将地址溢出与超长拷贝都归InvalidInput，使Linux映射为EINVAL。地址溢出改为Fault，长度超限保持InvalidInput，更新已有边界用例。之后越过首个失败，但信号路径出现三重故障，待定位。

- G4继续定位发现累计能力历史容量512在478次请求后耗尽（平台已占34）；旧探针未检查wait/reply错误，错误循环后UD2。
  两处结果加断言；历史容量扩至1280，完整保留已退休身份和祖先链，满额仍拒绝，不循环覆盖。
  audit查询改用不可调度静态scratch构造完整链，仅把小摘要移出syscall，避免链随容量增大压垮32KiB栈。
  G4双轮各1000样本通过：TCG/pre-iret总耗时p50/p99分别4617275/7964429ns、3534935/5837699ns；
  四分段原始时间戳与三类各1000条Gate事件均验证。多QEMU并行宿主环境，非真实硬件性能主张，无KVM数据。
  挂接后的初次yield只排除初始空等设置，不改变请求路径；每次均经真实trap、交付、答复和恢复。
- P1 CPU轨迹定位上下文答复的深调用栈：gate_frame_call同时承载GPR/FXSAVE暂存，外层通用控制闭包仍保留fork/audit scratch。
  将frame/copy分派置于该闭包前，保持保留位、授权、root、用户范围、失败审计与返回检查；继续实跑验证。

- 移出通用scratch后Prima首轮true/echo/cat/ls/uname/sort/P1/P2/M4/M4-fork均通过。
  M3逐值已输出至最终unknown.enosys，但审计收尾达到30秒；未据此计M3 PASS。
  原服务为每个abort从头再次读取审计，改为一次扫描保存最多18个活跃Task请求，终结时匹配并移除；
  仍逐条读取生产审计、核对原始号/参数/值及终结计数，并新增重复trap、无trap终结和残留请求断言。
  不删除审计、不减少测试、不增加时限；新一轮使用1280历史容量及最终查询/拷贝路径。
- 首次普通完整套件记录sbase缺缓存失败；恢复锁定干净快照后后续cat/ls可构建。
  Linux N1参考使用宿主UDP socket，受沙箱网络限制首个socket失败；最终完整回归需按权限流程在允许该参考网络能力的环境运行。
  C1第二轮30秒超时同时保留；首轮通过不替代双轮，最终全套仍待。

- 最终host源码检查：普通workspace1117通过、Linux特性1118通过，均1项既有忽略、0失败；
  普通lint与Linux特性all-targets Clippy退出0。能力历史满额拒绝/历史保留新反例通过，完整链scratch复用与旧查询相等检查通过。
- G1/G2/G2-death/G3最终各双轮PASS；七边界int80-entry/dispatch/int80-return/fast-return/pick-next/resume/irq
  各双轮在预期审计fail-stop退出35，unchanged=1。与正常退出33区分，未将panic任意视作通过。
- 单次扫描后的M3仍在多组QEMU并行时30秒超时，保留该失败；随后40秒上限CPU诊断中完成，
  trapped/delivered=445、replied=443、aborted=1、cancelled=1、status=0，无残留请求。
  诊断不计验收或一般性能保证；全部其它QEMU组完成后重新运行正式11项双轮，30秒门限不变。
- 首次普通全套最终退出1，共11项失败检查：sbase缺缓存对应Linux echo/cat-ls和E1–E6；
  N1与R3宿主参考因本机UDP权限被沙箱拒绝；C1第二轮超时。其它成功不能拼成完整PASS。
  按权限流程重跑独立N1参考退出0，11个布尔项全1，确认该失败为沙箱环境差异；R3也在UdpSocket::bind失败。
  最终普通全套须在允许本机loopback的环境下独立执行，避免QEMU资源争用。

### 最终普通回归

恢复锁定干净缓存、完成上述源码修复后，在允许宿主loopback参考的执行环境，
其它QEMU组停止后运行`CARGO_NET_OFFLINE=true cargo xtask test`，退出0。
原始输出为`/tmp/prima-stage6-unified-bios-final.log`，末尾为：

```text
PASS qemu bios R3 run=1
PASS qemu bios R3 run=2
PASS full BIOS suite: 36 programs, 72 runs
PASS stage1 boot slice
```

这是单次完整命令通过，包含默认启动、E1–E6、V1–V4、N1、W系列、I1、C1/L1–L3和R1–R3，
不是拼接分段结果。每次30秒门限及stdout/审计/归账断言保留。
首次缺缓存、沙箱UDP失败和C1超时，以及Linux并行资源争用下的M3超时仍按前节保留；
后继通过不证明所有历史超时的唯一原因。

### D14后继失败与平台映像覆盖修复（2026-11-04）

上述72/72通过后复跑D14，命令退出1、QEMU退出35，`S6DOM|FAIL`；不能以历史D14通过代替本次结果。
临时诊断确认两客户端初始化成功、跨域拒绝与私有内存保持，首个自身根的log解析却返回NotFound。
管理器自身可解析相同绑定；客户端静态字符串地址读出指令字节。栈内动态构造相同路径时三步均通过，
但这只是诊断，不作为修复或验收。另一次改动诊断出现cow.rs“fault requires a Ready successor”panic，仍记失败。
曾怀疑copy_in的根选择；临时改物理读取仍失败，已撤回，不把该推断写成原因。

实际原因是普通平台配置在首个服务映像`+0x800`写入task4自毁见证；当前ALPHA的PT_LOAD只读数据已占据该处。
此前“重定位已消费、可覆盖”的历史假设不成立，会损坏仍在使用的字符串。本轮将见证移到`+0x1800`，
写入前检查该范围处于首个映像页数之内，且不与任意PT_LOAD的memsz范围（含只读数据和BSS）相交；
不符合则确定拒绝启动，不能静默找地址或覆盖数据。一般Linux manifest仍不启用平台见证。
写入按实际页号和页内偏移取帧，initgen同步入口偏移；生成码指纹随已审阅立即数更新为`bd0a8870c2389d8b`。
新增host反例拒绝只读段/BSS覆盖和截断程序头，接受边界外空隙。

临时客户端/管理器诊断、栈路径绕过均已撤除。原静态字符串和原三步判据下，整改D14命令退出0：

```text
PASS S6DOM round=1 roots=2 shared_log=3 denied=3 survivor=1
PASS S6DOM round=2 roots=2 shared_log=3 denied=3 survivor=1
```

对应`/tmp/prima-stage6-domains-hole-fixed.log`。此前72/72是整改前的完整证据；整改后重新运行完整BIOS、
host/lint、Gate/审计、Linux差分与D14，不能把整改前结果直接写成最终源码全绿。
会话切换可能清理/tmp；本报告保留已观察的必要结果与失败链，不将临时输出路径当作永久附件。

### 平台修复后的参考检查失败与复跑

平台覆盖修复后的第一条完整命令，72个QEMU轮次均通过，但命令退出1：宿主Linux W9的
`SIGACT`只在`eintr=0`失败。原探针子进程fork后立即发信号并退出，信号可能在父进程进入waitpid前处理，
因此正常Linux也不保证该次waitpid返回EINTR。两端同源探针增加20ms nanosleep窗口后再发信号，
仍要求真实EINTR、SA_RESTART行为、信号处理和子退出状态，不重试、不删断言、不放宽QEMU时限。
这提供进入等待的调度窗口，不承诺无界宿主停顿下的确定时序。
补充启动范围预检时曾因usize/u64混用导致编译退出101；已改为页数与向上取整后的页数比较，避免字节乘法。

会话恢复后，普通沙箱完整命令的N1宿主UDP参考被拒绝，明确中断该条命令（退出130），不计通过。
按权限流程在允许loopback的环境重跑；正式原始记录统一存ignored `target/stage6-validation/`，
完整命令输出为`bios-final.log`，后续串行组的命令退出状态为`queue.log`；不提交这些运行文件。
此前各次已通过结果保留其当时源码范围，最终源码是否通过以接下来的实测汇总为准。

后续队列首次误设`PRIMA_GATE_CASE=G1`，进入只接受G2/G2-death/G3的矩阵入口后断言失败，命令退出1。
G1应不设置该变量；修正调用后复跑，不修改生产断言。错误调用输出另留ignored `G1-invalid-env.log`。

### M1e计量结果

命令为`PRIMA_GATE_CASE=G4 CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-probe -- test-gate`，退出0。
每轮1000次生产int80请求，以下每格为nearest-rank p50/p99，单位ns；总量由每次首末时间戳独立计算。

| 轮次 | trap→queue | queue→deliver | deliver→reply | reply→resume | 总量 |
|---|---|---|---|---|---|
| 1 | 132220/208531 | 1015589/1432510 | 521993/817248 | 288014/447273 | 1971774/2827933 |
| 2 | 121514/222328 | 936485/1368245 | 486934/794763 | 263639/452440 | 1825690/2679585 |

最终源码复测PIT/TSC校准分别为2085382940Hz和2085748335Hz。测量从Gate适配器入口至进入iret跳板前，
不含硬件陷入入口和最终iret/用户态继续；queue→deliver含服务恢复和wait重试，reply→resume含让出和调度。
每轮验证1000组有序原始样本及trap/delivered/replied各1000条；输出不进入采样区间。
原始样本保存在ignored `target/stage6-validation/G4-samples-1.log`与`G4-samples-2.log`，命令摘要在同目录G4.log。
这是平台覆盖修复后、其它QEMU组静止时的串行复测；前序并发宿主环境的较慢样本总量仍保留于失败链。
仅为TCG基线，无KVM/实机数据，不据此冻结性能门槛。

### 输入身份

工具为pinned nightly-2026-07-30（rustc 1.99.0-nightly 1a833e165）、GCC13.3.0，
QEMU8.2.2/pc-i440fx-8.2/qemu64/TCG单线程/1CPU/128MiB，无KVM或实机结果。
同一编译制品原字节在两端使用；以下SHA-256包含构建后的探针，不宣称跨工具链重现同一哈希。

| 制品 | SHA-256 |
|---|---|
| D12 busybox | dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14 |
| P1 | 4884b689a0771d60e4018fd0dbd9d29ecefcca3056b5532bec7f601e5d2d9263 |
| P2 | a0108371f8efb0165595f3ab53f05c1b9ab795fada05c1f80614a51899d262c2 |
| M4 pthread | c42fb2c1520ef8d9d96865b741839a1b6d31633b5e0176b3b8c0c18ec9848a48 |
| M4 process | bd6012d9f23bf9117e462c18001d21e8fd1e0ad9bd22f8d82cf97d07b6dbbe7b |
| M4 child | 8d404753572d63a134d31c410e62b77eaf8ad952567cdf7bb1eb7852e1c5883f |
| M3 | 928f9046c29bcec9c833055716926fbd6dae25358087596bc0a345f9ecfc1639 |

- 其它QEMU组完成后的正式差分命令退出0：pinned Linux参考11项通过，Prima每项独立启动、各双轮全部通过，
  两轮均输出`PASS S6LINUX ... pinned=1 applets=6 probes=2 pthread=1 fork_exec=1 syscall_matrix=1 stdout=1 stderr=1 exit=1`。
  M3 stdout逐值相等，审计445/445/443/1/0/1闭合；每次30秒门限不变。原始输出为`/tmp/prima-stage6-unified-linux-r13.log`。
  默认忽略的外部ELF演练另用PRIMA_ELF_SAMPLE=/usr/bin/busybox执行，1通过、0忽略。
- 最终TCB复测退出0：trusted=2087、mechanism=38353、ratio=5.16%，unsafe=79/0；
  include_bytes=0、prima_user_dep=false、service_name_lines=0、protocol_lines=52。docs self-test退出0。

### 出口技术复核范围

本轮对照阶段定义和实验机制契约进行源码/证据复核；这不是外部组织认证。

- 新增79–89只解释原生能力、Task、root、帧、ELF字节与原始审计；kernel/cosir没有新增Linux号表、
  CLONE_*、futex队列、fd、Linux信号帧或busybox识别。Linux策略集中在prima-user的linux模块，
  TCB耦合检查与源码diff复核相互补充，不以关键字检索单独证明架构隔离。既有原生upcall/wait接口不被冒称Linux ABI。
- TaskContext必须与Delivered请求、固定服务者、客户端完整cookie、活root同时授权；
  86/87返回的新管理根显式包含TaskContext（源服务已具备该授权），旧根不自动扩权；83仍可另建独立根。
  进程规格原“服务管理/上下文根”表述已澄清为该组合新根，未在代码中隐式扩大旧权限。
- 首请求实际拒绝管理根冒充上下文、ring0选择子、IOPL、NX栈作为PC及非法MXCSR，拒绝后保存帧不变；
  P1证明实际用户DF、callee-saved寄存器和XMM经Linux信号往返恢复。通用跨页、只读、COW/隐藏页代理边界由G1/G3及host反例限定，
  不宣称逐入口全部硬件故障组合或完整Linux信号ABI等价。
- 生命周期通过G2、G2-death、能力丢失host矩阵和每个差分案例的原始终结计数验证；
  事件满额在七边界停止，历史满额确定拒绝。单CPU不可调度区间与静态scratch是当前前提，SMP前必须重审。
- profile只冻结列出的变体和命名谓词；进程多代/孤儿收养、并发僵尸PID保留/编号复用等不在当前差分结论中，
  不能由单父子见证推出完整pid命名空间。XSAVE/AVX、实时信号/备用栈、非零futex时限与跨独立进程共享、
  任意VFS exec、动态链接、多LibOS、SMP和持久设备继续按上位阶段边界处理；不通过降低既有退出定义扩张支持范围。

## 最终源码汇总与退出复核（2026-11-04）

`cargo xtask test`在允许loopback的执行环境退出0，单次完整36组、72/72轮；包含W9 Linux参考与Prima双轮，
不拼接前序部分结果。普通/Linux特性workspace分别1118/1119通过、0失败、1默认忽略；
外部ELF演练另行1通过；lint与Linux特性Clippy退出0。

修复后串行复跑G1/G2/G2-death/G3/G4均退出0、各双轮，G4每轮1000真实请求；七个审计停止边界各双轮预期退出35，
unchanged=1。11项同ELF pinned Linux/Prima差分双轮退出0，stdout/stderr/status与原始审计严格核对；
D14原探针双轮退出0，模型示例退出0。命令及原始输出在ignored target/stage6-validation，未提交运行附件。
Lean构建与docs self-test退出0；新79–89协议不增加Lean证明范围。最终文档self-test/build/check/base-check全部退出0，基线为4335e184ef5dd10679d72ecf6a0189b7672d9cd2。

| 退出项 | 技术复核依据 |
|---|---|
| S6-X1 | D12未修改busybox六applet与同源P1/P2八成员逐项双轮通过；哈希和通过谓词不变 |
| S6-X2 | profile v1逐行partial/unsupported裁定；M3命名逐调用观察及M4 pthread/进程矩阵与同源Linux通过；不外推全参数 |
| S6-X3 | 新机制仅解释能力、Task/root/帧/ELF和原始审计；Linux号/errno/fd/futex/信号/亲缘政策仅在用户态；源码复核与TCB耦合指标相符 |
| S6-X4 | 未实现号/变体确定ENOSYS，原始号、六参数、cookie、-38和唯一终结经内核89读回核对；其它拒绝保留对应错误 |

交付D1–D5据上述有界证据完成；白名单和profile按原裁决冻结，不扩大既有阶段定义。
本轮技术退出不启动阶段7，也不构成外部组织签署。动态链接、多LibOS完整隔离、SMP、设备持久化等继续按上位边界处理。

最终TCB复测退出0：trusted=2087、mechanism=38424、total=40511、ratio=5.15%，unsafe=79/0；
include_bytes=0、prima_user_dep=false、service_name_lines=0、protocol_lines=52。
