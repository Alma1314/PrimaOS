---
id: report-windows-m3-loader-runtime
kind: report
authority: scoped-host-and-qemu-evidence
status: implemented-with-limits
lifecycle: current
updated: 2026-11-27
---

# WIN-M3 部分：多模块导入、静态 TLS 布局与三 DLL ring3 原型

## 结论与范围

本切片把 [WIN-M3 DLL/运行时基础](../plans/2026-10-31-windows-libos-roadmap.md) 拆成三个
可独立验收的小阶段：

1. **多模块导入解析**（宿主模型）：导出项区分为映像内函数与转发器；新增按序号查找与
   `ModuleSet`，按显式模块集合解析按名/按序号导入，模块名忽略 ASCII 大小写与可选
   `.dll` 后缀，转发链限深 8 防循环。
2. **TEB/PEB 冻结子集与静态 TLS 模板**（宿主模型）：冻结 x64 TEB/PEB 的窄字段子集，
   从 PE TLS 目录提取模板并在调用方缓冲中复制加清零；越界、非 2 的幂对齐与短缓冲拒绝。
3. **三 DLL ring3 原型**（QEMU）：EXE 同时导入 `winpe-stub.dll!Stub`、
   `winpe-extra.dll!Extra` 与 `winpe-fwd.dll!Fwd`；`Fwd` 是转发器，解析链落到
   `winpe-stub.dll!Stub`。三个真实映像各自 R+X 映射，结果 42 只能由三条导入共同取得。

**WIN-M3 未通过**：没有动态 TLS、运行期 TEB/PEB（需要用户 GS 与 WIN-D3）、API-set 清单
展开、`DllMain`/引用计数、堆、`LastError` 语义、命令行/环境或 Windows 参考差分。上述
宿主模型只证明布局与解析算法，不证明任何 Windows 程序兼容。WIN-M2 的完整验收也仍未完成。

## 基线与输入

- 代码基线：`d57b3f0`（WIN-M2 补片提交）。
- 前序：[WIN-M2 真实 DLL 导出与发布失败清理](2026-11-27-windows-m2-real-dll.md)；
  [Windows LibOS 计划书](../plans/2026-10-31-windows-libos-roadmap.md)。
- 夹具（`libos/windows/build.rs` 用 clang 22.1.4 + lld-link 18.1.3 生成，留在 ignored
  `target/`）：

| 制品 | 字节 | 基址 | 导出/导入 | SHA-256 |
|---|---|---|---|---|
| `winpe-probe.exe` | 3072 | `0x1_4000_0000` | 3 个 DLL、各 1 个按名导入、1 个 DIR64 重定位 | `c4fe5b7512b908746e48883cffb02c8973cb85dc9437d1ec79dcd151d1a984e0` |
| `winpe-stub.dll` | 2048 | `0x1_8000_0000` | 导出 `Stub`=RVA `0x1000` | `4b5d04a0af454e6a1194248f74d2557b2e07f9c75a31e4b48f4e3d72e74d608c` |
| `winpe-extra.dll` | 2048 | `0x1_9000_0000` | 导出 `Extra`=RVA `0x1000` | `5b1fe05f8215e98299728d0e89dc415366d9ae5e2d3628ae672bca98b33f3f60` |
| `winpe-fwd.dll` | 1536 | `0x1_A000_0000` | 导出 `Fwd` → 转发 `winpe-stub.Stub` | `6cd136107305e6e30063430ead988a9f9b0f97773aba55af27212dd4899b99a6` |

- QEMU：仓库既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、1 vCPU、128 MiB、
  `isa-debug-exit`。

## 实现内容

| 切片 | 路径 | 内容 |
|---|---|---|
| M3-1 | `libos/windows/src/pe.rs` | `ExportTarget::{Function, Forwarder}`；`Exports` 不再拒绝转发器；`export_by_ordinal`（`Base` 起算、越界/零槽位返回 `None`）；转发器字符串按首个 `.` 拆成模块与符号 |
| M3-1 | `libos/windows/src/load.rs` | `Module` 与 `ModuleSet::resolve`：按名/按序号 → 解密目标 → 转发递归；模块名 ASCII 大小写不敏感、可选 `.dll` 后缀；`MAX_FORWARD_DEPTH = 8` |
| M3-2 | `libos/windows/src/teb.rs` | 冻结子集写入：TEB（Self `0x30`、StackBase/Limit、ProcessId/ThreadId、TLS 指针 `0x58`、PEB `0x60`、LastError `0x68`）与 PEB（ImageBase `0x10`、ProcessParameters `0x20`、ProcessHeap `0x30`）；`tls_template` 从 TLS 目录取模板（VA→RVA 边界检查、特性字段对齐、缺省 8）；`write_into` 复制加清零 |
| M3-3 | `libos/windows/src/winpe.rs` | 三个 `DllSource` 各按首选基址解析并映射；`ModuleSet` 驱动 IAT；校验 `imports_resolved == 3` 与 Fwd 确为转发器；沿用发布失败负例与统一清理 |
| M3-3 | `libos/windows/build.rs`、`probes/` | 新增 `extra.c`、`fwd.c`/`fwd.def`；三个 DLL 显式 `/base` 并生成 implib；EXE 链接三个库 |
| M3-3 | `xtask/src/protocol.rs`、`qemu.rs` | 见证改为 `unpublish=clean` + `dlls=3;forwarder=winpe-fwd.dll!Fwd`；PASS 行同步 |
| 附带 | `libos/windows/examples/inspect.rs` | 打印导出项名称、序号与目标，便于夹具核对 |

结果链路：`Stub(31)=32`、`Extra(5)=7`、`Fwd(2)=3`（转发到 `Stub`），加 EXE 本地重定位
偏置 0，合计 42。任一导入缺失或转发错误都会使装载失败或结果不同。

## 命令与结果

| 命令/输入 | 本次结果 | 证据范围 |
|---|---|---|
| `PRIMA_WINPE_PROBE=1 CARGO_NET_OFFLINE=true cargo run -q -p xtask --features windows-pe-probe -- test-winpe` | PASS：两轮，每轮负例 + 两个 42 子任务，exit=33 | QEMU ring3 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1234 passed、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy、bare-metal kernel、零 unsafe、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p prima-windows --features winpe-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo test -p prima-windows` | PASS：PE 43/43、TEB 4/4、ABI 4/4 | host |

两轮串口日志 `target/prima-os/WINPE-{1,2}.log` 逐字节相同，SHA-256
`3b17511f8836ccda2e505c61d63333bfe1e63ac9607bdce6be0d86aa87841022`；关键行：

```text
WINPE|unpublish=clean
WINPE|dlls=3;forwarder=winpe-fwd.dll!Fwd
WINPE|exit=self
WINPE|exit=self
WINPE|child1=42;child2=42;parent=alive
WINPE|PASS
```

宿主模型新增用例覆盖：导出序号查找、转发器目标解析、模块名大小写/后缀归一、跨模块
转发、按序号导入走转发链、缺模块/缺导出、自引用转发在深度上限停止、TEB/PEB 写入与
短缓冲拒绝、TLS 模板复制/清零/对齐/越界拒绝。

## 失败、反例与限制

- 实现中一次宿主失败：TLS 夹具 `0x2700..0x2710` 是 16 字节，测试误按 4 字节模板 +
  8 字节清零断言大小；修正断言（真实模板 16 字节），不是实现放宽。
- **静态 TLS 只能停在宿主模型**：kernel 无用户 GS/`swapgs` 支持，Windows x64 的
  `__declspec(thread)` 访问依赖 GS。ring3 建立 TEB/TLS 需要 WIN-D3 评审，本切片不伪造
  该能力，也没有动态 TLS。
- ring3 只演示按名导入；按序号导入仅在宿主模型验证（lld-link 的 `.def` 不支持
  `IMPORTS` 指令，未构造按序号导入的 EXE）。
- API-set 没有名称猜测或清单解析；调用方须把白名单展开为显式模块集合，
  `api-ms-win-*` 未路由时按缺模块失败。
- 转发器解析只跟随模块集合内模块；没有延迟导入、绑定导入、`DllMain`、引用计数或卸载。
- 三个 DLL 整映像按 R+X 映射，无按节权限；DLL 均按首选基址装载（无重定位），EXE 仅
  DIR64。TEB/PEB 模型未接入任何运行路径。
- 没有 Windows 参考差分；不能用本机模型或 QEMU 结果替代原样程序在参考环境的对照。
- WIN-M3 验收项“双 DLL、静态/动态 TLS、回调顺序、动态查找及卸载负例、线程错误不串扰”
  中仅“双 DLL”有运行证据，其余未覆盖。

## 阶段/profile 影响

不勾选阶段 7 条件，不改变 personality 组合、默认 boot 事件或 profile。Windows 候选从
WIN-M2 补片推进到 WIN-M3 的部分交付；WIN-M3 未通过，WIN-D3 未决。

## 后继关系

- 前序：[WIN-M2 真实 DLL 导出与发布失败清理](2026-11-27-windows-m2-real-dll.md)。
- 后继：评审 WIN-D3 后接入用户 GS 与运行期 TEB/PEB，补动态 TLS、TLS 回调顺序与
  `LastError` 语义；在参考 Windows 环境取得逐 API 差分；再评估 API-set 清单与
  `DllMain`/引用计数/卸载。
