---
id: report-p2b-fork-exit
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: historical
updated: 2026-11-17
---

# P2b-3：fork/exec/exit 文件生命周期迁移

## 结论与范围

本报告记录 P2b-3：Linux personality 的 fork 预留/发布/回滚、exec 的 CLOEXEC 关闭、
exit 的 owner 关闭不再操作域内 OFD 表，而是经 Store 客户端接口提交；Store 服务新增
代理作用域的预留/发布/回滚消息。证据到 host、workspace、lint 与既有 Linux 差分
QEMU；**真实管理器签发会话仍由进程内 `Local` 代替（P2b-4）、代理路径无 QEMU
正反例（P2b-5），P2 未关闭，7-A 未勾选。**

## 基线与输入

- 基线 `206ec5b`（P2b-2 已提交）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) 新增
  P2b-3 节；计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)
  的 P2b-3 节。
- QEMU 回归使用 pinned `PRIMA_LINUX_ELF=/usr/bin/busybox`（v1.36.1、静态）。

## 实现内容

- `prima-runtime/src/bulk.rs`：新增 `PROXY_RESERVE`/`PROXY_PUBLISH`/`PROXY_RELEASE`
  与 `PROXY_CHILD_SHIFT`；slot 仍在 bits 32..39，key 在低 32 位。
- `prima-user/src/service.rs`：代理作用域生命周期处理。预留以登记项
  `entry.process` 为父、复制 `entry.owner` 的 OFD 持有位并回复 key；发布校验子 Task
  父 Process、把 key 绑定到子 cookie；回滚关闭未发布 owner。槽位、代理 cookie 与
  应用存活仍按登记表校验。
- `prima-user/src/sessions.rs`：`reserve_external` 对已有 owner 的 Process 复用槽位，
  使 fork 子进程注册自己的代理项时继承已发布 owner。
- `prima-user/src/linux_store.rs`：`Store` trait 增加 `exchange(owner,…)`、
  `reserve/publish/release/close_owner`；`Session` 记录当前 owner 与序号。`Local`
  实现 owner 槽位：预留继承 OFD 位、发布绑定子 cookie、释放/退出关闭、按 owner
  的序号、槽位复用。
- `prima-user/src/linux_process.rs`：`Family` 改为每个 Process 槽位一个 `Session`；
  fork 先预留、失败释放、成功后发布并复制 OFD 位与 fd 表；exec 关闭 CLOEXEC fd；
  exit 先关 Store owner 再做域内清理。
- `prima-user/src/linux_main.rs`：fork/exec/exit 传入 store。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 本次 | PASS；1169 passed、0 failed、1 ignored | host |
| `cargo test -p prima-user --lib linux_store` | 本次 | PASS；6 passed（含共享游标继承、owner 关闭、回滚与槽位复用） | host |
| `cargo test -p prima-user --lib sessions` | 本次 | PASS；4 passed（含发布子 owner 的代理注册复用） | host |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy `-D warnings`、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox cargo check -p prima-user --features linux-probe --bin prima-linux` | 本次 | PASS | host/编译 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -p xtask --features linux-probe -- test-linux` | 本次 | PASS；参考 11 项；Prima 两轮各 11 项，含 `M4-fork`，stdout/stderr/status/审计均相符 | QEMU（既有套件） |
| `python3 scripts/docs.py self-test` / `build` / `check` | 本次 | PASS | 文档 |
| `check --base 206ec5b` | 本次 | PASS | 文档 |

## 失败、反例与限制

- `Local` 的序号改为按 owner 保存，root owner 与 fork 子 owner 都从 1 开始；子
  `Session` 若沿用父序号会被拒绝。该错误在测试初跑时暴露（`-EINVAL`），按真实
  条目序号修正后通过。
- fork 成功后 `publish` 失败会使内核子 Task 已存在而 Store 无 owner；当前路径
  释放预留并返回错误。`Local` 中该路径不可达；真实 transport 必须保证“预留成功
  且子形成后发布不可失败”，或在 P2b-4 安排子任务销毁。该限制保留。
- 代理服务中的新消息处理只做编译与既有回归验证；没有运行真实代理 QEMU 路径，
  伪造 slot、旧 incarnation、死亡代理与单侧退出属 P2b-5。
- 新鲜上下文复核发现并已修复：
  - `reserve_external` 复用已发布子 owner 后，`PROXY_ADD` 失败分支会释放该 live
    owner 而不关闭 OFD；现在记录 `reused`，复用槽位在所有失败/回送失败路径都不释放。
  - exec 释放段持有位时用家族槽位而不是 `process.owner`；fork 后两者不同，
    现统一用 `process.owner`。
  - RESERVE 对无效 slot 回复 `EACCES` 会被客户端当作非零 key；现按契约回复 0，
    PUBLISH/RELEASE 统一 0/1。
  - `Local::release` 对已发布 key 静默成功、`publish` 非幂等，与服务语义不一致；
    现释放已发布 key 返回 `EACCES`，同子重发布幂等、异子拒绝。
  - exit 关闭失败注释原称“可重试”，但调用方当前按致命错误处理；注释已按实际行为修正。
- 域的 fork 失败回滚在 host 上以 `Session::reserve → release` 序列验证；真实
  `gate_fork` 失败路径依赖 QEMU。
- exec 段持有位与代理失败分支复用两项修正没有直接 host 测试（分别需要内核段权限
  与真实代理运行时），由编译、既有 QEMU 回归与代码路径复核验证。
- 多线程 fork、完整 execve、跨 owner 的配额与耗尽隔离不在本片。
- `Local` 仍是进程内 transport，QEMU 字节来自本地 fixture；管理器签发与
  `/linux`、`/native`、`/shared` 启动策略执行属 P2b-4。

## 阶段/profile 影响

不改变阶段 7 九轨勾选，不关闭 P2，不构成 7-A 证据，不改变 profile v1/v2 或
D12 白名单。既有 Linux 差分事件与回执未修改。

## 后继关系

- 计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)。
- 下一片：P2b-4 镜像与启动 fixture 接线（真实代理会话、`PROXY_*` 消息客户端、
  两 ABI 程序启动）。
- 之后 P2b-5 代理路径 QEMU 正反例矩阵、P2b-6 收口。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P2b-3 节。
