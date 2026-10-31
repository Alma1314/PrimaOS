---
id: report-rust-handle-source-refinement
kind: report
authority: scoped-evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-31
---

# Rust 句柄源码到 Lean 模型的细化证明

## 结论

建立首个直接绑定生产 Rust 源码的细化切片：`Handle::new` 与 `Handle::next_generation` 经 Charon/Aeneas 自动提取，5 条一般 Lean 定理证明构造器拒绝条件、机器整数后继到既有 release generation 的对应，以及从提取返回值出发的后续模型无复活性质。每次检查都重新提取、拒绝陈旧/手改生成物并重新证明。

**范围为两个方法及 release 的 generation 更新核心，尚非整张 Rust 槽表或完整内核的细化证明。** rustc/Charon/Aeneas 与其标准库模型属于可信翻译边界；LLVM/机器码/生产编译器版本等价均未证明。

## 基线与前序

基线提交 `59f4d8aa0c3087607b3ffc3638f49b6996abc42b`，叠加上一轮尚未提交的[有限槽表组合证明](2026-10-31-kernel-handle-table-proof.md)。本次保留那些改动，新增源码提取链，不修改生产 `handle.rs`、槽表行为或阶段 6 机制。

依据：[本切片契约](../specs/2026-10-31-rust-handle-refinement-design.md)、[原单槽设计](../specs/2026-10-24-kernel-handle-model-design.md)、[单槽证据](2026-10-24-kernel-handle-model-report.md)、[权威映照](../../../lean/README.md)、[COSIR §18](../../../COSIR%20原语规范.md#18-从抽象规范到实现的细化要求)。

## 输入与自动提取

`lean/refinement/source.rs` 的 `#[path = "../../kernel/src/handle.rs"]` 让 rustc 直接编译生产文件。包装入口仅调用方法；Charon 根据这两个入口提取可达代码，Aeneas 生成 2 个 Lean 文件、2 个原方法及 2 个包装函数，无手写 external template。Rust core `checked_add` 由 Aeneas 标准模型提供。

| 项 | 锁定身份 |
|---|---|
| Aeneas 官方 release | `nightly-2026.10.30-de16f3c` |
| Linux x86_64 发布包 SHA256 | `0963dc77cab890de16b213b903091721961afca6303b9b0ea05ff8d76013f624` |
| Charon | `0.1.301`，commit `373d84044d80f6eb44285c2c349bae1453f5b6ca` |
| 抽取 Rust | `nightly-2026-09-17` / rustc `1.100.0-nightly (923c95cdf 2026-09-16)`，独立安装 rustc-dev/rust-src |
| Lean | `leanprover/lean4:v4.35.0`；原模型保持原固定版本 |
| mathlib | v4.35.0 / `56a005855ea17e0b0958f73ec801ae140b0a1679`，传递依赖见独立 `lean/refinement/lake-manifest.json` |
| 源码/生成物身份 | `lean/refinement/source-manifest.json`；整个生产文件（含 cfg(test) 文本）hash，两个生成文件逐字节比对 |

发布包与三个工具二进制的具体 hash 在 `toolchain.lock.json`；工具及依赖只放 gitignored target/.lake，不提交归档、缓存或二进制。[官方流程](https://github.com/AeneasVerif/aeneas/blob/de16f3c/documentation/getting-started.md)说明其 Rust→LLBC→Lean 步骤；[发布记录](https://github.com/AeneasVerif/aeneas/releases/tag/nightly-2026.10.30-de16f3c)给出下载输入。

## 已证明的关系

| 定理 | 范围 |
|---|---|
| `checked_successor_refines` | 对任意 U32，checked_add(1) 经 `Some→Nat / None→0` 投影与模型上界策略相等 |
| `source_successor_refines` | 自动生成的真实方法定义恰调用 checked_add；不是把测试样例作为等价依据 |
| `source_release_generation_refines` | 任意成功模型 release 的目标 generation 与提取函数返回值投影相等 |
| `source_successor_no_revival` | 从提取方法返回的 Option 建立释放状态后，任意后续**模型**序列均不复活旧非零 generation |
| `source_constructor_refines` | 对任意 index/generation，生成构造器准确拒绝零分量、保留合法值 |

所有证明在 `lean/refinement/Refinement/Handle.lean`，生成代码在其 `Generated/`，完整公理清单由 `Checks.lean` 打印并由脚本校验：全部 5 条仅依赖 `propext`、`Classical.choice`、`Quot.sound`，没有自定义公理、sorryAx 或 native 求值公理。

## 命令与观察

| 命令/输入 | 本次结果与范围 |
|---|---|
| `python3 scripts/check_handle_refinement.py --regenerate` 后默认模式 | PASS：当前生产源码→LLBC→Lean、manifest/逐字节新鲜度、1802 jobs 的独立 Lake 依赖图构建、5 条定理公理检查 |
| 临时给正式生成的 Funs.lean 追加注释，再运行默认脚本（finally 恢复） | 预期失败：`generated Lean differs: Funs.lean`，exit 1；证明前拒绝手改产物 |
| `python3 scripts/check_handle_refinement.py --mutation-test` | PASS：临时 Rust `self.generation.checked_add(1)` 改为 `Some(1)`，重新提取后同一组定理因 `source_successor_refines` 的 rfl 无法成立而被拒绝；正式源码不变 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1045 项测试 + 7 doctest，共 1052；0 failed |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy、裸机构建、36 文件内核零 unsafe |
| 原独立模型固定版本 `lake build` | 前轮干净 11 jobs 通过；本次新增依赖不进入该包，源码证明通过本地依赖引用旧模型 |
| docs self-test/build/check/`check --base 59f4d8a` | PASS：340 篇文档，自检、生成、目录/链接和基线变更检查 |

原始日志位于 `/tmp/prima-refinement-*.log`、`/tmp/prima-source-refinement-*.log`；语义 mutation 的详细证明错误保存在 gitignored `target/handle-refinement-mutation.log`。报告保留关键失败及边界，不提交生成的 LLBC 或构建产物。

## 失败与修正

1. 沙箱内获取 GitHub API 因 DNS 失败，按授权在沙箱外下载并核验官方 SHA256；不是逻辑反例。
2. Charon 第一次运行时独立工具链尚未安装完成，因此因缺少 manifest/无法自动安装退出；安装完成后同一提取路径通过。未绕过 borrow checking。
3. Lake 更新默认尝试整个 mathlib 缓存；取消后只获取 Aeneas 实际导入的模块及其传递依赖（1313 个缓存文件）。保留独立 manifest，不升级版本。
4. 首次生成入口参数命名 `handle` 遮蔽 `handle` 模块名，生成文件不能编译；改包装函数参数名为 `value` 后**重新提取**，未手改生成物或生产源码。
5. 初轮证明脚本需展开 U32.numBits 与 Result bind 的定义/化简规则；修正后普适命题通过，未减弱边界、加公理或使用 sorry。
6. 错误 Rust 生成 Some(1) 的 mutation 通过 Rust/Charon/Aeneas，失败发生在 Lean 手写细化定理，而非仅 hash 检查或输入编译阶段。这证明检查链能区分所测真实语义缺陷；不能据此认为所有翻译器缺陷均被检测。

## 复现与工程边界

准备锁定工具时，从 `toolchain.lock.json` 的官方 URL 下载发布包，校验其 SHA256 后解包到 `target/aeneas/`；使用 rustup 独立安装锁定 Rust 版本及 rustc-dev/rust-src。在 `target/aeneas/backends/lean/` 使用固定 Lean 版本运行 `MATHLIB_NO_CACHE_ON_UPDATE=1 lake update`，然后用 `elan run leanprover/lean4:v4.35.0 lake exe cache get` 获取固定版本缓存（可显式指定所需 Mathlib 模块缩小下载），或用普通 Lake 构建准备依赖。依赖只位于当前工作树的 target 下。

在 `lean/refinement/` 运行 `MATHLIB_NO_CACHE_ON_UPDATE=1 elan run leanprover/lean4:v4.35.0 lake update`，固定其传递依赖，再从仓库根执行上面的检查脚本。可以将该包 `.lake/packages` 链接到后端已准备的同版本 packages 以复用缓存；该链接是本地优化，不属于证明假设。默认检查脚本不下载、不修改源码/生成物，版本或缓存未准备好时失败。

`--regenerate` 只更新提取工件/输入清单，后面仍强制证明；若语义改变导致证明失败，不能将 regenerate 当作豁免。原 `lean/` 的普通 `lake build` **不包含**独立源码细化包，不能只跑旧命令就宣称本链通过。

尚未覆盖整个 Rust 槽表的状态写入/最小槽选择/保留区间策略、完整句柄编码、权限/位置/审计、任意后续真实 Rust 操作、unsafe/硬件、并发和机器码。生产 Rust 与抽取 Rust 工具链不同；本切片不证明其编译结果等价。形式结果以可信翻译器及标准库语义正确为前提。

本报告不改阶段退出或签署；下一项是扩大真实槽表源码抽取与状态投影，而不是把已有整表手写模型结果当作已落实到实现。
