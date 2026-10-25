---
id: report-stage5-gap-closure
kind: report
authority: scoped-evidence
status: validated-in-scope
lifecycle: historical
updated: 2026-10-25
---

# 阶段 5 缺口补强验证记录

## 范围与基线

基线 `e305712`，干净 worktree。用户确认按阶段跟踪逐项推进技术缺口、保留待评审项，
最后统一全量测试。本报告只记录本批修改及实跑结果；不能据此声称全部技术缺口已经补齐。
设计见[工作契约](../specs/2026-10-25-stage5-gap-closure-design.md)，
兼容范围见[profile v2 提案](../specs/2026-10-25-stage5-profile-v2.md)。
阶段 5 保持进行中、未签署，阶段 6 未启动，BLK-003 Open，阶段出口勾选不变。

## 修改与根因

1. 启动：原先只留 256 字节给参数块，而 M12 允许更大 argv/envp。现在按实际表/字符串大小
   计算栈偏移并传给 task_create；短参数保留原布局。A2 为独立版本化载体探针，覆盖16/8项、
   长串、空值、空格和 UTF-8；包装阶段拒绝嵌入 NUL。
2. 栈保护：OSTD syscall 栈下新增页对齐保护页，自建根省略其镜像 VA 映射。
   可用容量仍 32 KiB，哨兵仍64字节；QEMU 强制检查当前根 `stack_guard=1`。
   没有证明所有路径峰值、保护任意跳跃越界或给全部 per-Task 中断栈加 guard。
3. 退出：发送者自毁会清除排队的退出状态。现在托管程序报告后阻塞，管理器收到状态后
   先销毁 Task，再回收栈/数据。host 反例保留旧顺序丢消息的事实。
   这是已识别时序问题，不自动解释早于内核输出的历史超时。
4. fork/clone：登记子任务时先保存当前父 FPU，再复制镜像；修正只用初始/旧 FPU 槽的问题。
   W6 增加非默认 MXCSR/x87 控制字的继承、父先改写及等待后恢复验证。
5. Rust：新增独立 `x86_64-prima` target、R1 core/alloc 与 R2 restricted std。
   使用 ignored rust-src 副本和显式 Prima cfg，无 Linux/wasm 伪装；锁定4个补丁输入hash与编译器，
   不修改安装工具链。探针仅支持一次进程生命周期的单调arena，不能把它当作生产 allocator。
   std仅纯内存 API；OS后端/默认System/标准启动初始化不支持，D6保持部分进展。
6. 验证驱动：完整命令执行全部独立组，即使已有失败仍继续其它组，最终统一非零退出。
   每映像仍恰好两轮；保留30秒、全部validator、stdout/IPC/容量检查，不自动重试。
   原31组62轮保留，新增A2/R1/R2后为34组68轮。失败组不计为成功pair。
7. 文档：补齐 profile v2 **提案**，覆盖既有v0/v1行与进程/信号/Rust增量，明确原生机制和libc符号边界。

## 编译过程中的失败

- 初次 workspace check 的 bootloader 子构建在沙箱内解析 rsproxy.cn 失败，外层 `--offline`
  未传递到子 cargo。按工具审批在沙箱外重跑后 check 通过；不算测试通过。
- 直接对 os=none 构建 std 失败：该 nightly 没有相应 allocator、错误、TLS、random 配置。
  补显式 Prima overlay；两个源锚点不唯一的初次脚本拒绝和缺少 alloc_zeroed/TLS guard 配置
  的编译失败均保留在 ignored `target/rust-std-compile*.log`。修订后 std 编译通过。
  编译成功不等于R1/R2机器验收通过。

## 本次实际测试

日志位于本次 worktree 的 ignored `target/gap-validation/`；不提交镜像或依赖副本。
固定 nightly-2026-07-30 / rustc 1.99.0-nightly (1a833e165)，CARGO_BUILD_JOBS=2；
QEMU 8.2.2、pc-i440fx-8.2、qemu64、TCG thread=single、1 CPU、128 MiB、无NIC、30秒门限。

| 命令 | 本次结果 |
|---|---|
| cargo test --workspace | PASS941；含kernel447、OSTD41、xtask208及7项rustdoc/compile_fail |
| cargo xtask lint | PASS fmt / Clippy -Dwarnings / 裸机构建 / kernel29文件零unsafe |
| cargo xtask test | 最终单次PASS：34组68/68轮，退出0；包括全部原31组与A2/R1/R2 |
| python3 scripts/build_rust_probe.py --clippy / --std --clippy | 两种模式均PASS；显式使用patched sysroot，独立target目录 |
| cargo xtask compat | PASS compat cow-write-fault |
| cargo xtask tcb | PASS；trusted1919/mechanism25066/total26985，7.11%；unsafe72/0 |
| docs self-test/build/check/check --base e305712 | PASS307份；目录、ID、主题、链接、锚点及增量治理检查 |

### 新增/补强见证

| 探针 | stdout字节 / FNV-1a | audit_used /8192 | 验收内容 |
|---|---|---|---|
| A2 | 32 /3021568398 | 1083 | argv/envp/auxv全1，Linux及QEMU双轮 |
| W6 | 68 /3526842021 | 1527 | 三种fork路径、四字节status/COW/哨兵/ECHILD及新增FPU全1 |
| R1 | 40 /3230918263 | 1179 | core/alloc/原生输出；页对齐Box、不可满足的大对齐拒绝 |
| R2 | 40 /1595943030 | 1179 | 同上及std内存API、metadata Unsupported、System NULL |

四例均tasks12、caps_live132/peak140/limit160、segment_slots41/limit48，清理后TASKCAP Dead/root_cleared全1。
stdout变化按新增字段和探针解释，保留逐字节、退出码、每映像两轮IPC基线及容量约束。
保护页证据是运行根中映射不存在与哨兵完好；没有故意触发内核栈溢出的机器故障注入。

## 失败记录及整改链

1. 第一完整命令执行全部34组：66/68轮验收PASS，默认两轮均正常退出但被旧IPC阻塞计数172拒绝，实际为173；
   Linux UDP参考在网络受限沙箱内全部失败。保留 `bios-full.log` / `bios-first-errors.log`，命令退出1。
2. 在沙箱外单独执行同一 `net-linux` 参考：逐字段全1，确认该项环境限制。
   第二完整命令同样跑完全部组、66/68轮PASS；默认两轮被旧 `audit_used=1346` 拒绝，实际为1347。
   保留 `bios-final.log` / `bios-second-errors.log`，命令退出1。没有把这些失败轮次写成PASS。
3. 同一新增RECV阻塞只多一条Task状态审计：默认IPC精确改为55/164/173，audit精确改为1347；
   caps/segment/message上限与其余固定值不变。修正后对第二轮捕获的默认完整stdout回放**全部原validator**通过，
   该回放不代替机器重跑。最终统一完整命令单独记录于 `bios-pass.log`。
4. lint先发现全量驱动重构后不再需要的echo包装，再发现新测试对Copy类型调用clone；删除旧包装、改为两个独立arena测试状态后通过。
   没有加allow或放松警告。补充Rust Clippy初次受wrapper名称/编译器sysroot缓存影响误选原始rust-src；
   rustc命名shim、SYSROOT、禁用编译器信息缓存与独立target目录后，两种模式的正式脚本命令均通过。
5. 临时日志回放驱动初次未指定panic=abort而无法链接workspace的abort库；补齐后通过。
   临时驱动因直接引用整个protocol模块有unused告警；不提交该驱动、不压制正式项目lint。

TCB指标含内联测试，是源码代理；比例不构成安全或性能证明。标准库副本/探针不加入内核依赖或内核TCB计数。
本批未提高既有帧表/能力/任务/消息/审计的固定容量上限。guard页的物理帧仍由image持有，省略的是镜像VA映射。

## 仍然开放的技术工作与待评审项

本批不关闭下列项目，不以提案、工作target或测试数量代替实现：

| 项目 | 保留的事实和后继要求 |
|---|---|
| ISSUE-FORK-CAPS-001 | 父子仍共享cap_space，fd close不隔离；需 per-Process capability/OFD holder的完整继承、失败回滚、退出回收设计及实现 |
| fork子VFS/execve/CLOEXEC | 管理器回复凭据仍绑定父Task，W7b仍父预读；需子会话/回复授权、打开描述继承和完整libc execve适配 |
| 信号/线程 | libc sigaction、mask、siginfo、EINTR、SA_RESTART、SIGCHLD自动通知及pthread取消/分离/属性仍未补齐；原生W9不升级这些符号 |
| ISSUE-TTYIN-001 | 输入仍在有读者时轮询COM1，WaitObject存在不等于IRQ替代完成 |
| ISSUE-CAP-001 | host模型完整位置/载体/溯源仍缺；parent/CDT及kernel机制证据不消除HeldBy建模边界 |
| S5-D6 | core/alloc/restricted std实验不构成POSIX std OS后端或D6组织级冻结 |
| S5-D1/D7/X3、BLK-003 | v2已写成提案，冻结/ABI归属/persona/符号版本/组织级复核保持待评审 |
| 历史QEMU超时 | 本批结果单独登记；未以退出时序或栈guard自动解释全部历史失败 |

ISSUE-AUTH-001及ISSUE-COW-001/002仍按原已接受细化边界解释；不把它们改写成未实现缺陷。
未执行Lean/Miri/SMP，不产生相应证明或并发安全主张。
