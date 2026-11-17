---
id: report-p2b-linux-frontend
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: historical
updated: 2026-11-17
---

# P2b-2：Linux 前端切换到 Store 批量数据面

## 结论与范围

本报告记录 P2b-2：Linux personality 不再持有 `Ram`/`OpenFiles` 数据面，
`open/read/stat/getdents/lseek/close/newfstatat` 改为经 `Session` 提交单 op
批量，并按 Linux ABI 重新编码 stat 与 dirent。证据到 host 测试、workspace、
lint 与既有 Linux 差分 QEMU 套件；**管理器签发的代理会话尚未接线（P2b-4）、
fork/exit 生命周期未迁移（P2b-3）、代理路径无 QEMU 正反例（P2b-5），P2 未
关闭，7-A 未勾选。**

## 基线与输入

- 基线 `e6150e7`（P2b-0/P2b-1 已提交）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P2b-0 节；
  计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)
  的 P2b-2 节。
- QEMU 回归使用 pinned `PRIMA_LINUX_ELF=/usr/bin/busybox`
  （BusyBox v1.36.1、静态、sha256 `dbac288c…4e62d6ac14`）。

## 实现内容

- 新增 `prima-user/src/linux_store.rs`：
  - `Store` trait 是传输缝；`Session` 负责序号、单 op 页面编码与完成状态解码，
    提供 `open/read/write/stat/stat_path/getdents/lseek/close`。
  - `Local` 在进程内执行同一 `bulk::execute` 编解码与 OFD 语义，携带原 Linux
    私有 fixture（`/fixture.txt`、`/fixturedir/{alpha,beta}`、只读根）；
    `Exchange` 先校验序号再产生文件效果。它只是 P2b-4 之前的传输实现，
    不是前端的数据面。
- `prima-user/src/linux.rs`：
  - `Shared` 只保留输入位置、按 wire fd 的 `F_GETFL` 值、段表；删除 `Ram`、
    `OpenFiles`、`dir_position`、`select`。
  - `Process.fds` 改为 app fd → wire fd（Store OFD 句柄）映射；`open` 由
    `entry.policy` 派生的视图解析（`DIRFD_ROOT` 或目录 wire fd），保留原有
    read-only/NONBLOCK/CLOEXEC/LARGEFILE 接受集与 `EMFILE` 槽位上限。OPEN 只回
    wire fd，前端随后提交一次 STAT 记录句柄类型，使目录 `read` 保持 `EBADF`。
  - `read` 失败回滚改用 Store `LSEEK`；`stat`/`newfstatat` 解码 16 B 中性记录后
    再写 144 B Linux stat；`getdents` 只发射完整 Linux 记录，未发射的 Store
    尾记录用 `LSEEK` 回退；`gate_copy_out` 失败时同样回退已消费的记录。
- `prima-user/src/linux_process.rs`：`Family` 持有 `Session`；`fork_inner`
  只复制 fd 表，Store owner 预留/回滚留给 P2b-3；`exec` 只清 CLOEXEC 槽位，
  Store 侧关闭留给 P2b-3；`cleanup` 只释放 personality 段。
- `prima-user/src/linux_main.rs`：每个 case 注入一个 `Local`；真实会话替换点
  已标注。
- host 测试：`Session` 经 `Local` 覆盖 open/write/read/stat/lseek/getdents/close
  往返、错误映射、序号错配无文件效果、半记录回退；`linux_dirents` 覆盖整记录
  裁剪与消费字节计数。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 本次 | PASS；1166 passed、0 failed、1 ignored | host |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy `-D warnings`、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox cargo check -p prima-user --features linux-probe --bin prima-linux` | 本次 | PASS | host/编译 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -p xtask --features linux-probe -- test-linux` | 本次 | PASS；参考 11 项；Prima 两轮各 11 项，stdout/stderr/status/审计均相符 | QEMU（既有套件，非代理路径） |
| `python3 scripts/docs.py self-test` / `build` / `check` | 本次 | PASS | 文档 |
| `check --base e6150e7` | 本次 | PASS | 文档 |

## 失败、反例与限制

- clippy `-D warnings` 首轮因 `Shared::new` 缺 `Default`、三个内部 helper 参数过多、
  一处无效果 `..Request::default()` 拒绝；补 `Default`、按既有代码加
  `#[allow(clippy::too_many_arguments)]`、删除无用更新后通过，属工具门记录。
- `linux_dirents` 测试初版预算/消费字节推导有误；按记录长度 24 B 重算后通过，
  不改变实现。
- 新鲜上下文复核发现并已修复：
  - GETDENTS 在 `gate_copy_out` 失败时未回退已消费的记录，重试会跳过条目；
    现回退 `consumed`，`failed_dirent_copy_rewinds_the_shared_store_cursor`
    在无修复版本上 RED、修复后 GREEN。
  - Store `READ` 对目录返回目录字节流（原生语义），而 P2b-0 的 Linux 矩阵要求
    目录 `read` 返回 `EBADF`；现 open 后以 STAT 记录句柄类型，`read` 返回 `-9`，
    `directory_handles_reject_read_with_ebadf` RED→GREEN。
  - `Local` 的 `Unformatted`/`Io` 映射由 38/14 对齐为契约的 5。
  - 1～4 B 的 `getdents` 预算此前被当作 EOF；现至少读取一条最小中性记录，
    仍返回 `EINVAL` 并回退游标（`tiny_dirent_budget_reports_einval_instead_of_eof`）。
  - fork 后单 owner 共享/关闭语义未修，按计划留给 P2b-3，不隐藏。
- **传输仍是进程内 `Local`**：前端已不持有文件内容，但 QEMU 中的字节仍来自
  `Local` 的 fixture，而不是管理器签发会话的真实 Store IPC。真实接线、拒绝
  伪造/旧 incarnation、`/linux`、`/native`、`/shared` 的启动策略执行属 P2b-4。
- **fork/exec/exit 的文件所有权未迁移**：本片删除本地 `fork_owner`/`close_owner`
  与 CLOEXEC 关闭调用，仅保留 fd 表复制/清空；owner 预留、失败回滚、退出关闭、
  槽位复用属 P2b-3。既有 `M4-fork` QEMU case 不覆盖文件共享，故本片回归通过
  不证明该语义。
- `read` 上限从 4096 B 降为批量载荷 3808 B（短读合法）；空路径返回 `EINVAL`；
  `newfstatat` 空路径且无 `AT_EMPTY_PATH` 由编解码拒绝（`EINVAL`）。
- 大目录、并发目录流、Store 侧 `EMFILE` 经真实代理、跨代理并发仍未验证。

## 阶段/profile 影响

不改变阶段 7 九轨勾选，不关闭 P2，不构成 7-A 证据，不改变 profile v1/v2 或
D12 白名单。既有 Linux 差分事件与回执未修改。

## 后继关系

- 计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)。
- 下一片：P2b-3 fork/exit 生命周期迁移（Store owner 预留/发布/回滚/关闭）。
- 之后 P2b-4 镜像与管理器会话接线、P2b-5 代理路径 QEMU 正反例。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P2b-0 节。
