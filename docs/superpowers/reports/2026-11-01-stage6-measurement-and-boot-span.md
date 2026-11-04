---
id: report-stage6-measurement-and-boot-span
kind: report
authority: scoped-implementation-progress
status: implemented-not-runtime-verified
lifecycle: historical
updated: 2026-11-01
---

# 阶段 6：计量、内核映射跨度与原生 Gate 封装（未完成整阶段）

## 结论与范围

基线 `4335e184ef5dd10679d72ecf6a0189b7672d9cd2`，本报告对应未提交修改。
**用户要求完成阶段 6 全部余项，本轮尚未完成该目标。** 本轮实现计量探针、内核映射跨度修正、
启动 ELF 去调试节打包与运行时 Gate 封装；编译检查通过，但没有运行 host 测试、QEMU 或 Linux 差分。
不能把“代码存在”记成 M1e、S6-D2 或阶段退出验收通过。退出条件仍为 0/4。

本轮按用户“全部完成后统一运行测试”的顺序要求暂缓执行测试套件，没有使用历史 PASS 代替本次结果。
下文保留实际已做工作及完整目标仍缺的内容，不能将本报告当成阶段签署包。

上位来源为[阶段 6–7 计划](../plans/2026-10-27-stage6-7-refinement.md)、
[递送设计](../specs/2026-10-29-stage6-m1-delivery-design.md)、
[现行协议](../specs/2026-10-29-stage6-m1-protocol.md)。
前序证据：[Gate 复制与生命周期](2026-11-01-stage6-gate-copy-lifecycle.md)、
[W7 分段诊断和 strip 失败](2026-11-01-stage6-w7-timing-diagnosis.md)。

## 已实现内容

### M1e 计量探针

`PRIMA_GATE_CASE=G4` 配合既有 `gate-probe` 特性，运行一个 ring3 服务与一个 ring3 客户端，
经生产 int80/Gate/wait/reply 路径完成 1000 次调用。客户端逐次核对不透明答复值，服务每次答复后让出。
普通镜像不编入计量模块及挂钩；既有 G1/G2/G2-death/G3 分支不选用计量。

每次记录五个 LFENCE/RDTSC 样本：客户端 Gate 适配器入口、入队提交后、服务 wait 输出写回后、
reply 提交后、客户端恢复路径即将进入 iret 跳板前。对应四段为 trap→queue、queue→deliver、
deliver→reply、reply→resume；总量是第一至第五点之差。queue→deliver 包含服务恢复和 wait 重试；
reply→resume 包含服务让出和调度。**不包含客户端硬件陷入前的指令或最后 iret/用户态继续执行**，
不可将其冒称完整用户态往返延迟。

复用已有 PIT/TSC 校准，不增加硬件信任面。1000 组原始有序时间戳在测量结束后导出，
串口输出不进入采样区间。宿主校验恰为 1000 组、索引连续、每组和跨组单调、唯一校准记录且无 panic，
再独立计算四段及总量的 nearest-rank p50/p99；不以分段 p99 之和冒充总量 p99。
内核结束检查三类 Gate 审计各 1000 条、无控制拒绝及 Gate/完成等待状态良构。

新增宿主反例检查缺记录、重复记录、索引错误、时钟回退、零频率和 panic 的拒绝。
这些测试只已编译，未执行。当前 harness 仍为 TCG 单线程，没有 KVM 实跑或 KVM 计量结论；
没有写入任何实测延迟或将“≤5 µs”升级为门槛。

### 内核 PT_LOAD 与启动文件长度分离

前序 strip 诊断已指出：自建根用 ELF **文件页数** 扫描内核 VA，会漏掉较短文件的 BSS。
本轮 `mem::kernel_virtual_pages` 从 ELF64 程序头计算 `max(p_vaddr+p_memsz)` 的页跨度，
检查头表边界、filesz≤memsz、文件范围及加法溢出，并限制当前内核虚拟跨度≤512 MiB。
`main` 用该跨度构造自建根；各页仍从 bootloader 根逐页翻译，保留源根空洞和内核栈 guard。

`FrameInventory::image_frames` 继续表示原 ELF 文件的物理帧归属，未将 BSS 或虚拟空洞
伪造为连续文件帧，未改变空闲帧分配器或文件 image Segment 的所有权口径。
新增一个短文件/大 BSS 及损坏程序头的宿主回归；只已编译，未执行。

`xtask` 使用已锁定且 bootloader 已依赖的 `llvm-tools 0.1.1` 定位 pinned `llvm-objcopy`，
将 `--strip-debug` 副本写入 ignored `target/prima-os/prima-kernel-boot.elf` 并用于 BIOS/UEFI 打包。
Cargo 原 ELF 保留调试符号供定位。未改变 30 秒验收门限、用户 ELF 或冻结 busybox。
此修正**尚未取得启动证据**，也不能推断全部历史 W7 超时具有同一根因。

### 原生运行时封装

`prima-runtime::gate` 封装既有 73–78，不追加或重编号 syscall。
`gate_wait` 检查 96 字节 v1 输出的版本、长度、非零 Task cookie、能力格式和 flags；
代理复制检查≤4096及客户端加法溢出，并核对实际返回长度。
reply/abort 保留不透明 u64 返回值，不将客户端的 Linux errno 解码为服务控制错误。
新增布局/非法输出/复制边界/不透明答复的可运行宿主检查；只已编译，未执行。
这些是中立机制封装，未加入 Linux 分派、OS 专属对象或 syscall 语义。

## 实际命令

| 命令 | 本次结果 | 范围 |
|---|---|---|
| `cargo fmt --all` | 退出 0 | 格式化 |
| `CARGO_NET_OFFLINE=true cargo check --workspace --all-targets` | 退出 0 | 普通配置编译；未执行测试 |
| `CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G4 cargo check --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe` | 退出 0 | 含新增宿主测试和 G4 的编译；未执行测试 |
| `CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G4 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings` | 退出 0 | 静态检查；未执行测试 |

未运行：`cargo test --workspace`、完整 `cargo xtask lint`、G1/G2/G2-death/G3/G4 双轮、
审计停止 14 轮、普通 BIOS 72 轮、冻结 Linux 白名单与 pinned guest 差分。
文档目录 build、check、check --base（基线为上述提交）退出 0；这些为静态文档校验，
不是整阶段验证。文档 self-test 暂未运行。

G4 待统一执行的命令：

```sh
CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G4 cargo run -p xtask --features gate-probe -- test-gate
```

不得并发构建不同 Gate 特性镜像并覆盖同一路径，再把混合输入的 QEMU 输出当作同版本验收。

## 整阶段剩余义务

具体当前状态只由[阶段跟踪](../../../Prima%20OS%20实现阶段.md)维护；本次核对发现以下实际依赖，
不是已完成声明：

1. 完成计量与 S6-D2 整体验收；本次代码尚不能关闭 M1e。
2. 可信停车与装载协议、大文件来源、冻结 sha256 busybox 的真实装载和初始栈启动。
3. 用户态 Linux LibOS、版本化 syscall profile、未支持调用的确定性可审计失败。
4. profile 所需 TLS、内存、futex 和进程环境，以及相应负例；不能把原生 musl 适配当成未修改 Linux ABI。
5. 白名单 P1/P2 源码及六 applet 的真实镜像、同一制品在 pinned Linux guest 的差分矩阵。
6. S6-M4 所需进程/线程/信号后继，随后统一验收、证据整理与退出评审。

协议 §4 当前在绑定期间禁止任意管理者改映射；§6 明确把开放共享 root、信号中断、整帧答复
列为重审触发。[D18/D20](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)把整帧答复、
门控 fork/exec 留给 S6-M4。不能删除这些保护，或让内核直接解释 Linux 调用号来凑退出条件。
下一增量须先明确相应中立机制与授权/事务后继，再接入真实 LibOS；本报告没有隐式通过该后继评审。

## 复查触发

任一新增路径实际运行、镜像打包策略变化、Gate v1 的 root/上下文边界变化，或冻结白名单取得
真实执行证据时，补充后继报告及阶段跟踪。保留前序 strip 异常退出、历史超时和 I1 差异；
没有新的实跑证据前，不能将这些历史失败改写为本轮修复已验证。
