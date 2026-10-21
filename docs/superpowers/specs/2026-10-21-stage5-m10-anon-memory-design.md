# 阶段 5 / M10：匿名内存与保护（设计）

**日期**：2026-10-21
**状态**：设计已落地；实现与 QEMU 证据见
[2026-10-21-stage5-m10-anon-memory.md](../reports/2026-10-21-stage5-m10-anon-memory.md)。
**上位计划**：[阶段 5 修订计划](../plans/2026-10-18-stage5-revised-plan.md)片 E 与
[musl 移植计划](../plans/2026-10-18-musl-port-plan.md) M10。
**范围**：把 M3b 的“预映射 bump arena + 空 `munmap`/`mprotect`”推进为按页分配、
真正拆/复映射、内核改权的匿名内存子集；同时实现 COSIR §7.4 的 `memseg_protect`。
不包含文件映射、`MAP_SHARED`、`mremap`、硬件 NX 或完整 POSIX `mmap`。

## 1. 问题

M3b 的适配层把 96 页预映射 arena 切成 `brk` 与 bump `mmap` 两段，但：

- `munmap` 是空操作，地址不能复用，单次/累计映射受 56 页 bump 上限约束；
- `mprotect` 返回成功但不改权限（匿名页一律 RW），无法支撑 PROT_NONE/只读契约；
- 内核只有 `memseg_create/map/unmap/destroy` 的规范与实现，COSIR §7.4 的
  `memseg_protect` 仍只有规范文本；
- W^X 只在 ELF 装载与 `memseg_map` 的 Segment `allowed_access` 上成立，没有
  “对已映射页改权”的强制路径。

M10 的取舍是：**不新建 Segment、不改 ABI 语义**，在既有 arena 上做真实的页级
分配/回收，并把改权下沉到内核；C 任务通过一次性 cap 申请获得对 arena 的
`Derive|SegMapR|SegMapW` 授权，之后 `memseg_map/unmap/protect` 都直接走原生号。

## 2. 内核 `memseg_protect`（COSIR §7.4）

### 2.1 调用号与参数

追加 `SYS_MEMSEG_PROTECT = 41`（38/39/40 已用于 FS base 与 PIT 时间）：

```text
memseg_protect(task_cap, target_base, page_count, new_access) -> ()
```

`new_access` 是 COSIR Segment 访问位集（`SegMapR=2`、`SegMapW=4`、`SegMapX=8`）。

### 2.2 前置（逐条实现）

1. `task_cap` 由调用者持有、`TaskManageVSpace` 有效，目标为 **CPU Task**；
2. `page_count > 0`；
3. `[target_base, target_base + page_count)` 每一页都在目标 Task 的逻辑 `vspace`
   中已映射；
4. `new_access` 非空、类型合法且是 `{R,W,X}` 的子集；
5. 每一页的 `new_access ⊆ segment.allowed_access`；
6. 每页 `added = new_access \ 当前 access`：`added` 中每个权限都要求调用者持有
   至少一条指向该页 Segment、含对应 `SegMapR/W/X` 的活动 Capability；缩小或保持
   权限不需要新授权；
7. 若区间包含 `page_of(task.entry)`，`new_access` 必须含 `X`（W8）。

失败顺序固定、且**全部前置在任何写入之前完成**；任一失败不改变 `vspace`、PTE
或审计。

### 2.3 成功动作

- 对每页仅替换逻辑条目的 `access`，`segment`/`offset` 不变；
- 同步改写叶 PTE：`W` 置/清 `PT_WRITABLE`，保留物理帧与 `PT_USER`；
  目标根等于当前根时逐页 `invlpg`；
- 追加一条 `AuditEvent::VspaceProtChanged { task_id, base, page_count, access,
  task_cap }`；§7.4 以 `base + page_count` 表达整段区间的原子改权。

### 2.4 错误映射

`ZeroSize`/`InvalidAccess`/`WrongRightType` → `EINVAL`；`NotMapped`/
`InvalidTask`/`TargetDead`/`CapacityExceeded` → `ENOENT`；`MissingRight`/
`MissingMapRight`/`EntryMapping` → `EACCES`；`OutOfBounds` → `EFAULT`。
C 适配层只把 `ENOENT` 翻译为 `ENOMEM`（POSIX `mprotect` 的未映射错误），
其余保持。

### 2.5 不变量

- **失败原子**：验证/审计容量预检先做；提交段只做已经证明不会失败的 PTE 与
  条目写入。
- **W8**：入口页不能因保护变更失去 `X`。
- **W7**：只改已映射页的权限，不新增映射；未映射页返回 `NotMapped`。
- 多页区间可跨 Segment：逐页读取该页的 `seg_id`/`allowed_access`，授权也按页
  检查。

## 3. C 侧匿名内存

### 3.1 arena 布局

服务管理器为 C 任务创建 **104 页** Segment（`C_STACK_PAGES + C_HEAP_PAGES = 8 + 96`），
其中：

| Segment 偏移 | 映射 VA | 用途 | 适配层所有权 |
|---|---|---|---|
| 0..7 | `ENDPOINT_STACK_VA` | C 栈 | 服务管理器 |
| 8..47 | `0x0000_0c00_0000_0000` 起 40 页 | `brk`（mallocng 元数据） | 适配层 `brk` |
| 48..103 | `0x0000_0c00_0000_0000 + 40*4096` 起 56 页 | 匿名 `mmap` | 适配层按页分配器 |

`mmap` 区 56 页 = 229376 字节。适配层维护两张 56 位图：`used`（用户是否占用）
与 `present`（PTE 是否仍在）。`used` 做 first-fit 连续页分配；`present` 区分
“首次使用（服务管理器已预映射 RW）”与“被 `munmap` 真正拆除后需要复映射”。

### 3.2 arena 映射授权的握手

`mmap` 首次调用时，适配层向服务管理器发送 `SERVICE_VMA_CAP_REQUEST_TAG`
（`0x5ED9…`，携带 client id）。管理器只对运行中的 C 任务（`capacity_probe` 存在）
从该任务的 arena 根 Segment cap **派生**一条
`Derive|SegMapR|SegMapW`（不含 `SegMapX`/`SegDestroy`）并通过
`SERVICE_VMA_CAP_REPLY_TAG`（`0x5EDA…`）随消息转移。失败时回 `UNAVAILABLE`
状态（不阻塞 C）。

适配层缓存该 cap（每个任务一次）：

- 有 cap：`munmap` 用 `SYS_MEMSEG_UNMAP` 真正拆 PTE；复用地址时用
  `SYS_MEMSEG_MAP` 以 `segment_offset = 48 + page_index` 复映射；
  `mprotect` 的新增权限由该 cap 授权；
- 无 cap：退化为“只 free 不 unmap”，不复用未映射页，R→RW 的 `mprotect`
  返回 `EACCES`。该退化路径不伪装成成功。

派生 cap 只在 C 任务存活期存在；C 自毁时由内核撤销，管理器的根 cap 仍负责
最后 `memseg_destroy`。

### 3.3 `mmap`

冻结子集：`MAP_PRIVATE|MAP_ANONYMOUS`、`fd=-1`、`offset=0`；其余返回 `EINVAL`。

- `length == 0` → `EINVAL`；取整后超过 56 页 → `ENOMEM`；
- 非 `MAP_FIXED`：first-fit 找连续空闲页；地址作为 hint 被忽略；
- `MAP_FIXED`：地址必须页对齐；**brk 区内的请求**按原地址成功返回（mallocng
  的 guard 页，保持 M3b 行为，不改变权限）；mmap 区内先释放重叠页再按固定地址
  映射；区外返回 `ENOMEM`；
- 分配成功后清零整段（复用的帧可能残留旧字节）；
- 初始 `prot` 生效：`PROT_NONE` 近似为 `R`（见 §3.5），`PROT_READ` → `R`，
  `PROT_READ|PROT_WRITE` → `RW`，`PROT_WRITE` → `W`；`PROT_EXEC` → `EACCES`；
  对已预映射页调用一次 `memseg_protect` 落实初始权限。

### 3.4 `munmap`

- `address` 页对齐、`length > 0`，否则 `EINVAL`；
- 区间必须完全落在 mmap 区内，否则 `EINVAL`；
- Linux 允许 `munmap` 未映射页；本实现同样只清 `used`，对仍 `present` 的连续
  段调用一次 `memseg_unmap`，成功后清 `present`；
- 部分释放（页粒度）合法；释放后再次 `mmap` 可复用该 VA 并重新清零。

### 3.5 `mprotect` 与 PROT_NONE/EXEC

- `address` 页对齐、`length > 0`，否则 `EINVAL`；
- 区间必须全部是 **仍被占用且已映射** 的 mmap 区页，否则 `ENOMEM`（Linux 对
  未映射区间返回 `ENOMEM`）；
- `PROT_NONE`：COSIR 的 `new_access` 不能为空，故取最小非空集 `R`，即
  “不可写、不可执行但仍可读”。这是显式偏差，必须写入 profile 边界；
- `PROT_EXEC`（含与其他位组合）：匿名 Segment 的 `allowed_access` 只有 `RW`，
  内核返回 `EACCES`。这构成 `mmap`/`mprotect` 路径上的 W^X：匿名页永不可执行，
  可写页永不能变成可执行页；
- 收缩（RW→R、R→W）不需要 Segment 授权；新增 `R/W` 需要 §3.2 的 cap；
- 内核 `ENOENT`（未映射/目标错误）翻译为 `ENOMEM`。

### 3.6 错误码冻结

| 场景 | Linux | Prima |
|---|---|---|
| `mmap(len=0)` | `EINVAL` | `EINVAL` |
| `mmap` 区耗尽/超 56 页 | —（宿主成功） | `ENOMEM` |
| `munmap` 非对齐 / `len=0` | `EINVAL` | `EINVAL` |
| `mprotect` 非对齐 | `EINVAL` | `EINVAL` |
| `mprotect` 未映射页 | `ENOMEM` | `ENOMEM` |
| `mprotect(PROT_EXEC)` 匿名页 | 成功 | `EACCES`（W^X） |

## 4. 冻结测试 A1

`prima-user/c/stage5_mem.c` 同一份源码在 Linux（glibc）与 Prima（静态 musl +
适配层）编译运行，输出：

```text
MEM|ok=1;map=1;zero=1;write=1;protect=1;protect_back=1;fixed=1;reuse=1;none=1;exec=1;mmap_err=1;munmap_err=1;mprotect_err=1;unmapped=1;big=1;native=1
```

覆盖：首次清零/读写、RW↔R、`MAP_FIXED` 替换、`munmap` 后复用并清零、
`PROT_NONE` 往返、错误码、140000 字节 `malloc`（mmap）后 `free`（munmap）与
`realloc`；Prima 侧另以原生号直调 `memseg_protect` 验证
`ZeroSize`/`InvalidAccess`/`NotMapped` 与收缩/扩权成功路径。`exec` 字段按平台
断言“本平台的冻结期望”：Linux 允许匿名可执行，Prima 必须 `EACCES`。

## 5. 已知边界

- 不支持文件映射、`MAP_SHARED`、`MAP_GROWSDOWN`、`MAP_LOCKED`、`mremap`、
  `madvise`、`msync`；
- `brk` 仍是 40 页上限；`mmap` 区固定在 56 页，不向内核申请新 Segment；
- `PROT_NONE` 不是不可访问，只是只读；硬件 NX 未启用，`X` 是 COSIR 访问位而
  非硬件执行位；
- 非 arena 地址的 `mprotect` 返回 `ENOMEM`（Linux 可能成功）；mmap 区外的
  `MAP_FIXED` 返回 `ENOMEM`；
- `munmap` 页表拆除会归还变空的页表帧；Segment 数据帧仍在原 Segment 内，
  直到任务退出/管理器销毁，不按映射回收；
- 退化模式（未取得 cap）不拆映射、不复用未映射页，不能据此宣称完整 `munmap`。

## 6. 证据等级

- **host**：kernel `rights`/`vspace`/`bootstrap` 单测（PTE 可写位、失败原子、
  `VspaceProtChanged`）；
- **QEMU**：A1 双轮 + 内核既有 FAULT/TASKCAP/CAPACITY 见证；
- **Linux/Prima 差分**：A1 stdout 逐字节一致（`exec`/`native` 为各平台冻结
  期望）；
- 不构成硬件 NX、完整 POSIX `mmap`、文件映射或堆回收证明。
