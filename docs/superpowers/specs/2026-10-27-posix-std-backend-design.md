---
id: spec-posix-std-backend
kind: specification
authority: scoped-implementation-contract
status: working-contract
lifecycle: current
updated: 2026-10-27
---

# Prima POSIX std 后端

基线 `be3d6dd`。用户在本任务明确要求完整 POSIX std 后端，范围从
[D6 的 A 子集](2026-10-26-rust-d6-scope-decision.md)扩到 B；保留原 A 的验证记录。
上位是[静态程序契约](2026-10-26-static-program-contract-design.md)及
[现有工作 API 台账](../compat/stage5-working-api.md)。本契约不发布或冻结新的 POSIX profile，
不代替组织级阶段签署，未修改 Linux ELF 的阶段6路线不因此启动。

## 调用链和边界

新增 `x86_64-prima-posix`：vendor=prima、os=prima、family=unix、env=musl。
新增目标使用与musl一致的x86_64 SSE/SSE2浮点ABI（禁用AVX），不能继承原生softfloat C调用假设。
保留原 `x86_64-prima` 原生目标、R1/R2及其限制。
完整 std 的 Unix 模块复用固定 nightly 的源码；在隔离副本显式增加 Prima 的
Linux/musl **用户态 ABI** 选择。编译器不注入 Linux target cfg，不修改安装源或 registry。
std/libc 全树输入 SHA-256 和编译器版本锁定，输出记录实际 ABI 选择文件及覆盖层摘要。

调用链为 Rust 标准 main/lang_start → Prima crt / musl 初始化 → std Unix模块 →
锁定的 musl libc → prima_syscall 适配 → 原生能力/用户态服务。
程序声明为 POSIX_STATIC / adapter 1；不得沿用原生 persona 0 声明。
默认 System allocator 使用 musl malloc/free，线程通过 pthread 和按线程 pthread-key TLS，
不使用不受 loader 支持的 ELF 原生 TLS。所有分配/文件/网络的失败按 errno 传播。

std 的文件、环境、网络、进程、线程、同步、stdio、时钟模块必须可编译链接，
不能只因可链接把上层所有 API 登记为支持。机器覆盖以实际执行清单为准。
底层文件树容量、96页匿名内存、单 CPU、UDP loopback、无 TCP/DNS/pipe/poll/持久化、
无 wall-clock 或安全熵的限制继承现有台账。
不伪造 TCP、Command、SystemTime 或随机数据成功；错误或既有panic边界必须明确记录。
HashMap仅使用上游无熵平台fallback，不承诺抗HashDoS。
栈溢出诊断沿用通用无handler实现，不假装支持sigaltstack。

## 验证要求

新增 R3 并纳入完整 BIOS 双轮，保留30秒门限、精确stdout和资源/退出检查。
覆盖标准入口/args/env/cwd、默认allocator、文件写读/seek/metadata/目录/rename/unlink及ENOENT、
UDP收发、pthread创建/join、TLS隔离/析构、Mutex/Condvar/park_timeout、Instant/sleep、进程ID/退出。
R3还应验证大于arena累计预算的反复分配/回收，args空串/UTF-8和初始环境；
fsync的ENOSYS、Command::status及TCP连接失败不伪装为支持。
UDP接受MSG_NOSIGNAL但仍拒绝其它未登记flags；host与机器分别取证。
扩展时增加同源host参考、错误路径及资源清理证据。R1/R2继续单独回归。

构建入口为 `python3 scripts/build_posix_std.py [--clippy]`；
`--manifest-path <Cargo.toml>` 可构建其它标准main程序，复用相同crt、适配层及System allocator，
无需复制探针入口或自带GlobalAlloc。始终强制panic=abort、静态链接；依赖须已缓存且lock存在。
C对象、libc.a及链接脚本内容纳入Cargo指纹，路径不变不能复用旧ELF。
保留调用者encoded flags（包括显式空值，优先于RUSTFLAGS），否则解析RUSTFLAGS引号；
后端必需参数后追加，panic=abort不能被调用者的unwind静默覆盖。
`python3 scripts/test_posix_std.py`提供参数边界host回归。

运行 workspace tests、lint、R1/R2/R3、完整BIOS、Clippy、compat、musl及文档四项检查。
实跑结论与编译/运行失败保留在[后端报告](../reports/2026-10-27-posix-std-backend-report.md)。
更换nightly、libc版本、ABI选择、TLS、allocator或入口时重新审查并重跑。


## 后继：文件长度与显式偏移I/O

2026-10-27继续实现底层文件扩展，std::fs::File::set_len及Unix FileExt::{read_at,write_at}
走musl ftruncate/pread64/pwrite64（Linux用户ABI号77/17/18），不以seek/read/restore模拟。
Store认证及OFD holder检查沿用当前机制；协议file op6=RESIZE、7=READ_AT、8=WRITE_AT。
RESIZE使用完整u32长度；更大的长度饱和到u32::MAX后必须ENOSPC，不能截断到可成功的小值。
AT参数offset[0..12]、count[12..14]、reserved[14..16]=0、data[16..32]；单请求最多2B。
所有大于4095的正偏移编码到4095 EOF哨兵；编译断言RAM内容区严格小于4095，扩容时必须修订协议。
read无data，保留位及count>2拒绝。错误或短传输保留此前字节数，多RPC调用不承诺整体原子性。

操作按打开时授予的access检查（包括空I/O），不因后续chmod撤回已打开权限；
dup/fork共享游标不变；长度增长清零，不泄漏截断前数据；unlink后存活OFD仍可操作，最后close回收。
WRITE_AT按POSIX忽略APPEND，与Linux在APPEND上的pwrite行为有明确差异，不能用host相同输出掩盖。
现有节点级vfs::truncate保留旧权限及只缩小契约。fsync仍ENOSYS，不伪造持久化。
V4及扩充R3纳入完整双轮，验收记录见[后继报告](../reports/2026-10-27-posix-std-file-io-report.md)。

## 后继：I1接收路径的验收修订

初始完整命令69/70的I1失败原样保留。SYS_RECV(_META)的阻塞可由用户SEND、
内核事件（包括UART IRQ4）、信号中断、Task回收完成，或者在快照时仍未返回。
新增IPCRECV核算全部非SEND出口，并按内核来源/IRQ4源签名识别UART子集。
每轮必须严格满足blocks=direct+kernel+interrupted+retired+faults+live，禁止faults或计数溢出。
跨轮用户queued+direct总数、非UART内核完成数、interrupted/retired/live均须相同；
UART的物理到达/合并和user queued/direct分布无需固定。缺少或重复归账记录/字段、未知字段均拒绝。
此处显式取代I1原先跨轮blocks相等的验收假设；N1与其余程序原断言保留。
功能、能力转交、精确stdout、退出/资源/双轮及30秒门限保持，分别保存I1-run1/I1-run2原始记录。
