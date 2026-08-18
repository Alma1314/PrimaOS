# BLK-001 形式化模型工件：Lean 4 可执行模型 + cosir 对拍

> 文档类型：阶段 0 形式化模型设计
>
> 关联规范：[[COSIR 原语规范]]（§13.1 抽象状态机验证、§17 预期安全结论与证明义务）；[[Prima OS]]（§13.1/§13.2 验证方法）；[[Prima OS 实现阶段]]（BLK-001、S0-X3）
>
> 状态：已获批准，待实现
>
> 日期：2026-08-18

## 1. 目标与范围

解除阶段 0 阻塞项 **BLK-001**（"尚未选择并建立形式化模型工件"，解除条件：明确工具、入口文件和可执行状态迁移），并支持阶段 0 退出条件 **S0-X3**（"形式化工具能够表达并执行状态与迁移"）。

**选定工具：Lean 4**。本设计建立于 `Prima OS.md` §13.1（"计划由形式化模型检查或 Lean 4 证明覆盖"）与 `COSIR 原语规范` §17（"本文不附带 Lean 4 证明；形式化工作应至少覆盖以下义务"）之上，把已具备的 Rust host 模型（`cosir` crate）升级为一套 Lean 4 数学化 + 可执行工件。

### 范围（中等深度）

覆盖**已细化原语**上落在 §17 上的**关键不变量**，并在 Lean 中获得可执行状态与迁移：

| 原语 | Lean 建模 | 关键不变量 |
|---|---|---|
| bootstrap | 状态 + 两段投影 α | WellFormed（init 仅初始 Authority）、幂等唯一性、投影保真 |
| exc_resume | 失败各路径 + 成功恢复 | 失败原子性（失败时状态完全不变，§17 基础保持性 4） |
| tlb_refine | per_task TLB + 非一致刷新 + 显式失效 | 写前显式失效后读一致 |
| memseg_refine | 物理帧分配双视图 | W6（唯一归属）/ W7（vspace 双视图一致） |
| cow_refine | 组合序列表达 | 组合不变量（H1 承载体） |

### 明确目标外（不属 BLK-001）

- 不一并完成 §17 全部 9 类证明义务（异步通知投递、消息/Send-Recv、Namespace、Timer 等需更多非已细化原语建模，属更长的独立轨道）；
- 不证明真实 kernel（真实实现满足 α 属 S1 之后对照，见 §20.4）；不改动真实 CR3/PTE/硬件结论；
- 不做自动差分测试基建（选项 2 的差分引擎，留后续切片；本切片用选项 1 对拍）；
- 不在 Lean 中复刻"内容复制字节"语义（host 抽象层本就不建模内存字节，对齐 ISSUE 与 `cow_refine` 边界）。

## 2. 架构：Lean↔Rust 双层 + 映照契约

```
lean/                       # 独立 lake 项目（不动 Rust workspace）
  Formal.lean               # 入口文件：模型目标清单 + #eval 冒烟
  State.lean                # Lean State + 映照契约注释
  Bootstrap.lean            # bootstrap 原语 + 证明（对拍样板）
  ExcResume.lean            # exc_resume 原语 + 失败原子性证明
  Tlb.lean / Memseg.lean / Cow.lean   # 后续原语（首批可只至 exc_resume）
Rust 侧（cosir）——结构性不动
  tests/lean_backtrace.rs    # 复用现有夹具，输出与 Lean 可比对的 trace
```

**机制（选项 1，对拍）：**
- Lean 每个迁移函数可 `#eval`，作为**参考语义**；
- Rust `lean_backtrace.rs` 复用现有 cosir 夹具（`build_state`、exc_resume 各原子失败用例、tlb/memseg 冒烟），把同样具体输入跑一遍并输出归一化 trace；
- **对拍** = Lean `#eval` 与 Rust 对同一状态迁移产生一致的"成功/失败/结果状态等价"，由 Lean `#eval`/`example` 断言 + Rust 测试**双向夹逼**。

**对应证明通道：** Lean 内证明的数学性质，通过 `State.lean` 中的**映照契约**（Lean 结构与 cosir `State` 投影一致）下落回 Rust 资产。映照契约为唯一权威；任何两套语义漂移记录为 ISSUE（对齐 ISSUE-MEM-006 教训）。

**证据边界（贯穿所有原语一致）：**
- Lean 证明覆盖**host 模型语义面**，证据等级 `MODEL-TEST`（+独立 Lean 证明通道 `REFINEMENT-DESIGN`）；
- **不宣称** `KERNEL-TRACE`、真实硬件、`REFINEMENT-PROOF`、机器证明。避免未来外推。

## 3. Lean 状态表示取舍

Rust 模型用 `HashMap<CapId, Capability>`、`HashSet<(TaskId, ExcSourceId)>`、`Vec`。Lean 侧选用与证明性、可执行性、Rust 映照成本三方权衡的表示：

- **Capability / 对象映射**：用**部分函数** `CapId → Option Capability`（或 `Finset`），迁移用函数式纯改写。语义等价于 Rust `HashMap` 的唯一归属/派生链，但**证明内存友好（结构回写）、且可 `#eval`（可执行）**——同时满足"可执行"与"可证明"两个目标。
- **映射结构不同不碍事**：对拍 trace 只承载**具体实例**；Lean 与 Rust 分别从同一输入构造结构，比对的是具体迁移结果，而非结构定义本身。
- **`Option` 表示失败**：迁移返回 `Option State`，`none` 表达"失败且状态不变"（对应 `Result::Err`），`some s'` 表达成功。这使"失败原子性"（§17 基础保持性 4）成为一条可直接证明的定理而非枚举各错误码。

> 默认不依赖 mathlib4：本设计需 `Finset`/部分函数/`Option`/归纳类型，Lean 4 std/core 已够。仅在证明受阻时再增量引入 mathlib4，以减少仓库体积与构建时间。

## 4. 入口文件与首个切入点

### 入口文件 `Formal.lean`
- 声明模型目标（`theorem` 清单）：哪些原语已建模、哪些不变量已证；
- `#eval` 冒烟迁移；
- 作为**工件总纲**，与《Prima OS 实现阶段.md》证据索引呼应（tracker 据此勾选 S0-X3）。

### 首个切入点：bootstrap（对拍样板）
Lean 定义：
- `State`：`booted : Bool`、init task 存在性；
- `bootstrap : State → Option State`：幂等——对已 booted 态返回 `none` 表达 `AlreadyBooted` 且状态不变。

证明三个性质：
1. 成功结果 WellFormed（init 仅持初始 Authority）；
2. 幂等唯一性（第二次返回 `none` 且状态不变，对应 §14 AlreadyBooted / W14 唯一性细化）；
3. 投影保真（`α(booted) ≠ α(pre_bootstrap)`）。

对拍：构造同一 pre_boot 机器态 → Lean `#eval` 与 Rust 既有 `abstract_ops::bootstrap()` 输出一致。

## 5. 原语建模顺序（增量、每原语独立可验收）

bootstrap（样板）→ exc_resume → tlb_refine → memseg_refine → cow_refine。

- **exc_resume**：核心不变量为**失败原子性**——各失败路径返回 `none` 且状态完全不变（§17 基础保持性 4；Rust `exc_resume_rejects_*_atomically` 测试已逐项 pin）。Lean 证明为一条统一定理：`excResume` 每返回 `failure` 分支下 `s' = s`。
- **tlb_refine**：per_task TLB、非一致刷新、显式 `invalidate_va`/`flush_all`；证明候选不变量——写前显式失效后读一致（对齐 Rust 已证行为）。
- **memseg_refine**：W6（物理帧唯一归属：活动 segment 帧两两不交、与 free 不交、并集 = frame_universe）/ W7（vspace 双视图一致）。用部分函数/集合在"具体实例"层面写成可证明 `theorem`（host 语义面，不模拟真实 CR3/PTE）。
- **cow_refine**：主要表达"共享只读 → 写 fault → 分配相离新段换映射 → exc_resume → 显式调度"组合序列**不必新增一等原语**这一结论（H1 承载体）。Lean 中为**组合态不变量 / 可执行序列**而非深度安全证明。

## 6. 环境安装与脚手架

### 安装（本会话）
1. winget 安装 `Lean.Elan`（4.2.3，工具链管理器，Rust 风格）；
2. 经 elan 安装 `leanprover/lean4:stable`（含 `lake` 构建工具）；
3. 验证 `lean --version`、`lake --version`；
4. 备选通道：`Lean.Lean` winget 包、或 scoop/npm，最坏回退为设计冻结、待装好再实现。

### 脚手架
```
lean/
  lakefile.lean           # lake 项目配置，指向 Lean 4.33
  Formal.lean             # 入口 + #eval 冒烟
  State.lean              # Lean State + 映照契约
  Bootstrap.lean          # bootstrap + 证明（样板）
  ExcResume.lean          # exc_resume + 失败原子性证明
  ...                     # 后续原语按需增补
  README.md               # 映照契约、证据边界说明
```

不修改现有 Rust workspace（`Cargo.toml` 成员不变，`lean/` 为独立 lake 项目）。

## 7. 验收标准（BLK-001 关闭 / S0-X3）

1. `lean/` 项目建成，`lake build` 无错误，`Formal.lean` 通过 `#eval` 冒烟；
2. **至少 bootstrap + exc_resume 两个原语在 Lean 中完成不变量证明**（失败原子性、WellFormed、幂等），并建立对拍样板；
3. Lean ↔ cosir 对拍：bootstrap 与 exc_resume 的具体状态序列，Lean `#eval` 与 Rust 一致；
4. 映照契约（Lean 结构与 cosir `State` 投影一致）文档化于 `lean/`；
5. `Prima OS 实现阶段.md` 记录新证据 ID（如 SPEC-019），BLK-001 → Closed、S0-X3 → 勾选（S0-X1 独立规格审查另行推进）；
6. 其余原语（tlb/memseg/cow）的 Lean 建模若首批做不完，**登记为后续切片**，不阻塞 BLK-001 关闭；
7. Rust workspace 全绿（cosir 既有测试、xtask、QEMU 启动闭环无回归）——Lean 项目独立，不引入 Rust 侧回归。

## 8. 风险与回退

| 风险 | 缓解 |
|---|---|
| Lean 4.33 + elan 在 Windows 上装不上/构建失败 | winget 多通道/打包备选；最坏回退为先冻结设计、待装好再实现 |
| mathlib4 是否需要（证明难度） | 默认不拉；证明受阻再增量引入 |
| 证明耗时过大、切片超时 | 严格执行最小门槛 bootstrap+exc_resume；tlb/memseg/cow 登记后续切片 |
| 对拍通道 Rust 侧改造成本 | `lean_backtrace.rs` 复用现有夹具与 `build_state`，改动收敛于一个测试文件 |
| Lean 与 cosir 语义漂移 | 映照契约为唯一权威；漂移记录为 ISSUE（对齐 ISSUE-MEM-006） |

## 9. 交付物汇总

1. Lean 项目 `lean/`（形式化模型工件，至少 bootstrap + exc_resume 证明）；
2. Rust `lean_backtrace.rs` 对拍样板；
3. 数据结构与不变量决策、映照契约记录（`lean/README.md`）；
4. `Prima OS 实现阶段.md` 证据登记 + BLK-001 / S0-X3 更新；
5. 后续切片待办（tlb/memseg/cow 的 Lean 建模）。
