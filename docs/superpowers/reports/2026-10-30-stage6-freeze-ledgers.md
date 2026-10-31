---
id: report-stage6-freeze-ledgers
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-30
---

# S6-M2 机械项：冻结台账、单一 `auxv` 键源与映像长度窗口

## 结论与范围

在 `298bf55` 基线上推进阶段 6 里**不需要 Gate 接线**的机械项，逐项留下机器可判定的门：

| 项 | 结论 | 依据 |
|---|---|---|
| D17 调用号冻结 | `prima-abi::syscall` 的常量与 `NUMBERS` 台账改由**同一份宏定义**生成，71 项逐号钉死；`52` 保留、`0` 保留、`73..=78` 未实现即不预留 | [`syscall_numbers!`](../../../prima-abi/src/lib.rs) 与 `tests::numbers_match_the_frozen_d17_ledger` |
| D17 权限位序冻结 | `Right` 的 31 项位序、位号与枚举序逐项钉死；M1b 只能追加 `GateAttach`/`GateServe` 到位 31、32 | `right_bit_order_is_frozen_and_only_appendable` |
| S6-M2 ⑤ 两条 `auxv` 路径 | 键号、键组顺序与取值口径收敛到 `prima_abi::auxv`；内核 exec 与 `prima-servicemgr` 不再各写一份字面量 | `prima_abi::auxv` + `kernel/src/exec.rs` + `prima-user/src/servicemgr.rs` |
| S6-M2 ① 映像长度上界 | `1 MiB` 字面量改为具名窗口 `MIN_EXEC_IMAGE_LEN..=MAX_EXEC_IMAGE_LEN`（120 B..=4 MiB），覆盖 D12 白名单的 2.03 MiB | `kernel/src/exec.rs` 与对应 host 测试 |
| S6-M2 ② 「整块 copy-in」 | **原判断已过时**：`exec` 自 `be3d6dd`（2026-10-27）起就是按页从请求者地址空间读入，内核侧峰值缓冲 4 KiB | 见[与既有文档的偏差](#与既有文档的偏差) |

仍**未完成**：S6-M2 ③（单次 exec 的帧容量：白名单映像需 526 页，`MAX_SEGMENT_FRAMES`
仍是 149）、S6-M2 ④（初始栈仍 8 页/32 KiB 的取值待 M3 实测）；S6-X3 的自动化守卫仍然缺席，
本次只给出可用于它的两张冻结台账。未新增 syscall 号、权利、对象或阶段 6 退出条件勾选，
阶段 6 仍为**进行中**。

## 基线与输入

基线 `298bf55`（许可切换提交），起始工作区干净。契约链：
[阶段 6–7 细化计划](../plans/2026-10-27-stage6-7-refinement.md) →
[D11–D16 前置裁决](../specs/2026-10-28-stage6-7-release-decisions.md) →
[装载器切片与未完成清单](2026-10-28-stage6-loader-slice.md) →
[D17–D22 机制裁决](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)。
`auxv` 键集与[静态程序契约](../specs/2026-10-26-static-program-contract-design.md)、
M12 冻结集合（`prima-servicemgr` C 装载路径）对齐。

本轮用 D12 白名单里的真实二进制复核装载侧事实：
`busybox-static 1:1.36.1-6ubuntu3.1`（Ubuntu noble-updates amd64），
`sha256=dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14`，
2 124 608 字节，`ELF 64-bit LSB executable, x86-64, statically linked, stripped`。
二进制不入库（`target/s6-sample/` 为本地临时解包），仓库只留 `sha256` 与尺寸。

## 逐项实现

### D17：调用号台账由同一份定义生成

此前 71 个 `SYS_*` 常量是手写清单，改号或漏登记只能靠人工核对。现在：

- `syscall_numbers!` 宏从 `NAME = NUMBER,` 序列**同时**生成 `pub const` 与
  `pub const NUMBERS: &[(u64, &str)]`，因此"常量改了台账没改"这种漂移在结构上不存在；
- `name(num)` 按号查名称，供证据工具把不透明调用号翻译成名字；
- `RESERVED = [52]` 显式登记保留号。

冻结测试（`prima-abi/src/lib.rs` 末尾）另写一份 `FROZEN: [(u64, &str); 71]` 字面量，
**故意与实现分开**：宏生成的两侧永远自洽，只有这张表会把重编号变成测试失败。四条测试
分别检查升序无重复、逐号等于冻结表、`0`/`52`/`73..=78` 不得出现、名称查询往返。

### D17：权限位序冻结

`Right` 的位序是对外契约：派生授权的位集、`render` 的渲染顺序、以及已归账的 QEMU 权限
列表都按 `ALL_RIGHTS` 排列。M1b 要追加两种权利，若插在中间会静默改变既有证据的语义。
新增测试按 31 项逐个断言 `name()`、`bit()` 与枚举判别值，`ALL_RIGHTS` 出现重复项即失败。

### S6-M2 ⑤：`auxv` 只有一处键源

`prima_abi::auxv` 现在是键号、键组与取值口径的唯一来源：

| 成员 | 内容 |
|---|---|
| 16 个键常量 | `PHDR=3`…`EXECFN=31`，与 Linux x86_64 `asm/auxvec.h` 一致 |
| `STATIC_KEYS`(13) | 内核 exec 路径的固定键（不含 `AT_HWCAP`/`AT_RANDOM`/`AT_EXECFN`） |
| `KERNEL_KEYS`(16) | 内核 exec 的完整键序，保持既有字节布局不变 |
| `CORE_KEYS`(7) | `prima-servicemgr` C 装载路径的 M12 冻结子集 |
| `Values` / `value()` | 装载后才确定的键（`AT_EXECFN`/`AT_RANDOM`）由调用方填入，其余取固定口径 |
| `core_pairs()` | C 装载路径逐对写栈的 `(键, 值)` 序列 |

内核侧 `static_auxv` 改为从 `STATIC_KEYS` + `Values` + `value()` 组装，
`AT_RANDOM_BYTES` 也改为 `prima_abi::auxv::RANDOM_BYTES` 的别名；C 装载侧
`write_auxv` 改为 `core_pairs()` 加 `AT_NULL`。`AUX_WORDS` 由
`2 * CORE_KEYS.len() + 2` 推导，不再是字面量 16。

**键序变化**：C 装载路径的 `AT_BASE`/`AT_ENTRY` 顺序按 `CORE_KEYS` 变成 `BASE` 先、
`ENTRY` 后。这是安全的：`getauxval`、musl `__init_libc` 与
`prima-user/c/stage5_probe.c` 的 `auxv_value()` 都按键线性查找，不依赖相对顺序；
`AUX_WORDS` 与栈布局的其余部分未变。

### S6-M2 ①：映像长度窗口具名化

`exec_call` 里的 `(120..=(1 << 20)).contains(&image_len)` 变成
`crate::exec::image_len_ok()`，上下界具名为 `MIN_EXEC_IMAGE_LEN = 120`
（64 B ELF 头 + 一个 56 B 程序头）与 `MAX_EXEC_IMAGE_LEN = 4 MiB`（白名单 2.03 MiB 的
一倍余量）。这条只挡荒谬长度参数，**不是**容量门：真正的门是
`allocate_exec_segment` 的帧数与 `check_exec` 的任务配额，函数文档里写明这一点，
避免把上界误读成"busybox 现在装得进来"。

## 验证

| 命令 | 本轮结果与证据范围 |
|---|---|
| `cargo test -p prima-abi` | PASS：9 passed（5 条台账 + 4 条 `auxv` 键/取值测试） |
| `cargo test -p kernel --lib` | PASS：507 passed，1 ignored（原 505 + 权限位序冻结 + 映像长度窗口各 1 条） |
| `cargo test --workspace` | PASS：1060 passed，0 failed，1 ignored（外部 ELF 样本） |
| `PRIMA_ELF_SAMPLE=… busybox cargo test -p kernel --lib exec -- --ignored` | PASS：D12 白名单映像解析成功、字节长度在窗口内、`PT_LOAD` 合计 **526 页**且大于 `MAX_SEGMENT_FRAMES` |
| `cargo xtask lint` | PASS：fmt、Clippy `-D warnings`、裸机编译、`kernel/src` 35 文件零 `unsafe` |
| `cargo xtask test`（第 1 轮，宿主 load≈10） | **未全绿**：122 PASS / 7 组 30 s 超时（W9 r1、W7 r2、W4 r1、W5 r1、I1 r1、M1 r1、D1 r2），无断言或输出失败 |
| 隔离重跑 W4/W7/W9（`test-process`）与 I1（`test-tty`） | PASS：各双轮，见下方归账 |
| `cargo xtask test`（第 2 轮，宿主 load≈4） | **72 轮 QEMU 全 PASS**；3 项环境失败：R1/R2/R3 的 Rust 参考构建因 `~/.cargo` 在 `workspace-write` 沙箱下只读而失败（`could not remove 'setup' file: … Read-only file system`） |
| 沙箱放宽后 `cargo xtask test-rust` | PASS：R1/R2/R3 参考比对 + 各自 QEMU 双轮 |
| 文档四项 | `self-test`、`build`、`check` 与 `check --base` 全 PASS（337 文档） |

host 证据只能证明计划与栈布局、键表与位序；QEMU 证据只能证明**既有**阶段 5 探针在改动后
仍然通过（`SYS_EXEC` 与 servicemgr 的 C 装载都在其中）。两者都不证明任何未修改 Linux
二进制可运行——那需要 S6-M1 的递送机制与 S6-M3 的 profile。

### 第 1 轮 7 处超时的归账

7 处失败全部是 `QEMU timed out after 30s`（`TEST_TIMEOUT` 为 30 s），没有一处是断言、格式
或协议失败。归因为**宿主争用下的等待超时**，依据是：

- W9 的失败轮已经打印出完整的 `SIGACT|sigaction=1;…;guard=1` 末行——程序自身的功能断言
  全部通过，缺的是来宾在 30 s 内正常退出；
- 同一程序另一轮通过（如 W7 r1、D1 r1 的 `target/prima-os/*.log` 已写出）；
- 宿主 `loadavg` 当轮为 10.5/8 核；第 2 轮同一负载降到 4 左右时 72 轮全绿；
- 隔离重跑 W4、W7、W9、I1 各双轮均 PASS。

W5、M1、D1 当轮没有单项子命令（`test-process` 只接受 W4/W6/W7/W7b/W8/W9），未做隔离
重跑；它们的覆盖由第 2 轮全量通过承担。两轮的 `target/prima-os/full-suite-errors.log`
都被第 2 轮覆盖，第 1 轮的失败清单以上文记录为准，`target/xtask-test-2026-10-30.log`
保留原始通过/失败计数（122/7）。

R1–R3 的 3 项是**沙箱**环境失败，不是实现回归：报错来自 `rustup` 无法删除
`~/.cargo/bin/rustup-init`，而 `workspace-write` 策略把 `~/.cargo` 挂成只读。放宽沙箱后
同一命令全部通过，故本轮不改任何参考程序、超时或断言，也不把该失败算作实现缺陷。

## 与既有文档的偏差

[装载器切片报告](2026-10-28-stage6-loader-slice.md)的「映像上限（三道，全部未解决）」
里第 ② 条写着"装载是从请求者地址空间**整块 copy-in**，未修改二进制需要流式/按页装载"。
该判断**在写报告时就已经过时**：`copy_exec_page` 于 `be3d6dd`（2026-10-27
"complete capability and process gap followups"）引入，`git log -S 'copy_exec_page' --
kernel/src/syscall.rs` 只命中这一个提交，早于该报告的 2026-10-29 落库。当前实现是：

- `load_exec_plan` 只从用户地址空间复制**至多 4096 B**（ELF 头 + 程序头）；
- `prepare_exec_image` 对每个段按 4 KiB 页调 `copy_exec_page`，源地址是
  `image + file_base + 4096*page`，峰值内核缓冲 4 KiB，不随映像大小增长。

按要求不改历史报告：本报告是该条判断在其范围内的更正，历史文档保留写作时的状态。
该报告中的 ①（1 MiB 上界）由本轮具名化并放宽，③④ 仍成立。

另有两处本轮顺带修正的表述：

- `kernel/src/exec.rs` 里被忽略样本测试的文档链接原先指向不存在的
  `2026-10-28-stage6-simple-slice.md`，改为真实存在的装载器切片报告；
- 该测试原先只断言"能解析"，现补上白名单专属断言（字节数、526 页、仍超帧数门），
  让 S6-M2 ③ 的输入被机器记住。

## 未完成与后继

1. **S6-M2 ③（实质剩余）**：`MAX_SEGMENT_FRAMES = 149` 是 `SegmentFrames::Vector` 的
   内联容量，不是参数微调。inline 数组按值出现在内核栈帧里，M8 曾从 149 试到 192 就触发
   三重故障（`bootstrap.rs` 的常量注释保留了该失败）。可行的方向是照
   `BootServices { slice, count }` 的先例，把 exec 段帧放进独立静态 arena 并给段槽位
   加"区域回收"（现有 `Arena::claim` 单调分配、不复用，直接复用会耗尽段），
   同时保持 `BootstrapState` 的 16 KiB 守卫。需要一次容量与栈预算的完整重测。
2. **S6-M2 ④**：初始栈 8 页（32 KiB）。取值应来自 M3 profile 的实测（musl 启动 +
   `busybox` 多级调用），当前没有 LibOS 就没有依据；先记账，不猜数。
3. **S6-X3 守卫**：本轮给出调用号与权限两张冻结台账，可被将来的守卫引用；但
   "内核/COSIR 不含 Linux 专属对象或 syscall 语义"仍无自动化门。粗粒度关键字扫描会误伤
   内核合法的 `auxv`/`sigaction`/ELF 解析，需要先定一份可复评的白名单口径。
4. Gate/Request 对象、`GateAttach`/`GateServe`、trap 接线与 G1–G3 仍是 M1b 的主体，
   不在本轮范围。

## 阶段影响

- 阶段 6 仍**进行中**，退出条件 0/4 不变；S6-D1 的剩余项从四项缩到 ③④。
- 未改 profile v2、未改白名单成员与哈希、未改阶段 7 结构。
- 新增/修改的键表与位序表是**证据前置**，不是兼容性声明：`prima-abi` 仍是实验性机器
  接口，本报告不把它升格为稳定 ABI。

## 主分支整合后继（2026-10-31）

[整合报告](2026-10-31-stage6-main-merge.md)保留本文的 D17 台账与共享 auxv 键源，
以[按需输入契约](../specs/2026-10-31-stage6-streamed-elf-input.md)替代120 B～4 MiB
字节窗口：声明边界与地址加法检查、实际读取故障以及私有帧准入分别处理。
[容量后继](2026-10-31-stage6-exec-frame-capacity.md)将私有exec扩至640页（含栈），
普通段仍149页。本文的旧窗口、私有149页及当时测试计数保留为历史基线，
不再用作当前限制；8页初始栈与冻结busybox的完整启动仍待验证。
