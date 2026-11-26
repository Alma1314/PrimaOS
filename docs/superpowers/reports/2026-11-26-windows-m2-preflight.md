---
id: report-windows-m2-preflight
kind: report
authority: scoped-host-evidence
status: host-tests-pass-limits
lifecycle: current
updated: 2026-11-26
---

# WIN-M2 前置：PE32+ 平铺装载宿主模型

## 结论与范围

本切片在 `prima-windows` 增加纯内存装载器模型：先校验映射计划，再把 PE32+ 头部与节复制为
`SizeOfImage` 大小的平铺映像，应用 DIR64/HIGHLOW 基址重定位，并通过调用方解析器填充 IAT。
PE 模型测试 32/32、ABI 4/4 在 Linux x86_64 宿主通过，真实 PE 在首选基址装载成功。

**未覆盖**：没有页映射、段权限设置、Task 创建/发布、TEB/PEB/TLS 初始化、DLL 装载、导出表、
delay/bound import、真实导入闭包、ring3 执行、QEMU/Prima 运行或 Windows 参考差分。
这是 WIN-M2 的前置宿主模型，不是 M2 验收。

## 基线与输入

- 代码基线：`d22bc28`（上一提交）；本切片的 `src/load.rs`、测试与文档在提交前的工作树。
- 上位：计划书 WIN-M2；[WIN-M0 有界基线](../specs/2026-11-26-windows-m0-baseline.md)；
  [WIN-M1 报告](2026-11-26-windows-m1-pe-model.md)。
- 真实 PE：`/tmp/winpe/tiny.exe`（无导入/重定位，SHA-256
  `a382ee754aef94219a8853356467fe8afb2a9d076e2d6322da867f08db9298b2`）与
  `/tmp/winpe/reloc.exe`（导入 `stub.dll!Stub`、1 个 DIR64 重定位，SHA-256
  `6ce18dfa107052e0a74131fef4e0b4cf690f5bb5764a99c7f069bbace2de1163`；对应
  `/tmp/winpe/stub.dll` SHA-256 `deeadc97ab6a8b2df6716f628d5e94a5a9e4ae8b9356657850fd9c06979f1d97`）。
  两者均用 clang 22.1.4 + lld-link 18.1.3 生成，不随仓库提交。
- 合成夹具：`prima-windows/tests/pe_model.rs` 的 `build_pe()`，含两个导入 thunk、一个
  DIR64 重定位与 TLS 目录。

## 实现内容

新增 `prima-windows/src/load.rs`：`PeImage::load_into(base, output, resolve) -> LoadedImage`。

| 行为 | 规则 |
|---|---|
| 输出缓冲 | 必须至少 `SizeOfImage`；只使用前 `SizeOfImage` 字节，先清零 |
| 复制 | 头部复制 `SizeOfHeaders`；每节复制 `min(SizeOfRawData, VirtualSize)` 到其 RVA |
| 重定位 | 支持 `IMAGE_REL_BASED_DIR64` 与 `HIGHLOW`；`ABSOLUTE` 跳过；其他类型报错 |
| 基址变化 | 无任何可应用重定位时返回 `BaseWithoutRelocations` |
| 导入 | 遍历 ILT/IAT，解析器返回替换地址，写入 `FirstThunk + slot*8`；`FirstThunk == 0` 拒绝 |
| 失败 | 输出过小、目标越界、IAT 越界、未解析导入分别报错；失败不承诺回滚输出 |

`LoadedImage` 只返回基址、入口 VA、映像大小、解析导入数与已应用重定位数；节权限仍由调用方
按 `Section::protection()` 设置。

## 命令与结果

| 命令/输入 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p prima-windows` | PASS：PE 32/32、ABI 4/4 | host，合成夹具 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS | host，全 workspace |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS | host |
| `cargo run -q -p prima-windows --example inspect -- /tmp/winpe/tiny.exe load` | PASS：真实 PE 首选基址装载 | host，真实链接器制品 |
| `cargo run -q -p prima-windows --example inspect -- /tmp/winpe/reloc.exe load 0x150000000` | PASS：真实导入与 DIR64 重定位在新基址装载 | host，真实链接器制品 |

真实 PE 装载观察（`reloc.exe` 在新基址，节选原样输出；示例解析器把所有导入替换为
`0x1_7000_0000` 起的占位地址，不是真实模块解析）：

```text
section ".data" va=0x3000 vsize=0x8 raw=0x800+0x200 ReadWrite
dll=stub.dll
  thunk[0]=Name { hint: 0, name: [83, 116, 117, 98] }
imports=1 relocation_blocks=1 tls=false
loaded base=0x150000000 entry=0x150001000 imports=1 relocs=1
```

## 失败、反例与限制

- TDD 首次运行因缺少 `prima_windows::load` 与 `load_into` 编译失败；实现后 30 个测试通过。
- 新增 `FirstThunk == 0` 负例先失败：实现把解析结果写入 RVA 0，会覆盖 `MZ` 头；补上零值拒绝后通过。
  这是 TDD 捕获的实现缺陷，不是测试期望错误。
- 平铺映像是一次性缓冲区，不表达每页权限；调用方仍需分节映射并在发布前设置保护。
- 导入解析只依赖调用方回调，没有模块搜索、API-set、转发导出、delay import 或加载顺序；
  `inspect` 的占位解析器不证明任何真实 DLL 绑定。
- 仅实现 DIR64/HIGHLOW；不支持 IA64/ARM 重定位类型，也不证明重定位表全覆盖。
- 失败时输出可能已部分写入；调用方必须丢弃缓冲区，不能发布。
- 未初始化 TLS、未运行 `DllMain`、未建立 TEB/PEB；未执行入口。
- 未运行 QEMU/BIOS 或 `cargo xtask test`。

## 阶段/profile 影响

不勾选阶段 7 条件，不改变 personality 组合。Windows 候选扩展从 M1 宿主解析推进到 M2 前置；
WIN-D3（停车/发布、映射权限、用户 GS）仍未决，不能据此启动 ring3 装载。

## 后继关系

- 前序：[WIN-M1 PE/ABI 宿主模型](2026-11-26-windows-m1-pe-model.md)；
  [WIN-M0 有界基线](../specs/2026-11-26-windows-m0-baseline.md)。
- 后继：[WIN-M2 ring3 原型](2026-11-26-windows-m2-ring3.md) 已把本模型的平铺/重定位/IAT
  输出用于 held Task 的 R+X 代码段，并在 QEMU 运行原样 PE 子任务；手写导入桩、单节权限与
  父服务继续仍不等于完整 M2。
