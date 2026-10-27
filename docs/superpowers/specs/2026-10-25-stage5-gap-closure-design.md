---
id: spec-stage5-gap-closure
kind: specification
authority: scoped-implementation-contract
status: working-contract
lifecycle: current
updated: 2026-10-25
---

# 阶段 5 缺口补强：启动、栈保护、退出及 Rust 最小目标

## 范围和前序

基线 `e305712`；用户要求逐项补缺口、最后统一全量测试，组织级待评审项保留。
承接 [M12 启动/退出](2026-10-22-stage5-m12-c-startup-exit-design.md)、
[fork 后继](2026-10-24-fork-continuation-design.md)、
[OSTD 复用](2026-10-24-asterinas-reuse-design.md) 与
[M20/D6 路线图](../plans/2026-10-22-stage5-completion-roadmap.md)。
这是工作实现边界，非 D3/D6、profile 或阶段组织级签署。

## 启动栈

原先 `C_STACK_OFFSET = stack_bytes - 256` 只容纳小参数。
新偏移按指针表、16 字节对齐字符串区、argv/envp 每串 NUL 的实际大小计算，
并保留 256 字节最小块，使固定探针布局不变。栈底至少留一页运行空间；
加减法、数量、单串长度、单页参数载体上限仍检查，拒绝嵌入 NUL，不截断。
返回实际偏移用于创建 Task，不能只改变写入位置而仍使用旧 RSP。

`A2` 为独立 C/musl 探针，使用版本化载体、16 argv、8 envp、255 字节参数、
空串、空环境值、空格与 UTF-8，并检查 auxv。Linux 使用同源程序与相同参数。
它只建立此固定边界集的证据，不等同任意用户程序或随机化测试。

## 系统调用栈保护

OSTD 分配页对齐的 `4096 + 32768` 字节缓冲。低页在自建内核根的镜像映射中省略；
所有随后克隆的 Task 根继承相同 supervisor 子树。裸入口的栈顶包含保护页偏移，
可用容量仍为 32 KiB；底部 64 字节哨兵移到可用区域。
生产代码不能借用或读取整个含未映射保护页的 Rust 对象。

`syscall_fast_ready` 新增强制 `stack_guard=1`，读取当前运行根实际页表确认保护页不存在；
保留原 `stack_canary=1`。host 检查布局与缺失/损坏证据拒绝；QEMU 验证实际页表。
不是完整栈峰值证明、不保护任意越界跳写，也不删除 kernel-only 物理直映射别名。
本片只覆盖 syscall 专用栈，per-Task 中断栈仍不是 MMU guard stack。

## 托管程序退出交接

`task_destroy` 会清理发送者尚未接收的消息。C 适配层报告退出后停在 RECV，
由管理器接收状态、销毁目标 Task，再销毁栈/数据 Segment。
销毁失败不得继续回收内存或写成功清理见证。fork 子仍走原生 Process exit。
不新增 syscall、权限或消息 tag；TASKCAP 的 Dead/root cleared 与清理计数仍需验证。
host 反例必须展示过早销毁确实删除队列中的状态；所有原验收轮次统一重跑。
此修复不自动解释 bootloader 阶段和全部历史超时。

## fork/clone FPU 继承

登记子任务后、用户态可调度前，对运行父任务执行 FXSAVE，再复制完整镜像到子槽。
不能仅复制父任务上一次让出时的旧槽。父子后续槽独立；fork/clone登记会用新快照覆盖复用槽，不据此承诺所有原生线程退出/复用路径的FPU生命周期。
W6 三种 fork 路径检查非默认 MXCSR/x87 舍入控制字继承、父先改写及等待后的父状态恢复。
不覆盖 AVX/XSAVE、SMP 或全部浮点数据寄存器的机器证明。

## Rust 工作目标

`targets/x86_64-prima.json` 名称 `x86_64-prima`，OS 为 none、vendor 为 prima，
单线程、softfloat、静态 PIE、无解释器；不冒充 Linux 或 wasm。
固定 nightly-2026-07-30，独立 Cargo 项目使用本仓库 prima-runtime/abi/ostd；
`R1` 构建 core/alloc，`R2` 从同一固定 rust-src 构建受限 std。

该 nightly 未为 none 提供完整 std 配置。构建脚本仅修改 ignored 的 rust-src 副本，
显式为 prima 选择generic错误、单线程TLS以及原生stdio/time/sleep/yield模块；其它OS模块保留unsupported。
记录编译器版本、九个上游输入SHA-256、三个PAL源hash及overlay hash；源许可证MIT/Apache-2.0。
安装的工具链不修改，副本与产物不入Git。构建无网络，依赖缺失明确失败。

2026-10-26工作增量及D6待选范围见[裁决工件](2026-10-26-rust-d6-scope-decision.md)。
原生入口先初始化时钟及TTY权限，链接值参数hooks；stdout/stderr通过Runtime发送真实字节，
Instant读100Hz物理单调时钟，sleep使用72绝对期限、向上取整且中断后续等，yield使用8。
R2在原内存API及metadata/System负例基础上，新增stdout/stderr逐字节、期限和stdin/spawn负例，尚未执行。
应用仍须自带GlobalAlloc：探针私有96页arena单调分配、进程销毁整体释放；
System返回NULL，非空stdin和线程创建返回Unsupported，SystemTime::now仍panic。
文件/网络/进程/环境/随机/标准lang_start和完整POSIX std未实现，不把有界PAL称为完整std移植。
这些工作不自动冻结D6；以用户对裁决工件的选择继续实施，之后仍需统一验收与出口评审。

可重复构建和检查：

```sh
python3 scripts/build_rust_probe.py
python3 scripts/build_rust_probe.py --std
python3 scripts/build_rust_probe.py --clippy
python3 scripts/build_rust_probe.py --std --clippy
cargo xtask test-rust
```

Clippy模式用独立target目录、rustc命名shim、SYSROOT及禁用编译器信息缓存，避免误选安装工具链的未修订rust-src。
S5-D6 仅取得工作目标/受限子集进展，完整范围冻结和组织级确认保持未完成。
重审触发为更换 nightly、增加 OS 后端、线程/动态 TLS、默认 allocator 或 PAL 安全假设变化。

## 统一验收

修改完成后运行 workspace tests、musl host、lint、全量 BIOS、compat、tcb 和文档四项检查。
R1/R2、A2 与 W6 纳入全量双轮，保留原 30 秒门限和逐字节/IPC/容量断言；不自动重试。
失败原样登记，重跑属于修复后新版本验证，不合并不同版本冒充全量单次通过。
实际证据见[报告](../reports/2026-10-25-stage5-gap-closure-report.md)。
