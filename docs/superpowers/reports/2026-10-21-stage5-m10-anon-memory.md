# 阶段 5 / M10：匿名内存与保护（`mmap`/`munmap`/`mprotect` + 内核 `memseg_protect`）

**日期**：2026-10-21
**范围**：实现 COSIR §7.4 `memseg_protect`，并把 M3b 的预映射 bump arena 推进为
56 页按页分配、真正拆/复映射、内核改权的匿名 `mmap` 子集。
**结论**：内核 `SYS_MEMSEG_PROTECT=41` 与 `VspaceProtChanged` 审计落地；C 侧
`mmap/munmap/mprotect` 覆盖匿名子集。A1（`stage5_mem.c`）在 Prima 双轮通过且与
Linux 参考逐字节一致；`P5-MM-01` 由 `planned` 推进为 `partial`。未勾选任何
S5-D*/S5-X*。设计与边界见
[M10 设计](../specs/2026-10-21-stage5-m10-anon-memory-design.md)。

## 1. 实现

### 1.1 内核：`memseg_protect`

- `prima-abi` 追加 `SYS_MEMSEG_PROTECT = 41`（39/40 已是 PIT 时间）。
- `BootstrapState::memseg_protect(task_cap, base, pages, new_access, root, offset)`
  按 §7.4 逐条验证：`TaskManageVSpace` + CPU Task、页对齐/溢出、区间全部已映射、
  `new_access` 非空且 ⊆ 每页 Segment 的 `allowed_access`、新增权限需要**当前持有**
  的对应 Segment 映射 Capability、入口页不得失去 `X`。全部前置通过后才写状态。
- 提交段同步改逻辑条目的 `access` 与叶 PTE 的 `PT_WRITABLE`（保留物理帧与
  `PT_USER`），当前根逐页 `invlpg`；追加一条
  `AuditEvent::VspaceProtChanged { task_id, base, page_count, access, task_cap }`。
- `MemsegError` 新增 `InvalidTask`（非 CPU/无效目标）；syscall 错误映射保持
  `InvalidAccess → EINVAL`、`NotMapped/InvalidTask → ENOENT`、
  `MissingMapRight/EntryMapping → EACCES`、`OutOfBounds → EFAULT`。
- **实现中发现的真实缺陷**：cap 随消息移动后，`CapRecord.holder` 仍是**派生时**
  的属主，权威持有者在位置表。首版授权扫描读 `record.holder`，导致 C 任务拿到
  arena cap 后仍被 `MissingMapRight` 拒绝；改为 `holder_of(handle)` 并新增
  `memseg_protect_authorizes_a_moved_segment_capability` 回归测试。

### 1.2 C 适配层：56 页匿名 arena

- arena 布局不变：8 页 C 栈 + 96 页堆（`brk` 40 页 + `mmap` 56 页，Segment 偏移
  48..103 对应 mmap 区）。新增两张 56 位图：
  - `used`：用户是否占用，first-fit 连续页分配；
  - `present`：PTE 是否仍在，区分“服务管理器预映射的首次使用”与“`munmap` 后的
    复映射”。
- 首次 `mmap` 向服务管理器申请 arena Segment 的派生 Capability
  （`Derive|SegMapR|SegMapW`，不含 `X`/`SegDestroy`），标签
  `SERVICE_VMA_CAP_REQUEST_TAG`/`SERVICE_VMA_CAP_REPLY_TAG`。管理器只对运行中的
  C 任务从 `capacity_probe` 的 stack cap 派生并随消息转移；失败回
  `UNAVAILABLE`，C 侧退化为“只 free 不 unmap”，不阻塞。
- `mmap`：仅 `MAP_PRIVATE|MAP_ANONYMOUS`、fd=-1、offset=0；按页 first-fit；
  `MAP_FIXED` 在 mmap 区内替换、brk 区内保持 mallocng guard 页行为；新页清零；
  初始 `prot` 经 `memseg_protect` 落实；失败回滚本次新映射的页。
- `munmap`：页对齐/非零长度/区间检查；对仍 `present` 的连续段调用
  `SYS_MEMSEG_UNMAP`，清 `present`；VA 可复用，复映射用
  `SYS_MEMSEG_MAP`（`segment_offset = 48 + page_index`）。
- `mprotect`：仅接受仍被占用且已映射的 mmap 区页；`PROT_NONE` 取最小非空访问集
  `R`（近似只读），`PROT_EXEC` 返回 `EACCES`（匿名段 `allowed_access` 只有 RW，
  W^X）；`ENOENT` 翻译为 `ENOMEM`；非法 prot 位返回 `EINVAL`。
- 新增 `prima_native_syscall4`，供探针直调原生号。

### 1.3 服务管理器

新增 VMA 授权分支：校验请求 tag，取 `capacity_probe` 的 stack cap 派生
`ARENA_MAP_RIGHTS`（`Derive|SegMapR|SegMapW`），经 `c_send` 回送；失败或无运行中
C 任务时回 `UNAVAILABLE`。派生 cap 在 C 自毁时由内核撤销，管理器的根 cap 仍负责
`memseg_destroy`。

### 1.4 测试接线

- `xtask` 新增 `Stage5Program::Mem`、`build_images_for_mem`、
  `check_stage5_linux_mem` 与 A1 双轮；Linux 参考用宿主 glibc 编译同一源码。
- 固定探针（stage1 基线）因一次性 cap 申请多出 2 次直投/阻塞，IPC 基线从
  `53/156/164` 更新为 `53/158/166`；审计基线从 1299 更新为 1312。

## 2. 证据

### 2.1 host 单测

```
cargo test -p kernel memseg_protect
running 3 tests
memseg_protect_atomically_updates_access_and_write_bit ... ok
memseg_protect_rejects_bad_ranges_and_access_atomically ... ok
memseg_protect_authorizes_a_moved_segment_capability ... ok
```

覆盖：PTE 可写位同步、`VspaceProtChanged` 事件、失败原子（PTE/访问集/审计不
变）、`ZeroSize`/`InvalidAccess`/`NotMapped`/`OutOfBounds`、新增权限缺授权、
cap 移动后的持有者判定。`rights::difference` 另有差集单测。

### 2.2 QEMU（`cargo xtask test`，A1 双轮）

```
MEM|ok=1;map=1;zero=1;write=1;protect=1;protect_back=1;fixed=1;reuse=1;none=1;exec=1;mmap_err=1;munmap_err=1;mprotect_err=1;unmapped=1;big=1;native=1
ECHO|status=0;len=150;hash=2299001577
TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132
CAPACITY|caps_live=132;caps_peak=140;caps_limit=160;segment_slots=41;segment_limit=48;messages_queued=1;messages_peak=14;messages_limit=32;audit_used=2507;audit_limit=8192
```

M5–M9 的全部既有 QEMU 轮（echo E1–E6、T1、F1、V1、M1、C1、L1–L3）在本切片后
继续双轮通过，证明新分配器/真实 unmap 未破坏 sbase 与既有探针。

### 2.3 Linux/Prima 同源差分

`check_stage5_linux_mem` 用宿主 glibc 编译 `stage5_mem.c`，要求退出 0 且 stdout
逐字节等于 `MEM_EXPECTED_STDOUT`；Prima 侧同一常量。`exec` 字段是平台冻结期望
（Linux 允许匿名可执行，Prima 必须 `EACCES`），`native` 在宿主为不适用（1）。

## 3. 容量与既有证据变化

| 项 | 旧（M9） | 新（M10） | 原因 |
|---|---:|---:|---|
| 固定探针 IPC `queued/direct/blocks` | 53/156/164 | 53/158/166 | 首次 `mmap` 的 arena cap 申请：1 次请求 + 1 次应答直投，及对应阻塞 |
| 固定探针 `audit_used` | 1299 | 1312 | cap 派生/移动、`memseg_map`/`unmap`/`protect` 审计 |
| A1 `audit_used` | — | 2507 | 探针多次 map/unmap/protect 与文件通道 |
| A1 `segment_slots` | — | 41 | 未新建 Segment（仍复用栈同段） |
| A1 `caps_live/peak` | — | 132/140 | 派生 cap 在 C 退出后被撤销，峰值未涨 |
| 新审计事件 | — | `VspaceProtChanged` | §7.4 成功动作 |

`MAX_SEGMENT_FRAMES`、`MAX_VSPACE_ENTRIES`、`MAX_AUDIT_EVENTS` 均未调整。

## 4. 已知边界（`P5-MM-01 = partial`）

- 只支持 `MAP_PRIVATE|MAP_ANONYMOUS`、fd=-1、offset=0；无文件映射、
  `MAP_SHARED`、`mremap`、`madvise`、`msync`；
- mmap 区固定 56 页，不向内核申请新 Segment；`brk` 仍 40 页；
- `PROT_NONE` 近似为只读（COSIR `new_access` 不能为空），页仍可读；
- 硬件 NX 未启用，`X` 是 COSIR 访问位而非硬件执行位；匿名 `PROT_EXEC` 在适配层
  返回 `EACCES`，`mmap` 的初始 prot 也不能是 X；
- `MAP_FIXED` 只接受 arena 内地址（brk guard 页按原行为成功）；区外 `ENOMEM`；
- 非 arena 地址的 `mprotect` 返回 `ENOMEM`；
- `munmap` 归还变空页表帧，但 Segment 数据帧仍随任务/管理器生命周期回收；
- 管理器未授予 cap 时退化为“只 free 不 unmap”，R→RW 的 `mprotect` 返回
  `EACCES`；该路径不作为支持证据；
- 未验证硬件写保护故障注入（QEMU 的固定用户 FAULT 见证仍只由既有测试提供）；
  保护变更的 PTE 效果由 host 单测 `set_leaf`/`memseg_protect` 覆盖。

## 5. 复现

```sh
cargo test --workspace          # 含 memseg_protect/rights 单测
cargo xtask lint                # fmt/clippy/裸机 unsafe 守卫
cargo xtask test                # A1 双轮 + 全部既有 BIOS 轮
```

产物与日志：`target/prima-os/stage5-mem.elf`、`target/prima-os/A1.log`、
`target/prima-os/mem-linux`。

## 6. 文件

- `prima-abi/src/lib.rs`：`SYS_MEMSEG_PROTECT=41`
- `kernel/src/rights.rs`：`Rights::difference` + 单测
- `kernel/src/bootstrap.rs`：`memseg_protect`、`caller_holds_segment_right`、
  `VspaceProtChanged`、`MemsegError::InvalidTask`、vspace `set_access`、单测
- `kernel/src/syscall.rs`：`memseg_protect_call` + 分发 + errno 映射
- `prima-user/c/prima_syscall.c`：56 页 arena 分配器、VMA cap 握手、
  `mmap/munmap/mprotect`、`prima_native_syscall4`
- `prima-user/c/prima_abi.h`、`prima_syscall.h`：原生号/访问位/标签/声明
- `prima-user/c/stage5_mem.c`：A1 同源探针
- `prima-user/src/servicemgr.rs`：VMA 授权分支与 `ARENA_MAP_RIGHTS`
- `prima-runtime/src/protocol.rs`：VMA 请求/应答标签与打包函数
- `xtask/src/image.rs`、`main.rs`、`protocol.rs`：M10 程序、Linux 参考、A1 双轮、
  IPC/审计基线
