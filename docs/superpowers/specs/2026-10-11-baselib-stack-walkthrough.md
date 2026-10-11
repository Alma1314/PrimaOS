# RuxOS 基础库栈 — 走查与扫雷审查记录

**日期**: 2026-10-11　**关联文档**: `docs/ruxos-baselib-spec-v1.md`　**执行**: 阶段四（用户走查）+ 阶段五（扫雷审查）

---

## 1. 用户走查（四类路径）

### 1.1 正常主流程：一个 C 程序从交叉编译到运行

```
app-dev: make app C_SRC=hello.c
 → clang --target=x86_64-ruxos --sysroot=build/sysroot
   产出：ET_DYN + PT_INTERP="/lib/ld-ruxos.so" + NT_RUXOS_ABI note
 → itss: sys_task_create(as, persona=Native)
 → itss: ld.so 已加载 → sys_as_derive/cap_transfer 注入 phdr 读能力 + AT_* 栈
 → 目标进程 _start → ld.so 校验 NT_RUXOS_ABI → __libc_start_main
 → sys_abi_version() 握手通过 → printf → 桩 write → ipc_call 到 tty server → 串口
```
**走查发现**：初版缺 `fork` 的原语映射、`brk` 无内核支撑、信号 syscall 未入表、libc 符号与 syscall 表未逐条对齐 → 已全部修入主文档（见 §2 RED-1/2/3/5）。

### 1.2 关键异常流程

| 场景 | 规格中的行为 | 走查结论 |
|---|---|---|
| libc 与内核 ABI 版本不一致 | `ld.so` 用 `NT_RUXOS_ABI` note 校验，major 不等 → `EABIMISMATCH` 拒绝加载；minor 允许内核 ≥ 库 | 初版未定规则 → 已补（RED-6） |
| 加载畸形 `.so`（12 类） | 逐项拒绝，错误码 `ENOEXEC/EILSEQ/EINVAL`，不崩溃 | 通过（T-LDSO-01 覆盖） |
| 使用已失效句柄（UAF 类） | `ECAPSTALE` → libc 转 `EBADF` 并保留原始码 `ruxos_errno()` | 初版只写"转 EBADF"会掩盖 bug → 已加强制诊断约束 |
| `read()` 期间 rootfs 崩溃 | 首次 `EIPCCLOSED` → `ipc` 库重建 session（仅 1 次）→ 成功则应用无感，失败上抛 `EHOSTDOWN` | 通过 |
| 端点等待超时 | `EIPCTIMEOUT` → `poll` 返回 0（等价语义），**库层禁止重试** | 通过 |
| 内存耗尽 | `EUNTEXHAUST` → `mmap` 返回 `ENOMEM`，malloc 走 `EAGAIN` 重试一次后 `OOM` | 通过 |
| 无熵源 | `sys_getrandom` 非阻塞返回 `EAGAIN`（不返回 0、不阻塞） | 初版写"返回 ENOSYS"与 P1 阻塞语义矛盾 → 已改（YELLOW-2） |

### 1.3 权限边界流程

| 尝试 | 结果 |
|---|---|
| 进程 A 把自己的能力注入进程 B 的线程 | `sys_cap_transfer` 需 B 线程 `CTRL` 权限 → `EACCESS` |
| 无 `EXEC` 权限却 `sys_protect(PROT_EXEC)` | `EACCESS`；`MAP` 与 `EXEC` 是独立位 |
| 跨 AS 的 `Handle` 数值碰撞（A 的 idx=5 传给 B） | `gen` + AS 归属双重校验 → `ECAPFORBID` |
| 驱动尝试 `sys_untyped_decant`（无 DERIVE 的 Untyped） | `EACCESS`，且返回时延与成功路径恒定（防时序探测） |
| 应用调用 `sys_domain_enter(Linux)` | V1 `EABIMISMATCH`（预留位生效，不改 ABI） |

**走查发现**：`EACCESS` 与 `ECAPBAD` 的时序等长要求只有文字没有测试 → 已补 `T-TIME-01` 用例要求。

### 1.4 韧性/降级流程

- rootfs 崩溃：shell 输入仍可用（tty 走 itss），文件读写降级为 `EHOSTDOWN`；重启后 100 ms 内 session 重建（T-LOG-01）。
- log server 崩溃：立即降 L1 串口直写，**应用不感知失败**；恢复后 seq 有 gap，文档已声明 gap 属预期。
- itss 崩溃：存量进程继续运行，新 `fork/exec` 失败（L3），已定义不自动重放。
- 内核 panic：崩溃转储 → 人工重启 → V1 接受（无热备，RPO = 全丢）。已在 §14.3 显式声明风险。

---

## 2. 扫雷审查结果（111 项模板 → 适配为系统软件 48 项检查）

### 🔴 RED — 开发前必须解决（已在本次修订回写主文档）

| # | 问题 | 解决方案（已落地位置） |
|---|---|---|
| RED-1 | **libc 公开符号与 syscall 表不对齐**：`brk`、`msync`(有)、`mlock/munlock`、`sigaction/sigreturn`、`munmap` 等无内核原语或号未分配 | 新增 `0x019 sys_brk`、`0x01A sys_as_derive`、`0x03A-0x03C` 信号三调用；`mlock/munlock` 定为 `EUNIMPLEMENTED`（微内核无交换，登记 PARTIAL）；补"符号↔syscall 追溯表"为 P0 交付物（§4.13） |
| RED-2 | **`fork/vfork` 语义未定义**：微内核下多线程 fork 如何处理，直接决定 itss 与 libc 的实现方式 | 定稿：V1 单线程 fork = `sys_task_create + sys_as_derive(COW) + sys_thread_create`；多线程 fork → `EAGAIN` + 文档声明；完整多线程 fork 属 P2（§5.11） |
| RED-3 | **信号机制与内核异常投递完全缺失**，而 `abort/kill/SIGSEGV` 是 stdio/断言/panic 的地基 | 新增 `sys_signal_set/send/sigreturn`（§4.4 段 0x30）+ 内核在返回用户态检查 pending + `sigcontext` 布局冻结进 IDL（§4.5.1） |
| RED-4 | **libm 归类矛盾**：§13 统计 320 符号，但 `libm` 有数百符号，P1/P2 位置与"符号总数"冲突 | 决策：V1 sysroot 只带**静态 `libm.a`（compiler-rt/libm 全量，不计入承诺符号表）**；统计口径改为"承诺符号"与"存在符号"两栏（§13 修正） |
| RED-5 | `MAP_FIXED/MAP_ANONYMOUS/MAP_NORESERVE` 语义与 `sys_map(page_cap)` 的 **anonymous 映射来源未定义** | 补 §4.14：anonymous 由 libc 侧 `mem_pool` 对象提供页，`sys_map` 传 `cap=0` 表示"从池取页"；flag 位逐条定义 |
| RED-6 | **ABI 版本握手无判定规则**（major/minor 语义、谁校验） | 定稿 §4.15：`ABI_VERSION` 由 major/minor/patch 三段组成；`ld.so` 校验 `NT_RUXOS_ABI`，major 必须相等，minor 内核 ≥ 库；不匹配只拒绝不降级运行 |
| RED-7 | tty/console 后端未指定，`printf` 走不到串口（V1 出口判据 #4 无法成立） | §5.12：V1 tty 由 itss 下 tty server 承担，boot 期允许 `sys_console_write`(0x0A4) 直通 |

### 🟡 YELLOW — 需明确方案（已写入主文档，待用户确认）

| # | 问题 | 推荐方案 |
|---|---|---|
| Y-1 | 时延目标缺环境限定 | `bench/baseline.json` 按 `(qemu, qemu-cpu, hostcpu)` 三元组分别记录；§6.1 数字标 `PROVISIONAL`，M2 起填实测 |
| Y-2 | `getrandom` 非阻塞/阻塞语义 | V1 只非阻塞（`EAGAIN`）；P1 加 `GRND_BLOCKING` |
| Y-3 | `poll/select` 在 P0 单线程下如何实现 | V1 用 `sys_wait_multi`(P1) 之前的忙等 + nanosleep 退化实现，标 PARTIAL；不假装可用 |
| Y-4 | `sys_features` 位图未定义 | 已给位表（§4.4 段 0x00）：bit0 multi_wait / bit1 direct_ipc / bit2 page_loan / bit3 futex / bit4 signals / bit5 console_direct |
| Y-5 | `dtable` 的 `cookie` 字段用途不明 | 定义为 rootfs 侧不透明偏移令牌（支持 O_NONBLOCK 阻塞队列），或删除；建议保留并写进 §4.5 |
| Y-6 | CI 25 min 与 `stress-*` 冲突 | `stress-*` 移出 PR/merge，仅夜间 12h（已改 §17） |
| Y-7 | 无 ELF note 机制描述 | 见 RED-6，`NT_RUXOS_ABI` 进 §4.15 |
| Y-8 | 静态链接程序如何校验 ABI | `crt1` 里硬编码期望 major，启动即校验（无 ld.so 参与） |
| Y-9 | `mmap` 失败与 `errno` 有损映射叠加导致误判 | `RUX_KERR_VERBOSE=1` 时原样上抛，测试环境默认开启 |

### 🟢 GREEN — V2 处理

`dlmopen`；`DT_RELR`/IFUNC；静态 PIE（D-12）；locale/宽字符；rootfs 只读缓存与批量 bio；aarch64/riscv；多 itss 热备；`LTP/PISA` 达标；形式化验证；真机基准常态化。

---

## 3. 本次修订对主文档的改动清单（v1.0 → v1.1）

| # | 改动 | 出处 |
|---|---|---|
| 1 | 新增 §4.13 符号↔syscall 追溯表（P0 交付物 + `make trace-check` 门禁） | RED-1 |
| 2 | 新增 §4.14 anonymous 页池与 mmap flag 冻结位、W^X 强制 | RED-5 |
| 3 | 新增 §4.15 ABI 版本编码与 `NT_RUXOS_ABI` 握手规则（禁止"尽量跑"） | RED-6 |
| 4 | 新增 §4.5.1 `Sigaction/Siginfo/Sigcontext` 布局与投递 5 条规则 | RED-3 |
| 5 | 新增 §5.11 `fork/execve/wait4` 通路、§5.12 tty/console 通路 | RED-2 / RED-7 |
| 6 | 补 syscall：`0x019 sys_brk`、`0x01A sys_as_derive`、`0x01B sys_pool_ctrl`、`0x03A-0x03D` 信号四调用、`0x0A4 sys_console_write`、`0x0A5 sys_crash_ack`；重算保留池 | RED-1/2/3/7 |
| 7 | §4.4 `sys_features` 位图逐位定义（bit0-7），未置位特性调用必返 `ENOSYS` | Y-4 |
| 8 | §6.1 时延全部标 `PROVISIONAL` + 环境三元组 + ">800ns 判内核问题"归因规则 | Y-1 |
| 9 | §7.3 测试矩阵补 14 行（fork/exec/sig/tty/time/cap/stk/handle），其中 `T-TIME-01` 落实时序等长要求 | §1.3 走查发现 |
| 10 | §11 判据 #4/#5/#6/#9 改为可测量口径（判据不依赖 busybox） | RED-1 连带 |
| 11 | §13 区分"承诺符号 / 存在符号"，libm 不计入承诺 | RED-4 |
| 12 | §14/§17：`stress-*` 移夜间，PR 门禁 ≤8 min，QEMU 分片理由写明 | Y-6 |
| 13 | §19 里程碑：信号从 M8 提前到 M6，M9 顺延，工期 22 周 / 26 人周对齐 | 走查根因 |
| 14 | §12 红线扩到 13 条（W^X、trace 先行、禁止降级 ABI 校验） | 汇总 |
| 15 | §21 待决策 12 → 18 项，并标注已闭环项与新增 6 项取向决策 | 汇总 |
| 16 | §22 新增发布检查清单（含无回滚能力声明与人工应急） | 阶段六 |
| 17 | 修正 `Msg` 尺寸断言 88 → **64**（`8+40+8+4+4`），§5.1 IDL 目录与 §10 对齐 | 自查 |

### 仍需你拍板的 6 项 🔴

D-01 代号 · D-02 futex/wait_multi 内核化 · D-06 errno 数值复用 Linux · D-09 直接 IPC（**依赖内核册承诺**） · D-13 多线程 fork 返回 `EAGAIN` · D-18 W^X 从 V1 强制

---

内容由 AI 生成
