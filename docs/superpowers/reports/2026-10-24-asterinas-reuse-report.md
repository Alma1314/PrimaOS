---
id: report-asterinas-reuse
kind: report
authority: scoped-evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-24
---

# Asterinas 复用与 OSTD 临界区验证报告

## 结论与范围

本轮将 Asterinas 对齐工具、原子互斥与 IRQ RAII 接入 PrimaOS 的真实帧计算和全局状态路径。
同时修正不返回调度跳转的锁生命周期，保留汇编数据布局，处理调试构建新增临界区后暴露的快路径栈溢出。
这不是整体移植 Asterinas 或 Linux personality；候选组件及适配条件以[契约](../specs/2026-10-24-asterinas-reuse-design.md)为准。
没有性能提升、完整框架 soundness、SMP、全栈边界证明或阶段签署主张。

## 基线与输入

- 直接修改 master checkout，PrimaOS 基线 `d1e472a5a6c5ca77b9e8ab94620594e63960c927`，开始时干净。
- 本地 Asterinas HEAD `ceb65e3f850096d30283137845ac01ad51dfcdaf`，开始与复核时均干净；没有抓取上游最新版本或修改参考项目。
  参考项目的代理/开发说明没有被作为用户指令执行。
- Rust pinned `nightly-2026-07-30`；构建 `CARGO_BUILD_JOBS=2`、musl make 默认 2 jobs。
- BIOS 使用现有 pc-i440fx-8.2 / TCG thread=single / qemu64 / 1 vCPU / 128M / 无 NIC 参数、30 秒门限；
  Linux UDP 参考仍在宿主执行。QEMU 单个 vCPU 仍可能占满一个 CPU 核，jobs=2 不是 CPU 利用率限制器。
- 前序 AE/AG/AI 设计与报告链由[本轮契约](../specs/2026-10-24-asterinas-reuse-design.md)显式列出；
  其中“StaticCell 仅依赖顺序调用”的判断在此次闭包入口上得到后继修订，其他历史限制保留。
- [来源登记](../../architecture/asterinas-provenance.md)固定上游版本、输入哈希、实际移植路径和设计参考，
  移植源码保留 MPL-2.0 SPDX 与原贡献者归属，许可证全文逐字复制；没有 Cargo 新依赖或本地目录依赖。

## 实现与影响

### 实际复用

AlignExt 保留上游五种无符号类型、align_up/down 和两项测试，增加 checked_align_up 与边界测试。
kernel/mem 中物理帧发现、空闲区构造、镜像区间计算改用该模块；合法常量为 4096，溢出保持原 Option 处理，
删除旧私有对齐函数。对所有 u8 值和 alignment 的 65,536 组输入，用 u16 扩展运算作为独立结果检查。

锁采用上游 Acquire/Release、先屏蔽 IRQ 后加锁、先释放锁后恢复原 IF 的顺序。
StaticCell 的共享和可变闭包均独占同一锁，host 的静态 arena 也因此获得实际互斥，
不再仅以“实例区段不同”推断整个 backing array 可同时建立可变引用。
host IF 模型检查嵌套与 unwind，bare-metal 实际执行 x86 指令；guard 限当前 CPU，不能 Send/Sync。
短锁/IRQ 原语与闭包入口内联，避免不必要的调试调用栈帧，但这单独没有解决下面的容量失败。

### 布局、调度与栈

StaticCell 显式 repr(C)，数据首字段、锁尾字段；实际 FAULT_FRAME/USER_FRAME/SYSCALL_STACK 地址测试
及高对齐类型测试保护裸入口 sym 偏移。as_ptr 保持返回数据地址，不持锁、不改变其裸访问责任。
调度器删除 SCRATCH 闭包中的不返回 resume，直接使用已有局部帧：跳板在 iretq 换栈前读完帧。
因此不会永久遗留锁，也不再需要该全局暂存。

快路径共享栈从 16 KiB 明确调整到 32 KiB，与既有每任务内核栈一致；底部 64 字节为固定哨兵。
执行后只 volatile 读取底部，不给活动栈建立整个缓冲的引用。
syscall_fast_ready 增加必需字段 `stack_canary=1`，kernel 纯判定和 xtask 均校验，缺失/0/2 等负例被拒绝。
原事件序号、业务输出、资源容量和双轮确定性检查保留，没有延长超时门限。
哨兵具有非零初始化，镜像文件长度/帧计数/地址也会随布局变化，不能仅按“多预留 16 KiB”推断 ELF 大小变化。

## 实跑命令与结果

本机原始日志放在 gitignored `target/asterinas-validation/`；不提交 boot images 或调试器文件。

| 命令/输入 | 结果 | 范围 |
|---|---|---|
| `CARGO_BUILD_JOBS=2 cargo test --workspace` | PASS，912 项，0 失败；含 kernel 435、prima-ostd 33、xtask 205、2 个新增 rustdoc 示例 | host 单元/集成与示例 |
| `CARGO_BUILD_JOBS=2 cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机构建、29 个内核源文件零 unsafe | 编译/静态门禁 |
| `CARGO_BUILD_JOBS=2 cargo xtask tcb` | PASS：trusted 1636、mechanism 24347、total 25983、ratio 6.30%；unsafe 64/0 | 源码代理度量，包含源文件内测试，不等于二进制 TCB 证明 |
| `d1e472a` 干净 checkout 的既有 `target/debug/xtask tcb` | trusted 1300、mechanism 24342、total 25642、ratio 5.07%；unsafe 62/0 | 同一源码扫描方法的本次基线实测 |
| 首次 `cargo xtask test`，16 KiB、未内联版本 | FAIL，默认 BIOS run 1 返回 0（期望 33），尚无 BIOS PASS | 实际启动失败，见下节 |
| 同一失败镜像 + QEMU `-d int,cpu_reset` 与 GDB stub 写监视点 | 捕获 GDT 被栈帧覆盖、随后 #GP → #DF → triple fault | 定位本次故障，不是前序无输出超时的根因 |
| 仅内联短原语、仍 16 KiB 的完整测试尝试 | FAIL，默认 BIOS run 1 仍返回 0 | 内联单独不足；没有把此尝试当作修复 |
| 32 KiB + 哨兵版本首次完整 `cargo xtask test` | FAIL：19 轮 BIOS 通过后，V1 run 2 在 bootloader 物理映射阶段 30 秒超时，未进入内核 | 不是栈溢出退出；保留原始日志 |
| 冻结 V1 镜像、复用原 qemu.rs / protocol.rs 与 30 秒门限 | PASS，同镜像双轮 17.603s / 16.825s，保留 IPC 双轮一致性和新增哨兵检查 | 隔离复测，不证明启动超时根因已修复 |
| `CARGO_BUILD_JOBS=2 cargo xtask test` 第二次完整命令 | FAIL：默认/N1 共 26 轮通过后，W1 run 1 在 30 秒时完全无输出超时 | 两次完整命令都未单次全绿，保留失败 |
| 临时驱动原样续跑 W1–A1，W1 使用冻结的失败镜像 | PASS，剩余 36 轮，退出 0；保留 Linux 参考、相同 guest 工件、输入注入、IPC 双轮一致性与原校验 | 与前 26 轮合计恰好覆盖全部 62 个用例/轮次；是分段覆盖，不是一次完整命令 PASS |
| docs self-test / build / check --base d1e472a | PASS，298 个源文档；最终报告更新后也重新生成/校验 | 导航、生命周期、链接、基线变更 |

新增测试覆盖：并行 8 线程各 2000 次加法、panic 后可再次访问、两种初始 IF 的嵌套恢复、
真实入口静态地址/对齐、全部 64 个哨兵字节的单字节破坏与 trace 缺失/损坏拒绝。
没有为了在旧实现上得到一个 FAIL 而故意执行并发可变引用的未定义行为。

## 本次失败与修复链

1. 初次 lint 拒绝脚本迁移带入的测试模块重复 import；已删除。哨兵接线初次编译因校验块落入同名 CS 判断的错误函数而失败，
   已按唯一 lstar 锚移回 validate_syscall_fast_data。测试中只经裸地址观察赋值触发 unused-assignments，补充了显式字段读取。
   均保留日志，最终未压制告警、跳过测试或放宽断言。
2. 首次 BIOS 运行完成 C 探针输出，但以 0 退出，QEMU 异常日志确认三重故障。
   冻结镜像 `first-bios.img` 的 SHA-256 为 `13b9eb193941ea21e97152c0a85e5c89d571c84240915f8c3d42b251c941be86`。
   故障 IP `0x100000b7eeb` 对应 load_fs_selector 的 mov fs；#GP error 0x28。
3. 对该冻结镜像设置 GDT 槽 5 写监视点：原 16 KiB syscall 栈底 `0x1000028bc50`，
   实测 RSP 已到 `0x1000028bb30`（越过栈底 0x120），AtomicBool::compare_exchange 的栈写覆盖相邻 GDT。
   调用链包括 BootstrapState::segment / segment_at → Arena::get → StaticCell::with → lock；
   捕获 IP 包括 `0x100000ba6ef` / `0x100000ba731`。该地址解释只适用于冻结镜像，不用于后续重编译 ELF。
4. 内联尝试仍失败；最终明确扩大栈预算并增加每轮必须通过的边界检测。此检测不能阻止越界或证明所有路径的栈上界。
5. 32 KiB/哨兵版本完整运行在 V1 run 2 超时；日志停在 bootloader 的 Map physical memory，尚无 PRIMA 内核事件。
   冻结 V1 SHA-256 `d7a3ee81303f5c140b9223a7bc03659b63e5b46358e325f3f1f0d8d90a4c8e62`。
   原始校验函数对同镜像隔离双轮通过；第二次完整命令继续通过前 26 轮后在 W1 超时，后续分段补齐。
   该启动超时根因仍未确定。
   qemu-stack-v1-timeout.log / v1-isolation.log / qemu-final.log 分别保留失败、隔离与完整复跑，未延长门限。
6. 第二次完整命令在 W1 首轮 30 秒完全无输出超时；冻结镜像 SHA-256
   `d80b945bedeb1842c07a4a5a81961aa9665ebd3171a0431499f887c517a6d077`。
   为避免重复已通过的 26 轮，用未提交的临时驱动保留原 test_boot_slice 的 W1–A1 后半段，
   W1 直接使用冻结镜像，其余用原 image.rs 构建；模块仍引用正式 qemu.rs/protocol.rs，
   guest 路径从正式 xtask 可执行文件的 CARGO_BIN_FILE 字符串读取。原 30 秒门限、所有断言、
   I1 串口注入及每程序的双轮 IPC 比较均保留。后 36 轮通过，合计覆盖全部 62 个唯一用例/轮次。
   完整命令没有单次全绿，该无输出超时根因仍未定位，不伪报为修复或完整重跑 PASS。
   临时驱动首轮误选旧 Windows host rlib、路径读取合并字符串时失败，修正为 ELF host 库和定长哈希路径后构建通过；
   正式测试代码未因此修改。日志 qemu-retry-w1-timeout.log / qemu-continuation.log 保留各自范围。
7. 文档首次检查拒绝 crate 目录中的来源文件作为 topic source；按治理规则移至 architecture 后通过。
   此文件在调整前未提交，不是删除已发布历史文档；无需伪造生命周期恢复记录。

## 限制、阶段与重审

- 32 KiB/哨兵只在实际验收范围提供证据，没有 MMU guard page、异常栈或所有路径峰值证明。
  上游 KernelStack 的 guard-page 分配器未迁入。
- host IF 模型与真实 CLI 执行是不同证据；没有专门的真实 IF=1 恢复/嵌套中断负载验收。
  裸指针/汇编的独占与映射责任仍在调用者；同单元递归、不返回闭包、跨锁环、NMI 重入仍受契约限制。
- Once 未改，Lean 未运行，不增加兼容 profile 支持或完成 Rust refinement。锁会增加静态占用与执行开销，
  本轮没有测吞吐/延迟，不能宣称性能提升或 TCB 缩小。
- [前序 W9 超时](2026-10-24-native-runtime-service-report.md)和
  [VFS 历史超时](2026-10-23-vfs-libos-foundation-report.md)的未定位结论保留；本次栈溢出不能用来解释它们。
- 不改退出勾选、BLK-003、阶段5未签署/阶段6未启动或用户资源容量。
  [阶段跟踪](../../../Prima%20OS%20实现阶段.md)只追加实际范围/证据。修改 IRQ、裸入口布局、
  不返回控制流、深调用链、原始访问或上游版本时重审契约与反例。
