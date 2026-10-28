---
id: report-implementation-quality-review
kind: report
authority: scoped-evidence
status: reviewed-with-open-findings
lifecycle: historical
updated: 2026-10-27
---

# 实现质量独立审查（2026-10-27）

> 范围：commit `6f0dfea`，`git status` 干净；host 门禁实跑 + 源码/文档结构审查。
> 本报告是**独立技术审查**，不是阶段签署、profile 发布或组织级采纳；审查过程未修改任何源码。

## 结论与范围

工程纪律与架构边界控制处于同类研究原型的上游水平：架构约束被写成可复跑门禁，信任面
（TCB）实测 6.42%，证据分层与"拒绝过度声称"的做法在文档中可核对。同时，系统仍是有界
单核原型，且**门禁本身存在一个可复现的环境脆弱性**：musl 归档审计依赖英文 `objdump`
输出，在非英文 locale 的宿主机上会直接失败。

本次覆盖：

- host 侧全部门禁（test / lint / tcb / docs）并核对文档引用的计数；
- QEMU 验收的一个切片（R1，BIOS 双轮）；
- TCB 结构、内核模块结构、测试分布、文档治理与仓库卫生；
- 复现并定位一个构建门禁缺陷。

未覆盖：完整 36 组 BIOS 双轮、Lean `lake build`、`cargo xtask test-musl` 全量、SMP/实机、
性能与内存水位、`expect` 逐条语义审计。见下文「失败、反例与限制」。

## 基线与输入

| 项 | 值 |
|---|---|
| 基线 | `6f0dfea`（master），工作树干净，无未提交修改 |
| 工具链 | `rust-toolchain.toml` 钉定 nightly-2026-07-30（rustc 1.99.0-nightly）；targets `x86_64-unknown-none`、`x86_64-unknown-uefi` |
| host | Linux x86_64；GNU binutils 2.42（`/usr/bin/objdump`）；clang/llvm-ar 取自本机 lean toolchain；Python 3；QEMU 在 `~/.local/bin/qemu-system-x86_64` |
| guest | BIOS profile、qemu64、单 CPU、TCG（与仓库既有验收口径一致） |
| 构建产物 | 审查会重建 gitignored 的 `target/musl-prefix`；未改动任何被 Git 跟踪的文件 |
| 文档系统 | [scripts/docs.py](../../../scripts/docs.py)，当前 319 篇文档 |

## 审查方法

1. **只读统计**：按 crate 区分生产/测试代码行数（剥离 `#[cfg(test)] mod` 与独立
   `*_tests.rs`/`tests.rs`），统计 `unwrap/expect/panic` 站点、测试数量与命名、
   `TODO/FIXME`、`#[ignore]`。
2. **门禁实跑**：`cargo test --workspace`、`cargo xtask lint`、`cargo xtask tcb`、
   `python3 scripts/docs.py self-test` / `check`。
3. **启动实测**：`cargo xtask test-rust R1`，观察 musl sysroot、C 探针与 QEMU 双轮。
4. **证据核对**：[阶段跟踪](../../../Prima%20OS%20实现阶段.md)与最近报告中的数字与本机实测对照。
5. **缺陷复现**：对失败的 musl 审计做最小复现与 locale 对照实验。

## 命令与结果

| 命令/输入 | 本次实跑 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 是 | PASS：**1018 passed / 0 failed**（kernel 487、xtask 212、cosir 及其余 319）+ 7 doctest；`#[ignore]` 为 0 | host |
| `cargo xtask lint` | 是 | PASS：`fmt --all --check`、`clippy --workspace --all-targets -D warnings`、`kernel` 在 `deny(unsafe_code)` 下编到 `x86_64-unknown-none`、源码级零 `unsafe` 扫描（32 文件） | host + 裸机编译 |
| `cargo xtask tcb` | 是 | `trusted_lines=2072 mechanism_lines=30223 total=32295 ratio=6.42%`；`unsafe_tokens trusted=78 mechanism=0`；`include_bytes=0`、`prima_user_dep=false` | 源码度量 |
| `python3 scripts/docs.py self-test` | 是 | PASS：Markdown、metadata、search、determinism、lifecycle、Git recovery、change guards | 文档系统 |
| `python3 scripts/docs.py check` | 是 | PASS：319 documents；catalog、IDs、topics、本地链接与锚点 | 文档系统 |
| `cargo xtask test-rust R1`（默认 locale） | 是 | **FAIL**：`ERROR musl: unexpected raw syscall boundary in musl: {'': [58, 48]}` | host 构建门禁 |
| `LC_ALL=C cargo xtask test-rust R1` | 是 | PASS：`PASS musl static sysroot`、`PASS stage5 C probe is static PIE`、`PASS qemu bios R1 run=1/2` | host + QEMU |
| `objdump -d target/musl-prefix/lib/libc.a` | 是 | 中文 locale 输出 `文件格式`；`LC_ALL=C` 输出 `file format` | 复现输入 |

统计类观察（不改变任何门禁结论）：

- 代码量：111 个 `.rs`、76,433 行（含测试、`xtask`、`cosir`）；8 个 Lean 模块 673 行、38 条定理。
- 测试代码占比：cosir 78.8%、kernel 36.2%、prima-ostd 31.9%、prima-runtime 36.3%、
  prima-user 9.7%、prima-initgen 4.1%。
- 内核生产代码（26,673 行）中 `unwrap` 1 处、`expect` 92 处、`panic!` 3 处。
- 318 个测试函数名含 `reject/denied/invalid/stale/overflow` 等负例词（kernel/cosir/prima-runtime 抽样）。
- Git：382 次提交、2026-08-04 至 2026-10-27、平均 6.7 文件/提交、最大单次 122 文件；
  `TODO/FIXME/XXX/HACK` 标记为 0。

## 观察：可复现的强项

### 1. 架构边界可执行，而不是文档承诺

内核在 crate 级 `#![deny(unsafe_code)]`（[kernel/src/lib.rs](../../../kernel/src/lib.rs)），
`lint` 又做源码级扫描并禁止 `#[allow(unsafe_code)]`（[xtask/src/lint.rs](../../../xtask/src/lint.rs)），
两层叠加后"内核零 `unsafe`"由编译器与门禁共同保证，而不是靠评审记忆。TCB 实测 6.42%
（2072 / 32295 行），78 个 `unsafe` token 全部位于 `prima-ostd`；内核依赖只有
`bootloader_api`、`prima-abi`、`prima-ostd`。TCB 内部按上游 revision 溯源、逐条 SAFETY，
[prima-ostd/src/pod.rs](../../../prima-ostd/src/pod.rs) 还用 4 条 `compile_fail` doctest
把"带 padding 的类型不得跨字节边界"钉成可执行反例。

### 2. 证据分层与自我否定

[证据规则](../../architecture/evidence.md)明确区分规范审查、MODEL-TEST、Lean、QEMU、
COMPAT 与 lint 各自能支撑什么；[lean/README.md](../../../lean/README.md) 明确拒绝把现有
定理称为 `REFINEMENT-PROOF` 或机器证明；[阶段跟踪](../../../Prima%20OS%20实现阶段.md)
至今保持"阶段 5 进行中、未签署"，报告保留 69/70 失败与多次 QEMU 超时。文档记录的
`workspace 1020` 与本机实测 1018 同量级，说明这些数字来自实跑而非估算。

### 3. 形式模型与代码对拍

[cosir/tests/lean_backtrace.rs](../../../cosir/tests/lean_backtrace.rs) 把 Lean 参考语义与
`cosir` 可执行模型放在同一组输入上做 31 处差分断言。形式化模型只有 673 行、38 条定理，
但"模型—代码"之间有一条可执行链路，这比"项目内存在 Lean 目录"强得多。

### 4. 测试密度与负例覆盖

内核 36.2%、cosir 78.8%、prima-ostd 31.9%、prima-runtime 36.3% 为测试代码；句柄
generation 不复用、stale handle、槽位退役、权限未知位等高风险点都有命名对应的负例。
全仓 `#[ignore]` 为 0，没有"跳过即绿"的测试。

### 5. 文档系统是工程化的

313 篇 Markdown、176 篇报告，配 645 行零外部依赖的 `docs.py`：目录与章节行号生成、
链接与锚点校验、生命周期台账、删除路径的 Git 恢复登记、确定性自测。这类系统的常见
腐化方式是文档各自过期，而本仓库至少有机器可复跑的入口约束。

### 6. 仓库卫生

工具链钉死；musl 快照不入 Git，身份/commit/sha256 锁在
[compat 台账](../compat/musl-snapshot.lock.toml)；Rust std 补丁在 `target/` 内生成并校验
树哈希，不改安装源与 registry；提交粒度小（平均 6.7 文件），历史可读。

## 观察：问题与风险

### P0 · 构建门禁对 locale 敏感（已复现）

- **症状**：默认 locale 下 `cargo xtask test-rust R1` 在 musl 阶段失败：
  `ERROR musl: unexpected raw syscall boundary in musl: {'': [58, 48]}`。
- **根因**：`prima-user/musl/build.py` 用英文正则 `":     file format "` 解析
  `objdump -d` 的归档成员边界（[build.py:76](../../../prima-user/musl/build.py)），
  并把白名单固定为 `{"restore.lo": [58], "clone.lo": [48]}`（同文件第 85 行）。
  中文 locale 下成员头是 `aio.lo：     文件格式 elf64-x86-64`，成员名永远解析为
  空串，两个 `syscall` 都归到 `''` 下，必然不匹配。
- **对照实验**：同一条命令加 `LC_ALL=C` 后完整通过（musl sysroot、静态 PIE 探针、
  BIOS 双轮）。
- **影响**：任何非英文 locale 的宿主机都无法跑通镜像构建与 QEMU 验收；这类失败会被
  误读为代码回归。仓库未在[开发手册](../../architecture/development.md)登记 locale 要求。
- **修复方向**：对 `objdump`/版本探测统一强制 `LC_ALL=C`（或改解析 `objdump -f` 的
  稳定字段、改用 `llvm-objdump`），并补一条"非 C locale 下审计仍通过"的回归。

### P1 · 缓存未命中时 `--check` 会打印 PASS

`build.py` 的 `--check` 分支只在缓存命中时才审计归档，其余情况仍打印
`PASS musl locked snapshot; cache=miss`（[build.py:107](../../../prima-user/musl/build.py)）。
实测默认 locale 得到 `cache=miss` 的 PASS，`LC_ALL=C` 得到 `cache=hit` 的 PASS；前者
实际未验证任何归档内容。此外缓存键包含 `objdump --version` 的本地化文本，locale 切换
会无谓地使缓存失效。建议 miss 时输出 SKIP 并返回非零，或明确区分"已验证/未验证"。

### P1 · 没有 CI，门禁靠本地纪律

仓库无 `.github/` 或等价流水线；`test`、`lint`、`tcb`、`docs check` 全部依赖提交者按
[AGENTS.md](../../../AGENTS.md) 手动执行。382 次提交在本机长期未出事说明纪律有效，但
这是最脆弱的一环，尤其考虑到 QEMU 验收本身较重（36 组 × 2 轮、30 秒门限）。

### P1 · `bootstrap.rs` 是 1.2 万行上帝模块

[kernel/src/bootstrap.rs](../../../kernel/src/bootstrap.rs) 共 17,072 行（生产约 12,177 行，
占内核生产代码近半），含 30 余张表（Task/Capability/Process/WaitObject/Namespace/Timer/
Device/Audit…）、39 个 `impl`、325 个 `pub fn`。架构上把内核状态集中在一处是自洽的，
但单文件规模已超出常规评审与 diff 工具的舒适区，也放大了局部改动的回归半径。建议按
`cap/`、`task/`、`ns/`、`timer/`、`audit/` 拆分子模块（保持同一 `BootstrapState` 所有权）。

### P2 · 内核故障路径上的 `expect`

内核生产代码有 92 处 `expect`、3 处 `panic!`，在 `no_std` + `panic=abort` 下等于停机。
多数位于不可达或启动期一次性路径，可以接受；但如
[kernel/src/idt.rs](../../../kernel/src/idt.rs) 中异常/缺页处理路径上的容量 `expect`，
会把"审计容量耗尽"变成静默死机。建议对故障路径做一次 `expect` 清单审计，能返回错误码
的改为确定性失败。

### P2 · 文档漂移：最主要的分派文件仍写"单任务"

[kernel/src/syscall.rs:18](../../../kernel/src/syscall.rs) 的模块注释仍声明
"单核、非抢占、单任务"，而同一文件已承载 71 个 syscall，含 `SYS_TASK_CREATE`、
`SYS_FORK`、`SYS_THREAD_CREATE` 等。`docs.py` 的一致性检查只覆盖 `docs/`，不覆盖
代码内 doc comment，因此这类漂移不会被门禁发现。

### P2 · 验收成本高，历史超时根因未定位

完整验收要求单次 36 组 × 2 轮通过，30 秒门限；[阶段跟踪](../../../Prima%20OS%20实现阶段.md)
保留 W8/W9/V1 等超时记录且明确"根因仍未定位"。失败无法归因（回归还是抖动）会削弱证据
可信度。建议把时序等待改为 trace 握手，或对超时保留可复现的最小输入。

### P3 · 结构性观察

- 文档/代码比高（313 篇文档对 76k 行 Rust）。治理工具降低了腐化风险，但维护成本真实存在。
- `prima-user` 9.7%、`prima-initgen` 4.1% 的测试占比低于其他 crate；用户态主要由 QEMU
  探针覆盖，不等于缺口，但 host 侧对参数/边界组合的覆盖有限。
- 未发现 fuzz、属性测试、Miri 或 sanitizer 配置；对内核解码、路径解析、协议解析这类
  纯函数，属性测试的成本极低、收益明显。

## 失败、反例与限制

- **本次唯一失败**：musl 归档审计的 locale 依赖。已最小复现（默认 locale 失败、
  `LC_ALL=C` 通过），根因定位到 `build.py` 的输出解析；修复未提交，因此该失败在
  `6f0dfea` 上仍然成立。
- **限制**：本次为 host 侧审查，仅跑通 R1 一个 QEMU 切片，未执行完整 36 组双轮；未运行
  `lake build`，Lean 结论引用自仓库文档；未覆盖 SMP、实机、性能与内存水位；`expect`
  与 `panic!` 只做计数与抽样，未逐条判定可达性；测试占比按行数近似，不等于覆盖率。
- 本报告不对既有报告的 PASS/FAIL 做改写；任何修复都应另开 spec/plan 并保留本次失败。

## 阶段与 profile 影响

- 不改变阶段 0–4 的签署范围，不改变阶段 5 "进行中、未签署"的结论，不改变 profile v2
  提案状态，不改变 compat 台账行。
- P0 属于门禁工具链缺陷，不是阶段退出条件；修复应附带 host 回归（非 C locale 下审计
  通过、`--check` 在未验证时不得报 PASS），并在
  [开发手册](../../architecture/development.md)的环境与排错表中登记 locale 要求。
- 结构负债（`bootstrap.rs` 拆分）属于重构，不改变外部行为与阶段证据。

## 评价（审查者判断，非项目证据）

| 维度 | 判断 | 说明 |
|---|---|---|
| 架构纪律与 TCB 控制 | A | 零 `unsafe` 内核 + 6.42% TCB + 可执行门禁 |
| 证据与文档治理 | A- | 分层诚实、保留失败；扣分在代码注释漂移与文档维护成本 |
| 测试质量 | B+ | 1018 个 host 测试、负例充足；缺属性测试/fuzz，QEMU 侧靠长超时兜底 |
| 代码结构 | B- | `bootstrap.rs` 与 5.6k 行 `syscall.rs` 的结构性负债 |
| 工程可复现性 | B | 本机可跑通，但门禁依赖 locale，`--check` 会假 PASS，且无 CI |
| 系统成熟度 | C+ | 单核、容量有界、兼容面窄，阶段 5 未签署 |

总体：在"研究原型"这一档位属于上游；价值不在功能数量，而在把"不声称什么"写成边界并用
量化门禁守住。优先修复顺序：P0 locale 与 `--check` 语义 → CI → `bootstrap.rs` 拆分 →
注释漂移/故障路径 `expect`/超时归因 → 属性测试。

## 后继关系

- 本报告补充[证据规则](../../architecture/evidence.md)中"lint/build 只能证明构建与规则通过"
  的边界：它同时给出了**门禁自身不可复现**的反例。
- 不改写[阶段跟踪](../../../Prima%20OS%20实现阶段.md)的当前结论，也不取代任何历史报告；
  若修复 P0，应新增修复报告并链接本报告的失败记录。
- 主题入口：[系统与代码地图](../../architecture/system-map.md)（结构）、
  [开发与验证手册](../../architecture/development.md)（环境）。本报告未加入主题包。
