---
id: report-musl-port-boundary
kind: report
authority: evidence
status: verified-with-limits
updated: 2026-10-24
---

# musl 移植补强：调用边界与 fd/stdio 验证

## 结论与范围

musl 1.2.5 的 Linux 号误入原生调用边界问题已由可重放补丁修复；fd CLOEXEC 复制行为、fcntl 描述符控制、fdopen/追加写与向量 I/O 的返回量/边界完成补强。F1、W4 定向 BIOS 验收各两轮通过；Rust 876 测试执行项、lint 和宿主边界检查通过。沙箱外全量引导回归亦通过（62 轮 BIOS），沙箱内 UDP 权限失败作为独立记录保留。

这是工作实现证据，不是完整 musl/POSIX 支持或阶段 5 签署。仍保留 libc-test、fork 包装续执行、取消/分离、信号与启动参数等缺口。

## 基线与输入

- 代码基线：`5ad4287b4c43591f80b35aa6dde7ae71cdf3957c`，本报告覆盖其上的 musl/C 适配层与 xtask 工作区变更。
- 上游：[musl 快照锁](../compat/musl-snapshot.lock.toml)，1.2.5 commit `0784374d561435f7c787a555aeab8ede699ed298`；未升级、未引入运行时依赖。
- 实现契约：[调用边界设计](../specs/2026-10-24-musl-port-boundary-design.md)。前序：[musl 计划](../plans/2026-10-18-musl-port-plan.md)、[M12](2026-10-22-stage5-m12-c-startup.md)、[M16b-3](2026-10-23-stage5-m16b3-pthread-join.md)、[VFS / LibOS 后继](2026-10-23-vfs-libos-foundation-report.md)。
- Rust 使用仓库 pinned nightly；C 使用现有 clang bare-metal target、静态 PIC musl，宿主参考使用 cc/glibc；QEMU 沿用单 vCPU、qemu64、TCG single、128 MiB、BIOS 和 30 秒门限。
- 新 F1 ELF SHA-256：`b14ad20907b1e2e6c75182d92f210c3c0819ca2a408da74f9cac8d4b1a9552b1`。
- 本次 libc.a SHA-256：`b8d3374681b1ebc6a7538ba6f65d933688d57178e2499a16b6cae5d96108778a`。产物含绝对构建路径；此哈希只标识本次输入工件，不声称跨路径 bit-for-bit 可重现。

## 实现内容

| 部位 | 修复或新增行为 |
|---|---|
| musl 汇编边界 | 普通/取消点调用都走适配层；vfork 确定 ENOSYS；线程自清理不再发送 Linux 11/60；restorer 使用原生 58；移除 Linux vDSO 声明 |
| libc.a 护栏 | 构建后反汇编整个归档，只允许 clone.lo 的 48/49 与 restore.lo 的 58 直接 syscall |
| fd 控制 | F_GETFD/SETFD、F_DUPFD/CLOEXEC、F_GETFL、受限 F_SETFL；共享打开状态与独立 CLOEXEC；dup/dup2 清除新 fd 标志 |
| stdio | fdopen 读模式、已有 O_APPEND fd 的追加模式、fopen("a")；GETFL/SETFL 失败不再被 __fdopen 吞掉 |
| 向量 I/O | 共用 readv/writev 实现；0..1024 向量、SSIZE_MAX 预校验、零长度跳过、后续错误保留进度、短传输结束 |
| sysroot 构建 | 锁文件驱动八补丁；身份/干净状态/许可与 VERSION 哈希校验；仅复制 tracked 源文件；完整缓存 key、构建锁、缺失产物拒绝；默认 make -j2 |
| 验证入口 | test-musl --host-only 与 test-musl；全量 test 也先跑新增 host 检查；不放宽旧 trace、容量、退出或超时断言 |

没有修改内核、原生 syscall 号或文件服务协议；没有增加内核容量。C 适配层增加 8 项共享打开状态值（32 B），本地 fd 数与服务端句柄容量不变。

## 命令与结果

| 命令 | 本次实跑结果 | 范围 |
|---|---|---|
| `CARGO_BUILD_JOBS=2 cargo test --workspace` | PASS，876 测试执行项 | Rust host；非新增 C 检查数量 |
| `CARGO_BUILD_JOBS=2 cargo xtask lint` | PASS fmt/Clippy/裸机编译/29 kernel 文件零 unsafe | Rust 门禁 |
| `python3 prima-user/musl/test_port.py` | PASS，4 个 test 方法 | 实际 C 适配层、取消点汇编、快照/缓存/反汇编护栏 |
| `sh prima-user/musl/build.sh --check` | PASS，锁定输入与当前 cache hit，归档审查通过 | 本地构建输入/产物 |
| `CARGO_BUILD_JOBS=2 cargo xtask test-musl` | PASS，Linux F1/W4 参考及 F1×2/W4×2 BIOS | 当前定向验收 |
| `CARGO_BUILD_JOBS=2 cargo xtask test`（沙箱内） | 24 轮 BIOS 通过，随后 Linux UDP 参考返回 EPERM，退出 1 | 非全量 PASS；不跳过网络断言 |
| `CARGO_BUILD_JOBS=2 cargo xtask test`（获准沙箱外重跑） | PASS，退出 0，62 轮 BIOS 全通过，含全部 Linux 参考；最终 `PASS stage1 boot slice` | 当前完整引导验收；不是完整 POSIX/libc-test |
| `python3 scripts/docs.py self-test/build/check` | PASS，293 文档、2928 章节；主题路由可检索新设计/报告 | 文档结构/检索，不是 libc 兼容证据 |

host C 检查用 `-O2 -Wall -Wextra -Werror` 编译实际 `prima_syscall.c`，仅走无需内核的 null fd 分支；覆盖 CLOEXEC 复制、同 fd dup2、最低空闲分配、表满 EMFILE、错误命令/参数、零向量/长度溢出与部分进度。取消点单独链接真实补丁汇编和 stub，逐项检查 n+六参数、负返回值和 pending-cancel 分支，不能据此宣称 pthread_cancel 端到端可用。

F1 新增字段 `fcntl/fdopen/iov/append/boundary`。Linux 与 Prima 的 fd/stdio 测试输出相同；`boundary` 中的原生取消点调用、vfork 拒绝、动态 append 拒绝仅在 Prima 运行，不是 Linux 差分覆盖。

```text
FD|ok=1;pid=1;ppid=1;open=1;dup=1;share=1;dup2=1;dup3=1;close=1;self=1;reuse=1;bad=1;cloexec=1;fcntl=1;fdopen=1;iov=1;append=1;boundary=1;held=1
PTHREAD|create=1;join=1;ret=1;value=1;self=1|ok=1;err=0
```

定向双轮仍要求进程退出/清理与完整 trace；F1 输出 145 B、hash 3806667068、IPC queued/direct/blocks=81/589/597，audit=3783/8192；W4 输出 56 B、hash 2250858748、IPC=45/159/167，audit=1387/8192。两者 caps_live/peak=132/140，原 cap 上限 160 不变。

日志保存在 git-ignored 的 `target/musl-validation/`，QEMU 细节在 `target/prima-os/F1.log` 与 `W4.log`；这些文件是本次本地工件，后续运行可能覆盖。没有把镜像、上游源码或 sysroot 提交入仓库。

## 失败与限制

- 沙箱内全量运行的默认、E1–E6、T1、F1、V1/V2/V3 各双轮（24 轮）通过，随后 Linux 环回 UDP 探针输出 `NET|pair=0;zero=0;large=0;drop=0;reuse=0;badfam=0;closed=0|ok=0;err=1`，返回 EPERM；记录于 `target/musl-validation/full.log`。获准在沙箱外重跑后，同一 Linux UDP 参考及 QEMU N1 双轮通过，支持该 EPERM 来自沙箱权限约束的判断；沙箱外全量最终退出 0，结果见上表，不降低验收条件。

- 开发中的严格快照检查曾因原快照的 ignored `obj/` 拒绝构建。后继修复只导出 Git tracked 源文件，既排除宿主生成物污染，也保留原快照里的生成文件；host fixture 验证该分支。
- 开发中的 readv 溢出差分发现 Linux 对某些总长超过 SSIZE_MAX 的组合按 MAX_RW_COUNT 截断；最终公共探针改用单向量 `~0UL` 的明确非法长度，适配层总长预校验另在 host 测试。没有把两种边界宣称完全等价。
- 异步取消、IPC 被取消或信号打断、EINTR、完整 pthread_cancel/detach 没有新增运行证据。`__unmapself` 的保护路径保留线程映射到进程生命周期结束，不能反复创建 detached 线程来证明无泄漏。
- rt_sigaction 仍没有完整 libc 映射；修正 restorer 编号不代表 Linux 信号帧/掩码/SA_RESTART 可用。vfork 仍不支持。
- F_SETFL 不支持切换 APPEND 或 NONBLOCK/ASYNC/锁；exec 尚不自动关闭 CLOEXEC fd；fork fd 生命周期与包装返回值问题保留。
- readv/writev 由多 IPC 组成，不具备 Linux 的共享打开描述级原子性；非 NULL 坏地址仍可能终止用户任务。
- sysroot 输入验证不代替上游供应链验证；本机 target 缓存是可信的本地产物目录。补丁重放与构建成功不等于上游 libc-test 全量通过；本次未引入 libc-test 快照或运行其矩阵。

## 阶段与后继关系

P5-FD-01、P5-IO-01/02、P5-LIB-02 继续 partial；新增 fcntl/汇编工作子集见台账后继章节。M12 的“CLOEXEC 仅记录”由本次补强为可读写的每 fd 标志，exec 关闭义务继续未满足；M16b-3 的 create/join 定向证据由本次重新执行补强。本次完整运行另有 62 轮 BIOS 证据，V1/V2/V3 均重跑双轮；历史 M21 的 114 PASS 不自动覆盖本次变更，前次 VFS 的超时记录仍保留，未声称已定位或修复历史超时根因。

所有 S5-D*/S5-X* 复选框不变，BLK-003 保持 Open，阶段 6 不由本报告启动。
