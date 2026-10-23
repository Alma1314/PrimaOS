---
id: topic-vfs
kind: topic
authority: explanatory
status: maintained
updated: 2026-10-23
---

# 用户态 VFS、文件描述符与路径

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

fd 表与服务句柄如何关联？路径/dirfd/cwd 怎么解释？符号链接、rename、权限和 stdio 各有哪些证据？

检索别名：rootfs, fd, OFD, openat, statx, cwd, rename, symlink, stdio, chmod, umask。

## 权威章节与阅读顺序

白皮书§11；profile v0/v1；原语 Namespace 与用户态文件语义分别读取。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

M4 单文件 → M7 fd/dup → M8 多级 VFS/rootfs → M9 metadata/cwd → M11 树变更 → M13a links/rename/fchdir → M13b mode/umask/stdio。较早“未实现符号链接”仅描述该切片基线。

## 契约、计划与证据入口

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

- [vfs.rs](../../../prima-runtime/src/vfs.rs)
- [store.rs](../../../prima-user/src/store.rs)
- [ns.rs](../../../prima-user/src/ns.rs)
- [servicemgr.rs](../../../prima-user/src/servicemgr.rs)
- [prima_syscall.c](../../../prima-user/c/prima_syscall.c)

## 测试与观察入口

- [stage5_fd.c](../../../prima-user/c/stage5_fd.c)
- [stage5_vfs.c](../../../prima-user/c/stage5_vfs.c)
- [stage5_meta.c](../../../prima-user/c/stage5_meta.c)
- [stage5_mut.c](../../../prima-user/c/stage5_mut.c)
- [stage5_vfs2.c](../../../prima-user/c/stage5_vfs2.c)
- [stage5_vfs3.c](../../../prima-user/c/stage5_vfs3.c)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

节点、路径、句柄和内容区都有固定容量；无硬链接和跨启动持久化。fork 的 fd/cap 继承仍有 close 隔离偏差，子任务 rootfs 按任务授权限制另见 process 主题。profile v1 是提案，不能因测试通过而自动发布。
