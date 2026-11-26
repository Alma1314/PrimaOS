---
id: report-windows-m1-pe-model
kind: report
authority: scoped-host-evidence
status: host-tests-pass-limits
lifecycle: current
updated: 2026-11-26
---

# WIN-M1：PE32+ 解析与 Win64 ABI 宿主模型

## 结论与范围

本切片按 TDD 新增 `prima-windows` crate，落地 WIN-M1 的宿主可验证部分：
PE32+ 头部/节表/数据目录解析、严格 RVA 映射、导入描述符与 thunk、基址重定位块、
TLS 目录与回调、映射计划校验（对齐/范围/重叠/W^X），以及 Win64 调用约定探针。
21 个 PE 模型测试与 4 个 ABI 测试在 Linux x86_64 宿主通过。

**未覆盖**：没有装载器、导出表、delay/bound import、重定位应用、TLS 初始化、TEB/PEB、
Win32 API、ring3 执行、QEMU/Prima 运行或 Windows 参考差分。合成夹具只验证解析器对
已知布局的处理；真实 PE 夹具只验证头部/节映射，不含导入、重定位或 TLS。
M0 参考环境与程序白名单仍未冻结，见 [WIN-M0 有界基线](../specs/2026-11-26-windows-m0-baseline.md)。

## 基线与输入

- 代码基线：`a6a5e8a`；本报告对应的 crate、测试与文档仍在工作树，未随本报告提交。
- 上位计划：[Windows LibOS 计划书](../plans/2026-10-31-windows-libos-roadmap.md) WIN-M1。
- 工具链：仓库固定 `nightly-2026-07-30`；宿主为 Linux x86_64。
- 真实 PE 夹具：`/tmp/winpe/tiny.exe`（1024 字节，不随仓库提交），
  clang 22.1.4（`--target=x86_64-pc-windows-msvc -ffreestanding`）+ lld-link 18.1.3；
  SHA-256 `a382ee754aef94219a8853356467fe8afb2a9d076e2d6322da867f08db9298b2`。
- 合成夹具：`prima-windows/tests/pe_model.rs` 内的 `build_pe()`，所有目录/段地址显式写入。

## 实现内容

新增 crate `prima-windows`（`#![no_std]`、无依赖、库内 `forbid(unsafe_code)`）：

| 路径 | 内容 |
|---|---|
| `src/pe.rs` | PE32+ 有界解析、`PeImage` 只读视图、严格 RVA 窗口、节保护推导、导入/重定位/TLS 迭代器、映射校验 |
| `tests/pe_model.rs` | 21 个正反例；覆盖坏 DOS/PE 签名、PE32、非 AMD64、截断节表、可选头过小、头部小于节表、对齐、虚拟填充、文件越界、节重叠、W^X、导入名越界、重定位块尺寸、TLS 回调越界 |
| `tests/win64_abi.rs` | 8 参数寄存器/栈传递、浮点位置分配、汇编 shadow space/第 5 槽探针、非易失寄存器保存探针 |
| `examples/inspect.rs` | 手工检查真实 PE 的宿主入口；不参与自动验收 |

关键不变量：`rva_to_offset` 先做 `rva + len` 溢出检查，再限定在头部或某节的原始数据窗口内；
落在虚拟填充（`VirtualSize` 大于 `SizeOfRawData`）的读取返回错误而不是零；`validate_mapping`
先校验对齐与范围，再拒绝重叠和任何可写可执行节。解析器不分配、不修改输入。

## 命令与结果

| 命令/输入 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p prima-windows` | PASS：PE 模型 21/21、ABI 4/4；单测与 doctest 0 | host，合成夹具 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：无失败；23 组 `test result: ok` | host，全 workspace |
| `CARGO_NET_OFFLINE=true cargo clippy -p prima-windows --all-targets -- -D warnings` | PASS | host |
| `cargo fmt -p prima-windows -- --check` | PASS | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy、bare-metal kernel、零 unsafe、S6-X3 | host |
| `python3 scripts/docs.py self-test/build/check` | 本切片变更时 PASS（432 文档） | docs |
| `cargo run -q -p prima-windows --example inspect -- /tmp/winpe/tiny.exe` | PASS：解析真实 PE32+ 头与 `.text` | host，真实链接器制品 |

真实 PE 观察（原样输出）：

```text
machine=0x8664 image_base=0x140000000 entry=0x1000 image=0x2000 headers=0x200
section ".text" va=0x1000 vsize=0xf raw=0x200+0x200 ReadExecute
imports=0 relocation_blocks=0 tls=false
```

该夹具没有导入/重定位/TLS，不能替代后续白名单程序的差分；它只说明解析器能接受一个由
真实 PE 链接器生成的最小映像。

## 失败、反例与限制

- TDD 首次运行按预期因缺少 `src/pe.rs` 编译失败；首轮 20 个 PE 测试中 1 个失败：测试把
  hint/name 名称起点写成 `0x2404`，实际应为 `0x2402`，修正测试后通过。
- ABI 首次运行 1 个失败：`mixed4(1, 2.0, 3, 4.0)` 的期望值误写为 26，实际为 24，修正测试后通过。
- 两处失败都是测试期望错误，不是实现缺陷；实现未为迁就错误期望而改动。
- 目录迭代在尾部不足一个结构时静默结束（例如导入目录剩余不足 20 字节）。这是当前有界模型
  的取舍，不是 Windows 装载器语义；后续若要求严格拒绝非零尾巴，应加显式负例。
- 导入目录缺失返回空迭代器，不区分“没有导入”和“目录不可用”；M2 需要按程序闭包强化。
- `Protection` 对无读位但有执行位的节返回 `ReadExecute`，未表达“不可读但可执行”的独立含义；
  正式映射计划需按 Windows 保护常量重新核对。
- ABI 探针只证明宿主 rustc 与手写汇编的约定子集；没有 Windows 编译器产物、NT 陷入或异常上下文。
- 未运行 QEMU/BIOS 或 `cargo xtask test`；`xtask lint` 通过不等于 boot 回归通过，本切片也不改变 boot 路径。
- 最终复核时，并行的 `2026-11-26-p4-segmentation-probe.md` 尚未登记 reports 索引，导致全局
  `docs.py check` 报该文件缺失；该文件不属于本切片，未代改其索引。本切片两份文档已登记。

## 阶段/profile 影响

不勾选阶段 7 任何条件，不改变阶段状态或 personality 组合。Windows LibOS 仍是独立候选扩展；
本报告把它的状态从“未启动”推进到“M1 宿主模型存在”，但 M0 冻结门仍全部未满足。
[兼容主题](../../architecture/topics/compatibility.md)与计划书已加入本切片指针。

## 后继关系

- 前序：[Windows LibOS 计划书](../plans/2026-10-31-windows-libos-roadmap.md)；
  [WIN-M0 有界基线](../specs/2026-11-26-windows-m0-baseline.md)。
- 后继：固定 WIN-D1 参考 Windows/工具链/程序闭包；为 M2 评审停车/发布、映射权限与用户 GS；
  用真实带导入/重定位/TLS 的 PE 扩展正例，并为导出、delay import 与异常目录补解析和负例。
