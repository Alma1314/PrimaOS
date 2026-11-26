---
id: report-windows-m2-real-dll
kind: report
authority: scoped-qemu-evidence
status: ring3-prototype-pass-limits
lifecycle: current
updated: 2026-11-27
---

# WIN-M2 补片：真实 DLL 导出与发布失败清理

## 结论与范围

本切片补齐 [WIN-M2 ring3 原型](2026-11-26-windows-m2-ring3.md) 的两个缺口：

1. **真实 DLL 导出**：EXE 的 `winpe-stub.dll!Stub` 导入不再绑定页面内手写机器码桩，而是
   解析 lld-link 生成的 `winpe-stub.dll` 的 PE 导出表，取到 `Stub` 的 RVA，按 DLL 首选
   基址 `0x1_8000_0000` 计算实际地址并写入 IAT；DLL 映像作为独立 Segment 以 R+X 映射到
   子任务地址空间。子任务返回 42 只能经真实 DLL 代码取得。
2. **发布失败无残留可执行映射**：新增负例——子地址空间已由 `task_create_held` 建立 R+X
   代码映射后，以未对齐目标强制映射失败；装载器销毁 held Task（内核拆除其地址空间）、
   解除暂存映射、销毁 Segment，并验证暂存 VA 重复解除映射返回错误（内核 `NotMapped`）。
   随后两个正常子任务在**同一批 VA** 成功装载并运行。

专用 QEMU 命令两轮各装载两次，串口见证含 `WINPE|unpublish=clean`、
`WINPE|dll=winpe-stub.dll!Stub`、两个 `exit=self`、`child1=42;child2=42;parent=alive`
与 `PASS`，两轮日志逐字节相同。host 1223、lint 通过。

**未达到完整 WIN-M2 验收**：仍无 Windows 参考差分；导入只覆盖一个 DLL 的一个按名导出，
没有真实导入闭包、API-set、转发导出、延迟导入、`DllMain` 或引用计数；整映像按 R+X 映射，
没有按节设置 RW；没有 TEB/PEB/TLS/SEH、控制台输出或 Win32 错误映射。WIN-D3（停车/发布、
分节权限、用户 GS）仍未决。

## 基线与输入

- 代码基线：`600af6e`（文档提交）；本切片的 `libos/windows/src/{pe,winpe}.rs`、
  `libos/windows/tests/pe_model.rs`、`xtask/src/{protocol,qemu}.rs` 在提交前的工作树。
- 前序：[WIN-M2 前置平铺装载模型](2026-11-26-windows-m2-preflight.md)、
  [WIN-M2 ring3 原型](2026-11-26-windows-m2-ring3.md)、
  [Windows LibOS 计划书](../plans/2026-10-31-windows-libos-roadmap.md)。
- 构建开关：`prima-user` feature `windows-pe-probe` 与构建期 `PRIMA_WINPE_PROBE`；
  PE 夹具由 `libos/windows/build.rs` 用 clang 22.1.4 + lld-link 18.1.3 生成，留在 ignored
  `target/`。本轮 OUT_DIR 产物：`winpe-probe.exe` 3072 B、SHA-256
  `71e86eb7ea9fc338bc1db42a7c6059c4a30333153755402b0f7c6daa2a0663d4`；
  `winpe-stub.dll` 2048 B、SHA-256
  `bc920720bc172be11aadceca6dc1f3d0e2e3365248912eca5df84ce85542a7f9`。
- EXE：image base `0x1_4000_0000`、image `0x5000`、1 个导入
  `winpe-stub.dll!Stub`、1 个 DIR64 重定位、无 TLS。DLL：image base `0x1_8000_0000`、
  image `0x3000`、1 个导出 `Stub`（RVA `0x1000`）、无导入与重定位。
- QEMU：仓库既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、1 vCPU、128 MiB、
  `isa-debug-exit`。

## 实现内容

| 路径 | 内容 |
|---|---|
| `libos/windows/src/pe.rs` | 新增 `PeImage::exports()` / `export_by_name()`、`Export`/`Exports`：解析导出目录（Base、函数表、名称表、序号表），构造时校验三张表落在文件数据内；按名导出给出序号与函数 RVA；转发器（RVA 落在导出目录内）返回 `PeError::ForwardedExport`；新增 `ExportOutOfBounds` 错误 |
| `libos/windows/tests/pe_model.rs` | 夹具加入 2 个导出；新增 4 个用例：按名迭代与序号、转发器拒绝、过短导出目录拒绝、表越界拒绝 |
| `libos/windows/src/winpe.rs` | 真实 DLL：解析 `winpe-stub.dll`、`export_by_name(b"Stub")`、IAT 解析器只接受该 DLL 的该导出；DLL 与 EXE 分别暂存、分别以 R+X 映射进子任务；`run_child` 改为闭包 + 统一清理（成功/失败都解除暂存映射、销毁/丢弃 Segment、销毁残留 held Task）；新增 `run_publish_failure` 负例；EXE 装载后校验 `imports_resolved==1`，DLL 装载后校验基址为首选基址 |
| `xtask/src/protocol.rs` | `validate_winpe` 要求 `unpublish=clean` 与 `dll=winpe-stub.dll!Stub` 两条见证；负例测试覆盖顺序/内容缺失 |
| `xtask/src/qemu.rs` | PASS 行补充 unpublish/DLL 信息 |

负例流程：创建 RWX Segment → 暂存映射 → EXE 平铺装载与 IAT 填充 → `task_create_held`
（内核在子地址空间建立 R+X 代码映射）→ 以 `CHILD_STACK_VA+1` 触发未对齐映射失败 →
清理 → 重复解除暂存映射必须失败 → 同一批 VA 随后被两个正常子任务复用。

## 命令与结果

| 命令/输入 | 本次结果 | 证据范围 |
|---|---|---|
| `PRIMA_WINPE_PROBE=1 CARGO_NET_OFFLINE=true cargo run -q -p xtask --features windows-pe-probe -- test-winpe` | PASS：QEMU 2/2 轮，每轮负例 + 2 个 PE 子任务，exit=33；连续 4 次运行（8 轮）通过 | QEMU ring3 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1223 passed、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy、bare-metal kernel、零 unsafe、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p prima-windows --features winpe-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo test -p prima-windows` | PASS：PE 36/36（含 4 个导出用例）、ABI 4/4 | host |

两轮串口日志 `target/prima-os/WINPE-{1,2}.log` 逐字节相同，SHA-256
`3378fd884c28e17a77a2fae4b63171c420d640c4c79765a37b6641f68ea676a9`；关键行：

```text
WINPE|stage=neg-refused
WINPE|stage=neg-cleaned
WINPE|stage=neg-unmapped
WINPE|unpublish=clean
WINPE|dll=winpe-stub.dll!Stub
WINPE|exit=self
WINPE|exit=self
WINPE|child1=42;child2=42;parent=alive
WINPE|PASS
```

## 失败、反例与限制

- **首次含负例运行失败**：`task_create_held` 在负例中返回错误，串口只有 `stage=neg-loaded`
  后接 `WINPE|FAIL`，QEMU exit 35。加入 `stage=neg-create=NN` 诊断（映射 `Error` 到两位
  编号）后连续 4 次两轮未复现；根因未定位，登记为未解释的限制（可能与 boot 期间任务表
  容量/本机 TCG 时序相关），不因后续 PASS 删除。诊断行以 `stage=` 开头，不被见证校验计入。
- 负例只覆盖“`task_create_held` 成功后、子 RX 映射请求本身失败”的路径；`task_create_held`
  内部失败（预检原子拒绝）没有单独断言，也没有部分映射回滚用例。
- 暂存清理的失败被忽略（`let _ =`）：负例用重复 `unmap` 失败来证明暂存 VA 已释放，但
  单个 `unmap`/`destroy` 返回错误时不会单独报告。
- DLL 按首选基址装载，无重定位；DLL 无导入，因此 `load_into` 的解析器未被调用。EXE 仅
  DIR64 重定位。
- 整映像按 R+X 映射（含 `.data` 页）；没有按节权限、TLS、TEB/PEB、SEH、`DllMain`、
  API-set、转发/延迟导入或引用计数。
- 输出仍是共享结果页与冻结串口行，没有 `WriteConsoleW`/stdout；错误值没有 Win32 映射。
- 没有 Windows 参考差分；本轮结果不能替代原样 Windows 程序在参考环境的对照。

## 阶段/profile 影响

不勾选阶段 7 条件，不改变 personality 组合、默认 boot 事件或 profile。Windows 候选从
ring3 原型推进到“真实 DLL 导出 + 发布失败清理”的有界切片；WIN-M2 仍未通过，WIN-D3 及
后续 M3 门未变。

## 后继关系

- 前序：[WIN-M2 ring3 原型](2026-11-26-windows-m2-ring3.md)；
  [WIN-M2 前置平铺装载模型](2026-11-26-windows-m2-preflight.md)。
- 后继：接入按节权限与 TEB/PEB/TLS，冻结 WIN-D3 后评审多 DLL 导入闭包与 `DllMain`；
  准备 Windows 参考环境取得逐 API 差分；补负例的部分失败/回滚与任务创建内部失败断言。
