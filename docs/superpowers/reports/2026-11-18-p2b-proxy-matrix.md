---
id: report-p2b-proxy-matrix
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-18
---

# P2b-5：代理路径正反例矩阵

## 结论与范围

本报告记录 P2b-5：把 P2b-4 的单一 `cat` 场景扩成正反例矩阵，并在真实代理路径上
运行 fork 子注册与 exec。矩阵分两次 QEMU 启动完成：Linux 侧覆盖私有读、代理
fork 子注册、`/shared` 显式授权与 fork 限额；原生/POSIX 侧改用原生策略视图，覆盖
共享正例、根逃逸、猜 fd、序号重放与伪造槽位。**P2 未关闭，7-A 未勾选；既有 72 轮
BIOS 套件未在本片实跑，由 P2b-6 的停止条件承担。**

## 基线与输入

- 基线 `bd5a2ec`（P2b-4 已提交，工作树干净）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) 新增 P2b-5 节，
  并显式修订 P2b-0 fixture 表、P2b-3 `PROXY_PUBLISH` 行与 P2b-4 原生传输段。
- 计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)
  的 P2b-5 节。
- QEMU 输入沿用 pinned `PRIMA_LINUX_ELF=/usr/bin/busybox`（v1.36.1、静态，
  sha256 `dbac288c…4e62d6ac14`）。

## 实现内容

- `prima-runtime/src/protocol.rs`：新增 `SERVICE_IMAGE_KIND_LINUX_MATRIX=0x107`
  与场景名 `SERVICE_NAME_LINUX_MATRIX="lxmx"`。
- `prima-user/src/proxy.rs`：`prepare_policy` 为新策略视图种入
  `/native/README`（`prima rootfs\n`）与两条根外逃逸符号链接 `/shared/escape`
  （绝对目标 `/linux/fixture.txt`）、`/shared/up`（相对目标 `../linux/fixture.txt`）。
  新增 host 测试验证同一链接在服务根指向目标、在两个策略视图中均 `ENOENT`。
- `prima-user/src/linux_store.rs`：
  - `Store::publish` 增加子 Task 的 `TaskSend` 能力；`Remote::publish` 派生并随
    `PROXY_PUBLISH` 发送，`Local` 忽略。
  - `Session::next()`、`Remote::slot()` 与 `Remote::probe()`：把一条原始代理控制
    消息发给 Store 并返回状态字，仅供负例矩阵（重放、伪造槽位）。
  - `Direct` 保留为未选用的遗留直接会话传输；场景不再使用。
- `prima-user/src/native_probe.rs`：改用 `Remote(SERVICE_ROOT_NATIVE)` +
  `Session`，读取 `/README`（witness 字节不变）与 `/shared/note`，随后执行
  `private`、`dotdot`、`symlink`、`symlinkup`、`guessfd`、`replay`、`foreign`
  七条负例并输出 `S7NEG|…` 与 `S7NEGDONE|fail=0`。
- `prima-user/src/service.rs`：
  - `PROXY_OPEN` 在槽位复用时先重试延迟销毁；客户端仍映射派生页时保持 `EAGAIN`。
  - `PROXY_PUBLISH` 以能力目标（完整 cookie）认证子 Task，取代内核父系校验。
  - `PROXY_ADD` 校验管理器转交的主体 `TaskSend` 能力目标等于命名的活 Task。
- `prima-user/src/servicemgr.rs`：
  - `PROXY_REGISTER` 不再沿内核父系遍历，改验发送者授权、主体能力与回复目标，
    并转交主体能力给 Store。
  - 识别 `LINUX_MATRIX` kind；管理器只为场景 Linux 根授予 `Derive|NsResolve`。
  - 首个完成报告输出 `S7HALF|first=…`，两侧报告齐全才写 `S7ABI` 并退出。
- `prima-user/src/linux_main.rs`：`lxsc` 跑 `cat`、`M4-fork`；`lxmx` 跑
  `cat /shared/note`、`fork-limit`；每 case 输出 `S7LXC|case=…;status=…`，
  全部完成后再写 `S7LX`、报告并阻塞。新增失败诊断 `S7LXP|phase=task/forkerr/stray`。
- `prima-user/linux-probes/forklimit.S`（新）：填满 `MAX_PROCESSES` 后第 4 次
  fork 返回 `EAGAIN`，随后三次 `wait4` 成功、`WNOHANG` 返回 `ECHILD`，继承 fd 与
  共享游标继续前进；`build.rs` 与 `linux.rs` 接线。
- `xtask/src/linux_proxy.rs`：同一命令打包并运行两次启动，分别断言上述见证。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 本次 | PASS；1170 passed、0 failed、1 ignored | host |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy `-D warnings`、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -q -p xtask --features linux-probe -- test-linux-proxy` | 本次 | PASS；两次启动各自 `S7ABI|…;status=0`，QEMU 退出码 33 | QEMU（新矩阵） |
| 同上 `test-linux`（既有差分） | 本次 | PASS；两轮各 11 项，含 `M4-fork` 与 `M3`，stdout/stderr/status/审计相符 | QEMU（既有套件） |
| `cargo xtask build` | 本次 | PASS（默认镜像；场景条目只在 linux-probe 镜像中） | host/镜像 |
| `python3 scripts/docs.py self-test` / `build` / `check` | 本次 | PASS | 文档 |
| `python3 scripts/docs.py check --base bd5a2ec` | 本次 | PASS | 文档 |

机器见证摘录（来自 `target/prima-os/S7PROXY-0x105.log` 与 `-0x107.log`）：

- `0x105`：`S6EXIT|case=cat;status=0`、`S7LXC|case=cat;status=0`、
  `S6WRITE|fd=1;hex=4d3420666f726b206f6b0a`、`S6EXIT|case=M4-fork;status=0`、
  `S7LXC|case=M4-fork;status=0`。
- `0x107`：`S6WRITE|fd=1;hex=7368617265640a`、
  `S6EXIT|case=cat-shared;status=0`、`S6WRITE|fd=1;hex=4d34206c696d6974206f6b0a`、
  `S6EXIT|case=fork-limit;status=0`、`S7LXC|case=fork-limit;status=0`。
- 两次启动共有：`S7NA|abi=native;path=/README;hex=7072696d6120726f6f7466730a;read=13;status=0`、
  `S7NPOS|case=shared;hex=7368617265640a;status=0`、七条 `S7NEG|…;status=0`、
  `S7NEGDONE|fail=0`、`S7HALF|first=native`、
  `S7ABI|linux=1;native=1;proxy=1;status=0`。

## 失败、反例与限制

- **Store 父系校验不可满足（已修）**：首次运行 `M4-fork` 时 fork 返回 `EACCES`。
  内核 `gate_fork` 创建的受管子进程 `Process.parent=None`、`forked=false`，
  `task_parent_cookie` 返回 0；P2b-3 的“子 Task 父 Process 等于 `entry.process`”
  在真实路径上恒假，P2b-4 也从未运行 fork。改为 `PROXY_PUBLISH` 携带子 Task 的
  `TaskSend` 能力并由 Store 校验能力目标；`PROXY_REGISTER/ADD` 同步改为管理器
  转交主体能力、Store 校验目标。旧 incarnation 仍由完整 cookie 拒绝。
- **代理页槽位复用 `EAGAIN`（已修）**：第二个 Linux case 注册时 `PROXY_OPEN`
  返回 `EAGAIN`。关闭代理项时客户端仍映射派生页，Store 的 `memseg_destroy`
  被延后且未重试，槽位复用看到残留 root 即拒绝。`PROXY_OPEN` 现在先重试延迟
  销毁；客户端仍映射时保持 `EAGAIN`。
- **Task id 容量（显式限制）**：`MAX_TASKS=18` 且 `task_create_held` 只扫描无记录
  的 id（销毁 Task 保留 Dead 记录），服务与场景客户端占满后每次启动只剩两个
  `create_held` 空闲 id。第三个 guest 根得到 `ENOSPC`。矩阵因此分两次启动；
  未提高 `MAX_TASKS`，以免平移既有 id 与冻结事件。`gate_fork` 走
  `ensure_thread_slot`，可复用无能力引用的 Dead 记录。
- **旧 incarnation 的 QEMU 覆盖缺口**：QEMU 无法强制任务 id 重生；该负例只在
  host 测试 `reused_task_ids_cannot_select_old_sessions` 出声，不当作机器证据。
- **`Direct` 保留但未选用**：原生场景改走策略视图后，镜像不再使用直接会话传输；
  代码保留，避免删除 store 直接路径的唯一客户端。P2b-6 可裁决是否保留。
- **单侧退出见证是顺序事实**：`S7HALF|first=native` 表示原生先报告；管理器在
  两侧都报告前不写 `S7ABI`、不退出。该见证不证明任意调度顺序。
- **未覆盖**：既有 72 轮 BIOS 套件未在本片实跑（P2b-6 停止条件）；SMP、配额、
  撤销/恢复、性能测量、Windows personality 不在本片。7-A 另需独立语义状态/根/
  能力视图证据。
- 场景端点的 64 页栈、`0x0B00_0060_0000` 基址与 Linux 根的 `Derive|NsResolve`
  仍是 P2b-4 登记的容量契约；本片未改。

## 阶段/profile 影响

不改变阶段 7 九轨勾选，不关闭 P2，不构成 7-A 证据，不改变 profile v1/v2 或 D12
白名单。既有 Linux 差分事件与回执未修改；P2b-0 fixture 表与 P2b-3/P2b-4 的两处
语义在契约中显式修订并保留原文指针。

## 后继关系

- 计划：[P2b 与 holder 模型依赖切片计划](../plans/2026-11-16-p2b-holder-model-slices.md)。
- 下一片：P2b-6 P2b 收口（报告汇总、组织级复核输入、`Direct` 去留裁决、72 轮
  BIOS 套件与阶段跟踪更新）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P2b-5 节。
