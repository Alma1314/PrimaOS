---
id: report-stage7-proxy-session
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-16
---

# P2a：管理器签发代理会话与受限根视图

## 范围与实际状态

基线4830638，初始工作树干净。[P2计划](../plans/2026-11-10-libos-data-path.md)把
“双personality共同文件服务”拆为两段；本报告只记录P2a：管理器签发的代理会话、
Store侧绑定/授权、根视图原语与代理批量通道。

**Linux personality仍使用域内RAM fixture，数据面没有迁移；启动镜像没有接入Store授权；
代理路径没有QEMU正反例。** 因此P2未完成，7-A及阶段7退出勾选不变。契约见
[工作契约P2a节](../specs/2026-11-10-libos-data-path.md#p2a代理会话与受限根视图2026-11-16)。

## 改动与不变量

- `prima-runtime/src/vfs.rs`新增`View`（根、可选显式`shared`子树）与`resolve_in`/
  `open_in`/`resolve_link_in`/`mkdir_in`/`unlink_in`/`chmod_in`/`symlink_in`/
  `rename_in`。绝对路径从视图根开始，`..`在视图根停留，符号链接的绝对目标重新从视图根
  开始，相对起点必须在视图内，共享子树根的`..`返回视图根。既有`View::ROOT`包装保持
  直接客户端的原语义。`OpenFiles::open_in`复用同一检查。
- `prima-user/src/proxy.rs`新增有界代理表：绑定`(proxy cookie, app Task cookie,
  Process, OFD owner, 根策略)`，序列从1严格递增；拒绝零cookie、legacy owner槽、重复
  登记、重放/跳号，槽位可复用。`prepare_policy`按Store固定策略懒创建`/linux`、
  `/native`、`/shared`及Linux既有fixture；只在新创建时写入，根模式临时放开后恢复。
- `prima-runtime/src/protocol.rs`新增`PROXY_REGISTER`/`PROXY_ADD`/`PROXY_CLOSE`/
  `PROXY_SELECT`与根策略常量；`bulk.rs`新增`PROXY_OPEN`/`PROXY_SUBMIT`/
  `PROXY_CLOSE`及槽位/序列编码。
- `prima-user/src/servicemgr.rs`只接受已登记文件客户端的`PROXY_REGISTER`：应用Task
  的`TaskSend`必须由该客户端实际持有，回复目标必须等于发送者，应用祖系经内核
  `task_parent_cookie`验证，`SERVICE_ROOT_SERVICE`拒绝；随后把`PROXY_ADD`转发给Store。
- `prima-user/src/service.rs`只接受管理器来源的`PROXY_ADD`，再次用内核元数据验证代理、
  应用、Process与祖系；分配独立owner并登记策略。`PROXY_SELECT`绑定后续路径操作的视图
  与owner；`PROXY_OPEN/SUBMIT/CLOSE`使用独立槽位和缓冲页，槽位每次都按登记表校验，
  应用incarnation死亡或代理退出会关闭owner并回收缓冲。请求中的fd、PID、owner、
  Task id与NodeId都不是授权。
- `sessions.rs`新增代理owner分配/释放；代理owner与直接客户端owner分离，释放后不能
  授权旧客户端。

## 检查记录

原始日志在`/tmp`或ignored `target/`，不入库。

| 命令 | 结果与范围 |
|---|---|
| `cargo test --workspace`（`CARGO_NET_OFFLINE=true`） | 退出0；1152 passed、1 ignored；新增7项分别覆盖VFS视图逃逸/共享挂载、代理表伪造/重放/容量、策略fixture幂等及代理owner复用 |
| `cargo xtask lint`（`CARGO_NET_OFFLINE=true`） | 退出0；fmt、workspace/all-targets Clippy、裸机构建、44文件零unsafe与personality边界 |
| `python3 prima-user/musl/test_port.py` | 退出0；7 tests；C适配层未改动，属既有回归 |
| `cargo xtask test`（`CARGO_NET_OFFLINE=true`） | 退出0；36组72轮BIOS通过；**该套件不注册代理，只证明既有路径未因VFS重构回归**，不是P2a数据面执行证据 |

## 失败、限制与未完成项

- 代理签发、Store绑定与代理批量只在host库测试及编译层面验证；管理器/Store的实际IPC
  路径没有QEMU执行证据。不得把普通72轮PASS或host表测试当作双personality共享完成。
- Linux数据面未迁移：open/stat/read/write/getdents/lseek/close仍由域内`ram`+`OpenFiles`
  处理；fork预留/发布/回滚/退出关闭仍调用Linux本地表，**没有迁移到Store owner**。
  P2b必须先解决fd→Store句柄的游标、append、目录流和错误映射，再接线。
- 代理选择在Store内按发送者保持，路径暂存仍按直接客户端槽位保存；当前设计假设单个
  代理同步串行提交。P3异步化前必须把选择与暂存绑定到具体batch，不能依赖该假设。
- 根策略fixture当前编译在Store内，`shared`挂载名固定；Linux fixture来源与镜像启动
  输入尚未冻结为可审计数据。策略0只能由直接客户端使用，代理不可选。
- `shared`只在视图根的第一段做挂载替换；若会话根下存在同名真实目录，路径解析会看到
  挂载子树，而按父目录 lookup 的 unlink 仍可能作用于真实目录。当前fixture不含同名项，
  但P2b冻结fixture时必须禁止或显式处理该遮蔽。
- 未验证的异常窗口：代理登记后、客户端取走ACK前代理死亡；SELECT后应用死亡；Store
  `close_owner`持续I/O失败。代码保留登记/缓冲重试，但没有机器故障注入证据。
- 未做SMP、撤销、恢复、凭据转移、审计配额或性能测量；本次不新增syscall或内核语义。
- 首轮`cargo xtask lint`因VFS `unwrap`和清理循环索引写法被`-D warnings`拒绝，改为
  `if let`与`iter_mut`后重跑通过；这不是行为失败，但保留为工具门记录。
