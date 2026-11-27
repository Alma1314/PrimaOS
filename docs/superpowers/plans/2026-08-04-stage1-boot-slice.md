---
id: docs/superpowers/plans/2026-08-04-stage1-boot-slice.md
kind: plan
authority: historical
status: execution-record
lifecycle: historical
updated: 2026-11-27
---

# Prima OS 阶段 1 启动闭环 Implementation Plan

本页保留 2026-08-04 起的执行顺序、设计例外及原计划内的审查/失败记录；逐行代码模板、安装脚本、重复提交命令与旧阶段快照已压缩。上位来源为[启动设计](../specs/2026-08-04-stage1-boot-slice-design.md)，当前命令见[开发手册](../../architecture/development.md)，当前进度只查[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

原 1844 行全文可由 `git show 5ebf2791fcf01f21cf6e396227483e74ef1b2dd9:docs/superpowers/plans/2026-08-04-stage1-boot-slice.md` 恢复；压缩清单与哈希见[整理记录](../reports/2026-11-27-documentation-consolidation.md)。下文“预期”不是本次实跑，原有实际观察单独标明。

## Global Constraints

- 当时环境：Windows 11、Rust nightly-2026-07-30/2024 edition、`bootloader`/`bootloader_api` 0.11.17、QEMU 11.0.50（winget `SoftwareFreedomConservancy.QEMU`）。固定 Cargo.lock、rust-src/llvm-tools-preview/rustfmt/clippy 和 none/uefi targets。
- `kernel` 是 bootloader 入口的 freestanding 二进制；主机 `xtask` 经 Cargo artifact dependency 取得 ELF，生成非空 BIOS/UEFI 镜像。只验收 BIOS，UEFI 生成不代表运行通过。
- 当时唯一功能入口 `cargo xtask test` 独立启动 BIOS 两次；固定 `pc-i440fx-10.0`、qemu64、128M、单 vCPU/单线程 TCG、无显示/monitor/NIC、`-no-reboot`、raw BIOS drive、`isa-debug-exit` 端口 0xf4。
- 成功序列恰为 v=1、seq=1…3 的 boot_start/serial_ready/boot_complete；非法、缺失、重复、乱序或 panic 均失败。PRIMA 行必须 LF/CRLF 终止，拒绝仅 CR、多余 CR 或未终止行；无关固件尾部噪声仍忽略。
- 最终正常/panic 事件先等待 UART LSR bit 6 TEMT，再退出；guest 0x10/0x11 对应主机 33/35，错误/超时不输出成功结论。
- 范围不含物理内存、页表、GDT/IDT、trap、Bootstrap、Task、Capability、形式化、CI 或 Linux 主机验证。当时只支持 S1-D1/S1-D2/S1-X1，不关闭 BLK-001/002，也不签署阶段 1。

## File Structure

当时创建 `.cargo/config.toml`、工具链/workspace/锁文件、kernel 入口与 serial/protocol/qemu 模块，以及 xtask 的 image/protocol/qemu/CLI；镜像在 ignored `target/prima-os/`。当前特权代码归属已演进，按[系统地图](../../architecture/system-map.md)定位，不把旧源码模板重新复制到 kernel。

### Task 1: 固定主机环境并建立镜像构建闭环

先确认缺少 `cargo xtask build` 的失败基线，再安装固定依赖并建立镜像管线；每次删除旧镜像后重建，检查 kernel ELF 与两个镜像非空。最初仅 hlt 的内核不产生完成事件，不能算功能验收。产物接口为 `Images.bios`、`build_images` 与稳定镜像路径。

### Task 2: 用测试锁定串口事件协议

先写解析负例再接生产校验器：固件噪声正例，缺失/乱序/重复、字段数、版本、status、非数字 seq 与 panic 负例。当时预期 9 项；严格终止符在 Task 6 补强。生产调用接通后移除临时函数签名引用，不靠无调用的声明消除告警。

### Task 3: 实现固定 QEMU 命令与安全超时

将 `Images.bios` 接 `run_bios_interactive`/`run_bios_test`；从 PATH 或当时 Windows 固定安装目录定位 QEMU。参数、退出码和超时必须由测试锁定，stdout/stderr 同时捕获。

保留的兼容例外与清理规则：

- QEMU 11.0.50 Windows 在启动前对 `readonly=on` 报 `Block node is read-only`，故 raw drive 不设只读，并由每次重建镜像避免跨命令残留；当时 kernel 不做块 I/O。
- `-no-shutdown` 会抑制 debug-exit 的 guest shutdown，使进程不退出；保留 `-no-reboot`，不传 `-no-shutdown`。
- child 保持唯一所有权。kill 成功后 wait/reap；kill 失败先 try_wait，确认已退出才继续；仍运行或查询失败时返回受控错误，绝不阻塞 wait。
- 正常/超时/等待错误路径完整汇聚两个 reader 的成功文本、read error 或 panic；只有确认 child 退出才 join。未确认清理时明确报告 reader 未 join/QEMU 可能仍运行，保留另一流诊断。
- 模块私有 ChildControl seam 只测试清理；当时预期参数/退出码和 failed-kill/read-error/panic 共 7 项。具名 fallible reader startup 由 Task 6 补齐。

### Task 4: 用 BIOS 集成测试驱动 kernel 串口与退出实现

接通 build/run/test CLI 后，先用不退出的内核观察受控超时并回收本命令持有的 child；不杀其它 QEMU。再接 COM1、单调序号、panic 与退出端口，验证两次 BIOS 均满足协议和退出码。原阶段单测预期 16，最终加固后为 23；这些是历史计数，不是现行门槛。

### Task 5: 完整验证并登记阶段证据

当时验收顺序：fmt、xtask/裸机内核 Clippy、xtask 单测、`cargo xtask test`。仅实际通过后登记 IMPL-001/TEST-001；S1-D1/D2/X1 可关联，S1-D3/D4/D5/X2/X3 与阶段 0 阻塞不据此关闭。原计划的六条 PASS 尾部是预期格式，不单独构成运行证据。

### Task 6: 最终整分支审查加固

保留原计划内已勾选的历史观察：

| 检查 | 原记录 |
|---|---|
| `cargo test -p xtask protocol::tests::rejects_prima_record_` | 三项 RED：旧解析器错误返回 `Ok(())`；随后收紧 LF/CRLF，并补固件未终止噪声正例 |
| `cargo test -p xtask reader_startup` | RED：缺少 `start_readers_with` 的 unresolved import；随后实现具名 `thread::Builder::spawn` 可失败启动、清理/reap 与安全 join |
| 聚焦 GREEN | protocol 13 passed、reader startup 3 passed、完整 xtask 23 passed；原计划已勾选该项 |
| fresh 验收与 TEST-002 | 原计划标记完成并要求保留 TEST-001；本页不补造当时没有记录的输出或退出状态 |

任一 reader 启动失败都先清理/reap child；第二个失败仅在确认退出后 join 第一个并保留输出。cleanup 未确认时不 join。正常与 panic 均 flush TEMT；固定参数、事件、退出值和上述两项 QEMU 例外不变。

### Task 7: 固定高负载主机的 BIOS 测试失败保护

以下是原计划唯一承载的时限诊断记录，继续保留：

- finishing fresh 验收在主机总 CPU 达 100% 时，BIOS run 1 触发原 10 秒保护且没有 PRIMA 输出；cleanup 正确终止并 reap child。
- 同一镜像和完全相同 QEMU 参数在负载回落后以 33 退出，耗时 7.079 秒并输出三条 PRIMA；两次 fresh BIOS acceptance 总耗时 17.156 秒。
- 原记录将 10 秒阈值判断为环境敏感的保护误报，没有确认 kernel、镜像或 QEMU 状态机挂死；这不解释后来的所有超时。
- 参数锁定测试先 RED（left 10s/right 30s），将 `TEST_TIMEOUT` 改为 30 秒后聚焦 GREEN；不增加自动重试，成功仍要求 33 与严格事件序列。
- 原记录：fmt、两种 Clippy、23 项单测与双轮 BIOS 验收通过，完整 acceptance exit 0、17.156 秒，追加 TEST-003，保留 TEST-001/002 及阶段边界。

## Plan Self-Review

依赖为构建→协议→runner→串口/退出→证据→审查加固→时限保护；Tasks 6/7 含实际失败与观察，不能作为冗余模板删除。其余任务中的安装/提交脚本、整份 Rust 源码和旧 tracker 替换块已移除，契约与当前代码另有来源。阶段结论及后继整改继续由阶段跟踪和报告链裁决；本次压缩没有重跑 OS 验收。
