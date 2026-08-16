# PrimaOS 审计缺陷修复设计

> 状态：已批准
>
> 日期：2026-08-16
>
> 范围：修复已确认的规范治理、COW、单核调度、TLB、组合状态、MemorySegment Authority 边界、测试与证据措辞缺陷
>
> 证据边界：本设计只产生 host `MODEL-TEST` 与既有 `BOOT-PROTOCOL-TEST`；不关闭 BLK-001，不宣称 `KERNEL-TRACE`、`REFINEMENT-PROOF` 或 `COMPAT-TEST`

## 1. 目标

本次修复使规划、规范、host 模型和测试重新服从 PrimaOS 的核心设计原则：

1. POSIR observable semantics 优先于实现便利；
2. Capability 是受保护操作的 Authority 载体；
3. 所有失败在完整抽象状态上原子不变；
4. 单核任意时刻最多一个 `Running` Task；
5. COW 策略位于用户态，内核模型只提供通用 fault、映射、恢复和 TLB 机制；
6. `MODEL-TEST` 不得被表述为真实内核细化或形式化证明。

## 2. 非目标

本次不实现：

- 真实 x86_64 CR3/PTE、IDT、page-fault trap、`invlpg` 或 `iretq`；
- Linux `COMPAT-TEST`；
- BLK-001 所要求的形式化工具工件；
- 完整 Capability 派生、移动、Namespace 容器或 W2–W4 机器证明；
- SMP TLB shootdown、ASID/PCID 或替换策略；
- 帧字节内容复制。host 模型中的 COW 复制仍表示为分配页数相同、物理帧相离的新 Segment。

## 3. 修复策略

采用增量收敛，而非局部补丁或一次性重写：

1. 修正规范冲突和证据措辞；
2. 先写失败测试，固定预期语义；
3. 修复 scheduler、COW 和 TLB 局部接口；
4. 引入统一 COW 垂直切片状态；
5. 将 MemorySegment 受保护操作接入 Capability authority adapter；
6. 删除手工跨模型同步入口；
7. 归档历史计划并同步 tracker；
8. 完成全量回归和独立审查。

现有局部模型 `RefineState`、`ops::State`、`TlbState` 继续承担各自机制，不进行无关重写。

## 4. 统一组合状态

新增 `CowSliceState`，统一拥有：

```text
CowSliceState {
    memory: RefineState,
    ops: State,
    tlb: TlbState,
    current: Option<TaskId>,
}
```

### 4.1 所有权边界

- `RefineState`：Segment、frame allocator、逻辑 page table、映射物理身份；
- `State`：Task 抽象状态、Capability、exception、pending exception、恢复帧和 Audit；
- `TlbState`：per-task 缓存项；
- `current`：单核当前执行 Task 的唯一权威。

COW 建立、访问、fault、换映射、失效、恢复和调度只能通过 `CowSliceState` 的具名迁移执行。测试不得直接写任一子状态来维持跨模型同步。

### 4.2 组合不变量

组合状态至少检查：

1. 最多一个 `Running` Task；
2. `current = Some(t)` 当且仅当 `t` 是唯一 `Running` Task；
3. 参与切片的 Task 在 memory 与 ops 视图中同时存在；
4. 两个视图的映射在 Segment、offset、物理帧、R/W/X、user/present 语义上一致；
5. `Faulted` Task 具有唯一 pending exception，且不是 `current`；
6. TLB 只属于存活 Task；PTE 更新后、显式失效前允许存在已标识的 stale entry；
7. COW 建立完成后父子映射同一 Segment/frame 且均无 W；
8. COW 私有化后新旧 Segment 页数相同且物理帧集合相离；
9. 失败迁移不改变完整 `CowSliceState`。

组合检查替代当前只比较 `COW_VA` 权限的 `cow_state_consistent`。

## 5. 单核调度语义

### 5.1 `schedule`

`schedule(task)` 改为返回 `Result`：

- Task 必须存在且为 `Ready`；
- `current` 必须为 `None`；
- 不允许已有其他 `Running` Task；
- 成功时原子执行 `Ready → Running` 与 `current = Some(task)`；
- 非法状态返回稳定错误，完整状态不变。

fixture 不再直接构造两个 `Running` Task。

### 5.2 fault 与恢复

fault 提交：

```text
Running(t) + current=Some(t)
→ 保存异常帧
→ pending_exc(t)=...
→ Faulted(t)
→ current=None
```

`exc_resume` 继续只执行 `Faulted → Ready`，不得隐式调度。恢复后必须显式调用 `schedule` 才能再次运行。

## 6. COW 协议

### 6.1 建立 COW

新增组合迁移：

```text
establish_cow(parent, child, segment_cap, child_task_cap, va)
```

验证阶段：

1. actor、父子 Task、Segment 和 Capability 存在且 active；
2. actor 持有 Segment 映射权和目标 Task `TaskManageVSpace`；
3. 父当前映射该 Segment，且具有可写私有映射；
4. 子目标范围未映射；
5. 地址 canonical、页对齐且范围不溢出；
6. 所有组合不变量在迁移前成立。

提交阶段：

1. 为子建立只读共享映射；
2. 将父映射降为只读；
3. 对父、子受影响地址执行显式 TLB 失效；
4. 同步更新 memory 与 ops 视图；
5. 验证父子指向同一 Segment/frame 且均不可写。

任一验证失败时完整组合状态不变。

### 6.2 写故障与私有化

写只读 COW 页：

```text
Running
→ PageProtectionViolation
→ Faulted + pending exception + current=None
→ 用户态 handler 分配相离新 Segment
→ 换为私有可写映射
→ 显式 TLB invalidation
→ exc_resume
→ Ready
→ schedule
→ Running
→ write succeeds
```

父和子首次写都必须走相同 fault 路径。host 模型不声称已复制字节内容。

## 7. TLB 语义

### 7.1 访问类型

删除 `for_write: bool`，改为：

```rust
enum AccessKind {
    Read,
    Write,
    Execute,
}
```

权限判断分别检查 `ACCESS_R`、`ACCESS_W`、`ACCESS_X`。TLB 命中与 page walk 使用同一访问谓词。

### 7.2 生命周期与失效

- `translate`、`invalidate_va`、`flush_all` 以组合状态中的 Task 表为存在性权威；
- 存活 Task 尚无 TLB cache 时，失效成功且无操作；
- `invalidate_va` 验证 canonical 与页对齐；
- Task 销毁时删除对应 TLB；
- 不存在 Task 返回 `NoSuchTask`；
- 合法但无条目的地址失效幂等成功。

## 8. Capability Authority 边界

MemorySegment 的受保护映射接口不再接收调用者自报的：

- `seg_cap_rights: u8`；
- `task_manage_rights: bool`。

公开组合接口接收：

```text
actor: TaskId
seg_cap: CapId
task_cap: CapId
```

统一 authority adapter 验证：

1. Capability 存在、active 且不是 stale；
2. holder 等于 actor；
3. object 类型和目标 ID 正确；
4. Segment Capability 具有请求所需的 `SegMapR/W/X` 或 `SegDestroy`；
5. Task Capability 具有 `TaskManageVSpace`；
6. 请求权限非空且不超过 Segment `allowed_access`。

adapter 只返回内部短生命周期的已验证授权结果。测试 fixture 必须通过 Capability 建立 Authority，不得直接传入裸权限位或布尔值。

本次只消除 MemorySegment/COW 路径中的授权绕过；不把该局部 adapter 宣称为完整 Capability 系统细化。

## 9. 错误与原子提交

组合迁移统一采用：

```text
完整纯验证
→ 构造 pending changes
→ 单次提交
→ 组合不变量检查
```

稳定错误至少区分：

- Task、Segment 或 Capability 不存在；
- holder/object 不匹配；
- Capability inactive/stale；
- 权限不足；
- 非法 Task 状态；
- CPU 已有 Running Task；
- 非法或未对齐地址；
- 映射冲突；
- TLB 生命周期不一致；
- 输入组合状态已损坏。

失败测试必须比较完整 `CowSliceState`，包括 allocator、ID、页表、Task、pending exception、TLB、恢复帧与 Audit。

## 10. 规范和文档治理

### 10.1 权威规范

修复：

- 定义并统一原语类别计数口径，消除“11/12 类”冲突；
- 将成功迁移的证明义务更新为 W1–W16；
- 将 `exc_resume` 中未定义的 `SegWrite` 统一到已定义的 Segment 写权限；
- 将 COW 结论改为“受限 host 组合场景未发现新增内核原语需求”；
- 禁止以“证明完整 COW fork”描述普通 Rust 单元测试；
- 更新白皮书和规范版本/修订日期。

### 10.2 证据术语

采用以下最低证据规则：

| 表述 | 最低证据 |
|---|---|
| 测试覆盖某场景 | `MODEL-TEST` |
| 在有限场景未发现反例 | 明确边界的模型测试 |
| 定义细化义务 | `REFINEMENT-DESIGN` |
| 真实实现行为 | `KERNEL-TRACE` |
| 兼容性结论 | `COMPAT-TEST` |
| 证明或定理成立 | `REFINEMENT-PROOF` |

测试名、设计稿、tracker 和冒烟输出同步使用该规则。

### 10.3 历史计划

已完成的 COW/TLB 计划增加：

- `Status: Completed / Archived`；
- 权威后继文档；
- 证据 ID；
- 实现提交；
- 已废弃假设。

COW 历史计划中“副本复用原物理帧”的断言明确标为错误且已被相离新帧语义替代。历史计划不得继续作为当前可执行指令。

`Prima OS 实现阶段.md` 保持为实现完成状态和证据索引的唯一权威来源。

### 10.4 格式与行尾

保留当前两处 Markdown 表格格式化变更，并统一处理三个权威文档的行尾状态。除本设计要求外，不进行无关文案重排。

## 11. 测试设计

严格执行 red-green-refactor：

1. 先增加失败测试；
2. 确认失败原因对应审计缺陷；
3. 写最小实现；
4. 运行局部测试；
5. 完成后运行全量验收。

新增覆盖至少包括：

- COW 建立后父子均只读；
- 父或子首次写均 fault；
- 不允许两个 `Running` Task；
- fault 清除 `current`；
- `exc_resume` 后必须重新 schedule；
- R/W/X 权限矩阵；
- 存活但无缓存 Task 的幂等失效；
- invalidation 拒绝非法地址；
- Task 销毁清理 TLB；
- Capability 缺失、错误 holder、错误 object、inactive/stale、权限不足；
- 每个失败路径完整组合状态不变；
- 任一子模型被直接篡改时，组合不变量检查失败。

现有正确的 `exc_resume` 原子性、MemorySegment allocator/pagewalk 和 QEMU boot protocol 测试必须保持通过。

## 12. 验收门槛

必须 fresh 运行：

```text
cargo fmt --all --check
cargo test -p cosir
cargo test --workspace
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo xtask test
git diff --check
```

还需独立审查：

- 实现是否符合本设计和权威规范；
- 是否存在测试直接篡改状态绕过迁移；
- 是否仍有 `prove`/“证明”等超过证据等级的措辞；
- 是否引入新的 OS personality 或架构专属 POSIR 原语；
- 是否保留 `MODEL-TEST`、`BOOT-PROTOCOL-TEST` 与未来证据等级边界。

## 13. 完成定义

只有同时满足以下条件，范围 A 才完成：

1. 本设计列出的具体规范和实现缺陷全部有对应测试或文档验收；
2. COW 默认路径父子均只读，父子首次写均 fault；
3. 单核 `Running/current` 不变量由 API 强制；
4. TLB 完整区分 R/W/X，并统一 Task/地址生命周期语义；
5. COW 垂直切片不再依赖测试手工同步两个状态；
6. MemorySegment/COW 受保护入口不再接受裸权限布尔值；
7. 历史计划已归档，过期断言不可被误作当前指令；
8. 权威文档、tracker、测试名和实现使用一致证据措辞；
9. 全量验收通过且工作区 diff 只包含批准范围内的变更；
10. 最终结论仍明确：没有关闭 BLK-001，也没有产生真实 `KERNEL-TRACE`、`REFINEMENT-PROOF` 或 `COMPAT-TEST`。
