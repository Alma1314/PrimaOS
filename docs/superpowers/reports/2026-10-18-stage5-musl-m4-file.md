# 阶段 5 / M4：文件与 fd 生命周期（首切片）

**日期**：2026-10-18。**性质**：实施与 QEMU 证据报告；不是 profile 升级承诺。
本切片把 `openat/read/write/lseek/close` 的冻结子集接到真实字节存储与用户态 fd
生命周期，并补上 M3 遗留的 `printf`+`fflush` 输出路径验收。

## 1. 实现

### 1.1 线上协议

`prima-runtime/src/protocol.rs` 追加：

- `SERVICE_FILE_REQUEST_TAG=0x5ED4...`、`SERVICE_FILE_REPLY_TAG=0x5ED5...`；
- 请求布局 `tag(48..63) | op(44..47) | fd(40..43) | client(32..39) | arg(0..31)`；
- 应答布局 `tag | op | fd | status(32..39) | data(0..31)`，`status` 为 0 或正 errno；
- `open` 的 `arg` 为归一化 flags(28..31) + 路径长度(26..27) + 路径字节(0..23)；
- `write` 的 `arg` 为 `count(24..31) | data(0..23)`；`lseek` 为
  `whence(30..31) | offset(0..29 二补码)`；
- 单条消息最多携带 `SERVICE_FILE_CHUNK=3` 字节，由适配层分片。

### 1.2 存储服务（`prima-user/src/service.rs`）

存储服务复用 SPEC-112 的持久化段（1 页）：

| 偏移 | 用途 |
|---|---|
| 0..8 | SPEC-112 的 `u64` 状态（`ADVANCE`/`QUERY` 语义不变） |
| 8..16 | 文件长度（LE u64） |
| 16..4096 | 文件内容（容量 4080 字节） |

- 唯一文件名 `f`（单层、≤3 字节、无 `/`）；`O_CREAT` 无动作（文件始终存在），
  `O_TRUNC` 清空长度。
- 句柄表 4 项（fd 3..6），每项保存 `open/offset/access`；offset 归服务端所有，
  每次 `open` 从 0 开始。`close` 立即释放并可复用。
- `read`/`write` 每次最多 3 字节，返回实际字节数；跨片由适配层聚合。
- 句柄耗尽返回 `EAGAIN`；内容区用尽返回 `ENOSPC`；非法 whence/负偏移返回 `EINVAL`。

### 1.3 服务管理器（`prima-user/src/servicemgr.rs`）

- C 任务创建后，把 `Derive|TaskSend` 令牌绑定到 `isolated_ns` 的路径 `f`，并从
  C 的 manager cap 派生一条「管理器→C」的 `TaskSend` 用于回送应答。
  **绑定必须在 C 创建之后**：创建前多占一个 cap 会让 `live + 3 + initial`
  越过旧上限 144，`derive_capability_to_holder` 返回 `CapacityExceeded`。
- `open` 请求先对 `isolated_ns` 做 `SYS_NS_RESOLVE`（请求 `TaskSend` 子集）；
  解析成功才转发存储服务，失败直接回 `ENOENT`。解析出的 cap 立即归还。
- 其余操作只按同步请求记录（`file_client`）转发；管理器不解析文件内容或目录，
  不是 VFS。
- 存储服务应答按 client id 回送 C；无等待者时丢弃。

### 1.4 C 适配层（`prima-user/c/prima_syscall.c`、`prima_abi.h`）

- fd 3..6 本地表；`openat(257)`、`read`、`lseek`、`write(fd>=3)`、`close(fd>=3)`
  翻译为文件请求。
- 同步调用：`SEND` 后直接 `RECV`（不显式 `YIELD`——`RECV` 空队列时会阻塞让出；
  显式 yield 曾与抢占窗口的任务状态迁移竞争）。
- `openat` 只接受 `AT_FDCWD`、单层 ≤3 字节路径、access/CREAT/TRUNC；
  `O_LARGEFILE`/`O_CLOEXEC` 接受但无语义效果（单线程、无 exec）；其他 flag 返回
  `EINVAL`。
- `write`/`read` 对调用者透明地分片；中途失败返回已完成的字节数。

### 1.5 C 探针（`prima-user/c/stage5_probe.c`）

Linux 与 Prima 共用同一 `file_api_probe`：

1. `openat(AT_FDCWD,"f",O_RDWR|O_CREAT|O_TRUNC,0600)`；
2. 写 `abcdef`，`lseek(SET,0)`，读 4 字节得 `abcd`；
3. `lseek(END,-2)` 得 4，读 2 字节得 `ef`，再读 1 字节得 EOF；
4. 在偏移 6 写 `g`，回到 0 读 7 字节得 `abcdefg`；
5. `close` 后 read/write/close 均 `EBADF`；不存在路径 `ENOENT`；
6. 关闭后立即重开（句柄复用）、只读句柄写 `EBADF`、负偏移 `EINVAL`、
   坏缓冲区 `EFAULT`、重开后 offset 从 0 开始。

Linux 参考在 `target/prima-os/linux-probe-cwd` 下运行，避免污染仓库。文件失败码
编码为 `40 + reason`；通过后执行 `printf("M4STDIO\n") + fflush(stdout)`。

## 2. 内核容量（照实扩容并重测）

| 常量 | 变化 | 依据 |
|---|---|---|
| `MAX_SEGMENT_FRAMES` | 96 → 128 | 服务映像段含目录页实测 97 页（C 探针 12→14 页） |
| `MAX_VSPACE_ENTRIES` | 256 → 320 | 映像逐页计入 init 逻辑映射，C 运行期峰值 |
| `MAX_CAPABILITY_SLOTS` | 144 → 160 | 文件授权令牌 + C 创建后 bind 的 cap 峰值；实测峰值 141 |
| `MAX_AUDIT_EVENTS` | 768 → 2048 | 文件 IPC 的调度迁移与授权审计；实测 `used=1268` |

`MAX_FRAME_OWNERS` 保持 256；`MAX_SEGMENTS`/`MAX_MESSAGES` 不变。
`BootstrapState ≤ 13 KiB` 守卫测试照实重跑通过。

## 3. 证据（2026-10-18）

```
cargo test --workspace    15 组 test result: ok（含 200 个 xtask 测试）
cargo xtask lint          fmt / clippy -D warnings / 裸机 deny(unsafe_code) / 零 unsafe 全 PASS
cargo xtask test          8 项全 PASS（BIOS 双轮 QEMU）
```

串口（手动捕获）：

```
CIO|task=17;fd=1;ch=104;fd=2;ch=105
TTYIN|task=17;fd=1;ch=104;fd=2;ch=105
TTYOUT|task=17;in_magic=1;out_of_range=1;out_ok=1
xyz
M4STDIO
TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132
CAPACITY|caps_live=132;caps_peak=141;caps_limit=160;segment_slots=41;segment_limit=48;messages_queued=1;messages_peak=14;messages_limit=32;audit_used=1268;audit_limit=2048
state_audit_ready total=557;to_running=201;to_ready=189;to_blocked=162;to_faulted=5;used=1268;cap=2048
```

`EVENT_STAGE5_CAPACITY` trace 仍为 `[17,0x5a,37,1,1]`，即 C 探针退出码 37。
Linux 参考：`exit=37`，stdout `h\nxyz\nM4STDIO\n`，stderr `i`，文件 `f` 7 字节。

## 4. 既有事件变化（逐条登记）

| 事件/字段 | 旧 | 新 | 原因 |
|---|---|---|---|
| `ipc_ready` direct/blocks | 56/64 | 152/160 | 文件请求的阻塞与直投往返 |
| `task_ready` caps/caps_after | 131 | 132 | Namespace 文件授权令牌 |
| `namespace_ready` entries/binds/resolves | 2/3/3 | 3/4/5 | 绑 `f` 一次、两次 open 授权解析 |
| `TASKCAP` caps | 131 | 132 | 同上 |
| `CAPACITY` caps_limit/peak/live | 144/137/131 | 160/141/132 | cap 槽扩容与令牌 |
| `CAPACITY` audit | 757/768 | 1268/2048 | 文件 IPC 与授权审计 |
| `state_audit_ready` used/cap | 757/768 | 1268/2048 | 同上 |
| `frames_ready` image/free | 1158/28564 | 1201/28225 | 内核镜像与 ramdisk 增长 |
| `self_root_ready` pages | 423 | 423 | 不变 |

以上均为真实计数变化，不是断言放宽；xtask 校验同步更新。

## 5. 边界（不得据此宣称通用文件系统）

- 仅一个固定文件 `f`；无目录、目录枚举、多级路径、`openat` 目录 fd。
- 路径长度 ≤3 字节、无 `/`；其他名字返回 `ENOENT`。
- 每任务 4 个句柄（fd 3..6）；无 `dup`、无 `CLOEXEC` 语义、无跨句柄共享 offset。
- `O_APPEND`/`O_EXCL`/`O_NOFOLLOW`/`O_DIRECTORY` 不支持；非法 flag 返回 `EINVAL`。
- 单条 IPC 3 字节，适配层分片；无 io_uring/异步/非阻塞语义。
- 无进程退出自动关闭（没有 Process 退出观察）；探针显式 `close`。
- 内容驻留运行期内核内存段，不是持久文件系统；服务/Task 生命周期外无保证。
- 管理器只做授权与路由；授权令牌是管理器发送凭据的派生（带 `Derive`），
  Namespace 是入口而非文件树。
- 未验证 musl 的 `fopen/fread/fwrite/fclose`；stdio 只验收 `printf+fflush→writev`。
- Linux 差分不覆盖非法 flags（两端错误码不同：Linux 接受 `O_ACCMODE==3`，
  对 `O_DIRECTORY` 返回 `ENOTDIR`）。
- 失败时会话可能留下服务端句柄（若调用方未 close）；探针覆盖正常 close 与句柄复用，
  但没有客户端崩溃回收。
