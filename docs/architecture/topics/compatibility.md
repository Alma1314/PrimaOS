---
id: topic-compatibility
kind: topic
authority: explanatory
status: maintained
updated: 2026-10-24
---

# POSIX profile、musl、网络与 Linux 路线

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

源码兼容与未修改 ELF 兼容有什么区别？哪个 profile 已冻结？逐 API 和符号怎么追溯？哪些测试有 Linux 参考？

检索别名：POSIX, Linux, musl, libc, profile, ABI, sbase, UDP, socket, compat。

## 权威章节与阅读顺序

白皮书§8/§15；profile v0/v1；compat 两表；阶段5 M21 与阶段6入口。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

阶段5准入/修订计划 → profile v0 → musl M0–M4 → sbase 与各子系统里程碑 → M20 两表汇总 → M21 技术出口评审（未签署）。profile v1 是 delta 提案；M21 记录 profile v2 未编写与 Rust target 缺口。阶段6不能由 exec_replace 探针直接宣告启动。

## 契约、计划与证据入口

- [musl 调用边界与可重放构建设计](../../superpowers/specs/2026-10-24-musl-port-boundary-design.md)
- [musl fd/stdio 验证报告](../../superpowers/reports/2026-10-24-musl-port-boundary-report.md)

- [VFS / LibOS 基础设计](../../superpowers/specs/2026-10-23-vfs-libos-foundation-design.md)
- [VFS / LibOS 验证报告](../../superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)
- [Prima OS 实现阶段.md](../../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md)
- [2026-10-18-stage5-profile-v0.md](../../superpowers/specs/2026-10-18-stage5-profile-v0.md)
- [2026-10-20-stage5-profile-v1.md](../../superpowers/specs/2026-10-20-stage5-profile-v1.md)
- [2026-10-18-musl-port-plan.md](../../superpowers/plans/2026-10-18-musl-port-plan.md)
- [2026-10-18-stage5-revised-plan.md](../../superpowers/plans/2026-10-18-stage5-revised-plan.md)
- [2026-10-22-stage5-completion-roadmap.md](../../superpowers/plans/2026-10-22-stage5-completion-roadmap.md)
- [compat-state.md](../../superpowers/compat/compat-state.md)
- [symbol-trace.md](../../superpowers/compat/symbol-trace.md)
- [2026-10-18-stage5-musl-m3-base.md](../../superpowers/reports/2026-10-18-stage5-musl-m3-base.md)
- [2026-10-19-stage5-m5-sbase-echo.md](../../superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md)
- [2026-10-23-stage5-m19-loopback-udp-design.md](../../superpowers/specs/2026-10-23-stage5-m19-loopback-udp-design.md)
- [2026-10-23-stage5-m19-loopback-udp.md](../../superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md)
- [2026-10-23-stage5-m20-compat-final.md](../../superpowers/reports/2026-10-23-stage5-m20-compat-final.md)
- [2026-10-23-stage5-m21-exit-review.md](../../superpowers/reports/2026-10-23-stage5-m21-exit-review.md)

## 源码定位

- [prima_crt.c](../../../prima-user/c/prima_crt.c)
- [prima_crt_start.S](../../../prima-user/c/prima_crt_start.S)
- [prima_syscall.c](../../../prima-user/c/prima_syscall.c)
- [build.sh](../../../prima-user/musl/build.sh)
- [syscall_arch.h](../../../prima-user/musl/patches/syscall_arch.h)
- [compat.rs](../../../xtask/src/compat.rs)
- [linux.rs](../../../xtask/src/linux.rs)

## 测试与观察入口

- [musl 宿主边界测试](../../../prima-user/musl/test_port.py)
- [fd/stdio 同源探针](../../../prima-user/c/stage5_fd.c)
- [stage5_probe.c](../../../prima-user/c/stage5_probe.c)
- [stage5_net.c](../../../prima-user/c/stage5_net.c)
- [image.rs](../../../xtask/src/image.rs)
- [protocol.rs](../../../xtask/src/protocol.rs)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

网络仅用户态 loopback IPv4/UDP，未包含 NIC/TCP/DNS。W7/W7b 无 Linux 同源参考。cargo xtask compat 是 cow-write-fault 场景，不等于所有 API 都被差分。完整 POSIX、未修改 Linux 静态 ELF 和动态链接是不同范围。

2026-10-24 后继补强限定 fcntl、fdopen/已有 append、向量 I/O 和 musl 汇编陷入边界；台账新增章节记录实际状态。`test-musl --host-only` 可独立检验 C 适配层；取消点转发正确不表示完整取消或分离线程已经验收。
