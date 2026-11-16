---
id: report-p2b-store-batch-ops
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: historical
updated: 2026-11-17
---

# P2b-0/P2b-1：Linux Store 批量数据面

## 结论与范围

本报告记录 P2b 的前两片：P2b-0 冻结 Linux 数据面 op 矩阵与线描述符，
P2b-1 在 Store 侧实现 `OPEN`、`STAT`、`CLOSE`、`GETDENTS`、`LSEEK`、
`STAT_PATH` 与 proxy `EMFILE` 映射。证据只到 host 测试、workspace 与 lint；
**Linux 前端尚未切换（P2b-2）、fork/exit 未迁移（P2b-3）、镜像未接线、
新 ops 没有 QEMU 执行，P2 未关闭，7-A 未勾选。**

## 基线与输入

- 基线 `5a163e9`（P2a 已提交）；本片改动在工作树中，未提交。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) 的 P2b-0 节；
  计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)。
- 工具链按 `rust-toolchain.toml`；命令使用 `CARGO_NET_OFFLINE=true`。
- 宿主/QEMU/guest 身份：本片未运行 QEMU，故无 guest 身份可记。

## 实现内容

- `docs/superpowers/specs/2026-11-10-libos-data-path.md`：新增 P2b-0 节，冻结
  op 矩阵、描述符字段、wire fd、中性 flags/stat、错误映射、容量、并发、fixture
  字节与哈希、保留挂载名和验收边界。
- `prima-runtime/src/bulk.rs`：新增 op 编码 3/4/5/6/9/10、中性常量与描述符形态校验；
  `STAT_PATH` 按 80 B 窗口参与范围/重叠检查；现有 READ/WRITE/READ_AT/WRITE_AT
  行为保持不变。
- `prima-runtime/src/protocol.rs`：新增 `SERVICE_FILE_EMFILE=24`。
- `prima-user/src/bulk.rs`：`execute` 增加 `View` 参数并实现新 ops；`OPEN` 返回
  wire fd；`GETDENTS` 只返回完整记录并用 `SEEK` 回退半条记录；`STAT`/`STAT_PATH`
  输出 16 B 中性记录；`LSEEK` 结果放描述符 `result:u32`。
- `prima-user/src/service.rs`：proxy batch 的视图来自登记表 `entry.policy`，
  不依赖 `PROXY_SELECT`；proxy 错误映射把 `TooManyOpen` 映射为 `EMFILE`，
  直接客户端路径保持既有 `EAGAIN`。
- 新增 host 测试：OPEN/STAT/LSEEK/READ/CLOSE 往返、GETDENTS 整记录与回退、
  STAT_PATH 视图与固定输出窗口、OPEN 视图根不可逃逸、proxy EMFILE。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test -p prima-runtime bulk` | 本次 | PASS；2 passed（批量校验 + 新 op 形态） | host |
| `cargo test -p prima-user bulk` | 本次 | PASS；7 passed（含 5 项新 op 测试） | host |
| `cargo test --workspace` | 本次 | PASS；1158 passed、0 failed | host |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy `-D warnings`、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `python3 scripts/docs.py self-test` / `build` / `check` | 本次 | PASS；402 文档、3820 节 | 文档 |
| `check --base 5a163e9` | 本次 | PASS | 文档 |
| `cargo xtask test`（QEMU） | 未运行 | 本片不产生 QEMU 证据；新 ops 未接线到 Linux 前端 | QEMU 未覆盖 |

## 失败、反例与限制

- 首轮 clippy 因两个测试的 `needless_update` 被 `-D warnings` 拒绝；删除无效果
  的 `..Request::default()` 后重跑通过，属工具门记录，不是行为失败。
- 一个测试初版把 `/../file` 长度写成 9、把错误状态统一映射为 9；修正为 8 和
  显式映射后通过，不改变实现。
- `OPEN` 单独批次的限制：同一 batch 的后继描述符不能使用 `OPEN` 的结果；前端
  必须先提交 OPEN 再提交数据 op。该限制已写入契约。
- Store OFD 表全局 8 项；proxy `OPEN` 满时返回 `EMFILE`，直接客户端仍为 `EAGAIN`。
  本片没有修改容量，也没有每应用预留。
- `GETDENTS` 使用 3808 B 栈缓冲读取后裁剪整记录；半条记录用 `SEEK` 回退。
  没有验证超大目录、不可写目录或并发目录流。
- 视图绑定只覆盖 proxy batch；`PROXY_SELECT` 的路径原语、跨代理并发和 P3 的
  batch 绑定未改变。一个代理服务同时服务多个应用时仍须串行提交。
- 未运行 QEMU、未接入管理器/Store 的真实 IPC 路径、未迁移 Linux 前端的
  open/read/write/stat/getdents/lseek/close。host 测试只证明 Store 操作与线
  编解码，不证明双 personality 数据面。

## 阶段/profile 影响

本片不改变阶段 7 的九轨勾选，不关闭 P2，不构成 7-A 证据，不改变 profile v1/v2
或 D12 白名单。P2b-2～P2b-6 与 holder 模型切片仍按计划推进。

## 后继关系

- 计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)。
- 下一片：P2b-2 Linux 前端切换到代理会话（将消耗 `OPEN`/`STAT`/`GETDENTS` 等
  wire op）；之后 P2b-3 fork/exit、P2b-4 镜像、P2b-5 QEMU 正反例。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P2b-0 节。
