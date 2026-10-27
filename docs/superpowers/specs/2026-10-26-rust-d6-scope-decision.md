---
id: decision-rust-d6-scope
kind: specification
authority: user-selected-stage-scope
status: user-selected-posix-backend-extension
lifecycle: current
updated: 2026-10-27
---

# S5-D6：Rust 目标与 std 范围裁决

上位：[阶段定义](../../../Prima%20OS.md)、[D6/M20路线图](../plans/2026-10-22-stage5-completion-roadmap.md)、
[现行工作目标](2026-10-25-stage5-gap-closure-design.md#rust-工作目标)、[M21出口评审](../reports/2026-10-23-stage5-m21-exit-review.md)。
用户于2026-10-26明确选择A：原生OS接口的单线程std子集。本选择确定本轮实现及验收范围，不签署阶段出口；本轮R1/R2双轮与两种Clippy已通过，完整结果见[统一报告](../reports/2026-10-25-stage0-5-gap-followup.md)。

## 触发问题

D6允许冻结最小target/core/alloc/std子集；M20明确不以全量std为目标。
前批R2仅覆盖内存API，stdio沿用上游unsupported后端，会把输出当作成功而丢弃；Instant和sleep会panic。
这不足以成为一个已明确范围的原生OS后端。用户已选择下列A范围。

## 选项 A：冻结有原生 OS 接口的单线程 std 子集（推荐）

固定nightly-2026-07-30及rust-src输入哈希，target保持vendor=prima/os=none，不冒充Unix。
通过隔离副本中显式Prima PAL和原生Runtime链接hooks实现下列范围：

| 模块 | 纳入本次范围 | 保留边界和确定失败 |
|---|---|---|
| core/alloc | 算术、Box/Vec/格式化、显式GlobalAlloc | 探针96页单调arena；dealloc不单独回收，不能当生产allocator；System分配返回NULL |
| std内存 | BTreeMap、slice Read、Duration、Instant运算 | HashMap随机种子不作为安全熵；随机生成不支持 |
| std::io | stdout/stderr真实TTY发送、write_all/flush；失败不伪造成功 | 两流合并同一TTY；flush只保证字节已入消息队列，不保证终端已呈现；非空stdin读返回Unsupported，空读返回0 |
| std::time | 原生物理单调100Hz Instant，checked运算 | 无wall-clock；SystemTime::now明确保留上游panic；无纳秒精度或SMP保证 |
| std::thread | 原生yield、绝对期限阻塞sleep，中断后续等、期限向上取整 | 仅单线程target；Builder::spawn返回Unsupported；无动态TLS/线程析构承诺；超出u64期限panic |
| std::fs | metadata返回Unsupported的负例 | 不支持文件OS PAL，不把C/VFS符号覆盖等同Rust文件API覆盖 |
| std::net/process/env | 不纳入支持范围 | 不承诺完整失败/初始化语义；无Command/args/environ/标准lang_start，保留原生入口 |

链接hooks参考实现为`prima-user/rust-probe/src/pal.rs`；只传值，不跨FFI传借用缓冲。
必须在任何std OS调用前由原生入口完成时钟和TTY发送权限初始化；不能把无初始化的其它程序称为可用std移植。
构建脚本记录PAL文件哈希；修改PAL也强制重建隔离副本，不复用旧stamp。
输出按字节排队，队列满时yield重试；不承诺多字节原子性、并发stdio或读取消。

R2新增真实stdout/stderr分拆标记、Instant/sleep/yield及stdin/spawn负例；逐字节验收标记增加`pal=1`。
R1标记`pal=0`，仍不链接std。运行证据必须在所有实现项完成之后统一取得。

选择A表示D6按上述有限子集继续实施和验收，未覆盖模块属于明确范围边界。
仍需技术证据与组织级出口评审，不能仅凭选择勾选S5-D6或阶段5。

## 选项 B：要求 POSIX std OS 后端

将fs、net、process、env、默认allocator、标准入口与线程/TLS纳入本次实现。
需新增真实Prima std平台模块与C/persona链接约定；不能靠将target标成Linux复用Unix假设。
现有A子集只作为中间工作，不关闭对应实现缺口。需相应扩展机器/同源/错误路径验收；没有完整实现前不进入最终统一测试。

## 选择记录与重审

2026-10-26，用户在范围问答中明确选择“A：原生 OS 接口的单线程 std 子集（推荐）”。据此继续A的统一验收，不实现B。选择不替代profile/ABI/persona/动态链接、性能目标或组织级签署。
更换nightly、增加OS模块、线程/动态TLS、默认allocator、标准入口或SMP时重新审查PAL安全及支持矩阵。

## 统一验收中的线程失败路径修订（2026-10-27）

上游线程公共构建层在调用unsupported PAL之前使用System分配内部句柄；
本轮System明确返回NULL，因此不能等到后端才拒绝spawn。
Prima覆盖在共同spawn入口分配前返回Unsupported，保留System失败语义；
上游输入锁增加thread/lifecycle.rs的精确hash（共十项），仅修改隔离的源码副本。
R2继续实际调用Builder::spawn验证失败类型，不通过跳过调用规避此路径。

干净R2原生stdout/stderr、Instant/sleep/yield及stdin/spawn/System负例已两轮PASS；覆盖层重新编译后的core/alloc与std Clippy均PASS。完整单次BIOS68轮已通过；阶段签署仍单独跟踪。


## 本任务范围扩展（2026-10-27）

用户在后继任务明确要求“完成完整 POSIX std 后端”，因此本任务按选项 B 继续，
原 A 的选择和验证记录保留。新增独立 Prima Unix/musl 目标，具体实现契约见
[POSIX std 后端](2026-10-27-posix-std-backend-design.md)。B 的编译/机器结果单独取证，
不把原 R1/R2 结果升级为 B 已完成，也不自动签署 D6 或扩大底层 POSIX profile。
