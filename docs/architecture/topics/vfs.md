---
id: topic-vfs
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-24
---

# 用户态 VFS、文件描述符与路径

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

fd 表与服务句柄如何关联？路径/dirfd/cwd 怎么解释？符号链接、rename、权限和 stdio 各有哪些证据？

检索别名：rootfs, fd, OFD, openat, statx, cwd, rename, symlink, stdio, chmod, umask, LibOS, OpenFiles, orphan, 稀疏写。

## 权威章节与阅读顺序

白皮书§11；profile v0/v1；原语 Namespace 与用户态文件语义分别读取。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

M4 单文件 → M7 fd/dup → M8 多级 VFS/rootfs → M9 metadata/cwd → M11 树变更 → M13a links/rename/fchdir → M13b mode/umask/stdio → LibOS 原生打开对象与延迟回收。较早“未实现符号链接”仅描述该切片基线。

## 契约、计划与证据入口

- [原生握手与分片事务契约](../../superpowers/specs/2026-10-24-native-runtime-service-design.md)
- [运行时/服务后继验证](../../superpowers/reports/2026-10-24-native-runtime-service-report.md)

- [LibOS 基础设计](../../superpowers/specs/2026-10-23-vfs-libos-foundation-design.md)
- [LibOS 重构计划](../../superpowers/plans/2026-10-23-vfs-libos-foundation-plan.md)
- [LibOS 重构验证报告](../../superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)
- [2026-10-20-stage5-user-vfs-design.md](../../superpowers/specs/2026-10-20-stage5-user-vfs-design.md)
- [2026-10-20-stage5-profile-v1.md](../../superpowers/specs/2026-10-20-stage5-profile-v1.md)
- [2026-10-22-stage5-m11-file-mutation-design.md](../../superpowers/specs/2026-10-22-stage5-m11-file-mutation-design.md)
- [2026-10-22-stage5-m13a-vfs-links-design.md](../../superpowers/specs/2026-10-22-stage5-m13a-vfs-links-design.md)
- [2026-10-22-stage5-m13b-permissions-stdio-design.md](../../superpowers/specs/2026-10-22-stage5-m13b-permissions-stdio-design.md)
- [2026-10-20-stage5-m8-user-vfs.md](../../superpowers/reports/2026-10-20-stage5-m8-user-vfs.md)
- [2026-10-21-stage5-m9-metadata-cwd.md](../../superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md)
- [2026-10-22-stage5-m11-file-mutation.md](../../superpowers/reports/2026-10-22-stage5-m11-file-mutation.md)
- [2026-10-22-stage5-m13a-vfs-links.md](../../superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md)
- [2026-10-22-stage5-m13b-permissions-stdio.md](../../superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md)
- [compat-state.md](../../superpowers/compat/compat-state.md)

## 源码定位

- [staging.rs](../../../prima-user/src/staging.rs) — 生产使用且可 host 验证的路径状态机

- [vfs.rs](../../../prima-runtime/src/vfs.rs) — 路径、RAM 镜像与节点生命周期
- [handles.rs](../../../prima-runtime/src/vfs/handles.rs) — 原生打开对象、类型化访问意图与游标
- [service.rs](../../../prima-user/src/service.rs) — IPC 编解码、回复与错误映射
- [store.rs](../../../prima-user/src/store.rs)
- [ns.rs](../../../prima-user/src/ns.rs)
- [servicemgr.rs](../../../prima-user/src/servicemgr.rs)
- [prima_syscall.c](../../../prima-user/c/prima_syscall.c)

## 测试与观察入口

- [tests.rs](../../../prima-runtime/src/vfs/tests.rs) — 后端及打开对象 host 回归
- [stage5_fd.c](../../../prima-user/c/stage5_fd.c)
- [stage5_vfs.c](../../../prima-user/c/stage5_vfs.c)
- [stage5_meta.c](../../../prima-user/c/stage5_meta.c)
- [stage5_mut.c](../../../prima-user/c/stage5_mut.c)
- [stage5_vfs2.c](../../../prima-user/c/stage5_vfs2.c)
- [stage5_vfs3.c](../../../prima-user/c/stage5_vfs3.c)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

普通文件 unlink/rename 替换后，旧打开对象保留到最后 close；打开目录仍拒绝删除/替换。v3 镜像只对全零头自动格式化，旧版本须显式处理。

节点、路径、句柄和内容区都有固定容量；无硬链接、跨启动持久化、每 LibOS 独立根视图或跨客户端句柄授权。fork 的 fd/cap 继承仍有 close 隔离偏差，子任务 rootfs 按任务授权限制另见 process 主题。profile v1 是提案，不能因测试通过而自动发布。
