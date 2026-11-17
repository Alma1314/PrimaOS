---
id: report-p2b-proxy-wiring
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-18
---

# P2b-4：镜像接线、管理器签发会话与两 ABI 场景

## 结论与范围

本报告记录 P2b-4：把 Linux personality 与原生/POSIX 应用装入同一个启动镜像，
由服务管理器在 Store 就绪后创建二者、签发文件会话与代理会话；Linux 前端改为真实
`Remote` 传输，原生应用使用直接文件会话；新增 `cargo xtask test-linux-proxy` 专用
QEMU 场景并取得机器证据。**代理 fork 子注册、正反例矩阵、共享资源授权与故障注入属
P2b-5；P2 未关闭，7-A 未勾选。**

## 基线与输入

- 基线 `fea5208`（P2b-3 已提交，工作树干净）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) 新增 P2b-4 节；
  计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)
  的 P2b-4 节。
- QEMU 输入沿用 pinned `PRIMA_LINUX_ELF=/usr/bin/busybox`（v1.36.1、静态，
  sha256 `dbac288c…4e62d6ac14`）。

## 实现内容

- `prima-runtime/src/protocol.rs`：新增 manifest kind `SERVICE_IMAGE_KIND_LINUX`
  (`0x105`)、`SERVICE_IMAGE_KIND_NATIVE_APP` (`0x106`)，场景运行期名
  `"lxsc"`/`"nasc"`，以及客户端完成消息 `SERVICE_PROXY_DONE_TAG`。
- `prima-user/src/linux_store.rs`：
  - `Store` 新增 `register_child(owner, task)`（默认 no-op）；`publish`/`release`
    增加 acting owner 参数。
  - `Remote`：管理器签发的代理传输。`open_session`/`accept_store` 只接受目标等于
    发送者的会话 ACK；`register_proxy` 用应用 TaskSend 作为唯一主体凭据；
    `open_slot` 为槽位映射共享页（固定 VA）；`exchange` 经 `PROXY_SUBMIT` 提交并把
    完成后的页复制回本地缓冲；`reserve`/`publish`/`release` 走 `PROXY_*`；
    `close_owner` 走 `PROXY_CLOSE`。提交前先写共享页、完成后读回。
  - `Direct`：原生/直接会话客户端（会话注册 + 一页批量 OPEN/READ/CLOSE）。
- `prima-user/src/linux_process.rs`：fork 在 publish 之后为该子 Process 注册自己的
  代理条目（`register_child`）；`Local` 下仍是 no-op。
- `prima-user/src/linux_main.rs`：管理器以 `"lxsc"` 名创建时进入代理场景：
  `Remote` 连接、根代理注册、`cat /fixture.txt` 单用例、阶段见证
  `S7LXP|phase=…`，完成后向管理器报告并阻塞在 `receive`（避免自旋饿死其它 Ready
  任务）。
- `prima-user/src/native_probe.rs`（新二进制 `prima-native-scenario`）：以直接文件
  会话读取 Store 根 `/README`，输出 `S7NA|…` 见证并向管理器报告。
- `prima-user/src/servicemgr.rs`：
  - 从映像目录识别 `LINUX`/`NATIVE_APP` kind；含 Linux 条目即进入场景模式，历史镜像
    不含该条目时所有新增路径保持惰性。
  - 在 Store 处理完段与三条探针后创建两个客户端；按任务 id 授权其文件会话注册。
  - 既有 `PROXY_REGISTER` 校验扩展到场景客户端；Store 再次用内核元数据校验。
  - 场景端点使用 64 页栈、基址 `0x0B00_0060_0000`（避开 `SERVER_VA` 与映像跨度），
    Linux personality 的根同时具备 `Derive|NsResolve` 以便创建 guest Task。
  - 收齐两侧 `SERVICE_PROXY_DONE_TAG` 后写 `S7ABI|…` 并以 0x10/0x11 退出 QEMU。
- `prima-user/src/service.rs`：
  - 代理 `PROXY_PUBLISH` 的 ACK 在 bits 8..15 返回已发布 owner，客户端据此给子
    Process 建立会话。
  - `PROXY_CLOSE` 对**缺失条目**返回 0（已由死亡回收清理），对**他人条目**仍回
    `EACCES`；这修正了“应用先退出、Store 先回收条目”的正常顺序。
- `xtask/src/linux_proxy.rs`：`test-linux-proxy` 打包 init（initgen）+ smoke +
  log/ns/store + linux + native + servicemgr，运行 QEMU 并断言
  `S7LX|read=1;proxy=1;status=0`、cat 的十六进制 stdout、`S7NA|…read=13…` 与
  `S7ABI|linux=1;native=1;proxy=1;status=0`。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 本次 | PASS；1169 passed、0 failed、1 ignored | host |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy `-D warnings`、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -p xtask --features linux-probe -- test-linux-proxy` | 本次 | PASS；`S7PROXY abi=2 linux=cat native=/README proxy=1 shared_store=1` | QEMU（新场景） |
| 同上 `test-linux`（既有差分） | 本次 | 第二次完整 PASS；22 轮（含 M4-fork），final `PASS S6LINUX round=2 pinned=…` | QEMU（既有套件） |
| `cargo xtask build` | 本次 | PASS（默认镜像；场景条目只在 linux-probe 镜像中） | host/镜像 |
| `python3 scripts/docs.py self-test` / `build` / `check` | 本次 | PASS | 文档 |
| `python3 scripts/docs.py check --base fea5208` | 本次 | PASS | 文档 |

## 失败、反例与限制

- 首次既有差分运行中 `M3` 第 1 轮在 30 秒 QEMU 阈值下超时；未改代码立即重跑，
  22/22 轮全部通过、日志无 `ERROR`。该次超时保留为 TCG/宿主负载波动记录，不当作
  代码失败，也不掩盖。
- 场景镜像的栈容量提升是显式登记项：管理器创建的场景端点使用 64 页栈与
  `0x0B00_0060_0000` 基址。原因：Linux personality 的复合请求帧约 87 KiB，且其
  ELF 约 3.2 MiB，与 `ENDPOINT_STACK_VA`（1 MiB）和 `SERVER_VA`（4 MiB）重叠会
  使栈或代理页映射失败。历史镜像的 1/2/4/8 页预算不变。
- Linux personality 在场景镜像中获得 `Derive|NsResolve` 根权限（它按同一入口 ABI
  创建 guest Task）；其余场景端点仍只有 `NsResolve`。
- 代理 fork 子注册路径（`Family::fork` → `register_child` → 新 `PROXY_ADD`）已实现，
  但本场景只运行 `cat`，未在 QEMU 中触发 fork；`M4-fork` 差分仍使用进程内 `Local`。
  该路径留待 P2b-5。
- 两个 ABI 读取的是**不同**资源：Linux 代理视图 `/linux/fixture.txt`，原生直接会话
  读取服务根 `/README`。同一 `/shared` 资源的显式授权正例属 P2b-5。
- 场景只覆盖“应用先退出、Store 回收条目、代理随后 `PROXY_CLOSE` 幂等成功”的顺
  序；伪造槽位、旧 incarnation、死亡代理与单侧退出属 P2b-5。
- `PROXY_PUBLISH` 之后若 `register_child` 失败，内核子 Task 已存在而代理条目缺失；
  当前返回错误并保留窗口记录（与 P2b-3 的 publish 窗口同类）。
- 管理器场景模式由 manifest 推断；历史镜像不含 Linux 条目，72 轮套件不在本次
  实跑范围（P2b-4 不改变既有事件/回执，但完整套件属 P2b-5 的停止条件）。
- 文件会话注册仍受 `MAX_CLIENTS=18` 的 Task id 上限约束；场景任务 id 实测 14/15，
  未接近上限，但没有显式预留。
- 未做 SMP、配额、撤销/恢复、性能测量；不新增 syscall 或内核语义。

## 阶段/profile 影响

不改变阶段 7 九轨勾选，不关闭 P2，不构成 7-A 证据，不改变 profile v1/v2 或 D12
白名单。既有 Linux 差分事件与回执未修改。

## 后继关系

- 计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)。
- 下一片：P2b-5 代理路径 QEMU 正反例矩阵（fork 子注册、`/shared` 显式授权、伪造
  owner/旧 incarnation/根逃逸、单侧退出）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P2b-4 节。
