---
id: spec-native-runtime-service
kind: spec
authority: design
status: implementation-contract
lifecycle: current
updated: 2026-10-24
---

# 原生运行时与服务：握手和分片事务边界

## 范围与前序

增量修订 [SPEC-084](2026-09-25-stage4-runtime-api-design.md) 的 Hello 校验和
[M13a](2026-10-22-stage5-m13a-vfs-links-design.md) 的分片暂存状态机。
服务/VFS 分层沿用 [LibOS 基础契约](2026-10-23-vfs-libos-foundation-design.md)；
[SPEC-130](../reports/2026-10-16-stage4-closure-report.md) 与
[SPEC-149](../reports/2026-10-17-stage4-exit-review-spec149.md) 的退出证据范围不扩大。
这是本轮实现契约，不是稳定外部 ABI、组织级 profile 裁决或内核证明。

## 成功和失败契约

- Hello 沿用 PRIM/20 B 头/24 B offer。opcode=1、flags=0、payload_len=24，头部
  major/minor 必须分别与 offer 一致；request_id 不限于示例值 3。先验证整个 Hello，再协商。
  失败仍返回协商字 0；合法报文和协商规则不变，不新增 wire 字段。
- 路径最多 64 B，首片 3 B、后续片最多 5 B。后片 offset 必须等于已收到长度，且仍有
  待接收字节。拒绝覆盖、重复、跳片、完成后追加；失败使当前请求失效，COMMIT 返回 EINVAL。
  不支持片段重传；重试必须从新的 OPEN/PAIR 开始。空路径仅保留 fstat/fchmod 的既有入口。
  MUTATE 仅接受 mkdir/unlink/rmdir 已定义操作，不允许借由未知 op 命中内部 chmod 暂存码。
- 双路径槽 0 总是开始新事务并清空两个槽；槽 1 仅在同 op 的槽 0 完整且 COMMIT 确认后开始。
  槽 0 确认后不得再追加。COMMIT 必须对应当前槽与 op，最终提交只消费一次；失败清除活跃态。
  readlink 只使用槽 0；新 readlink 请求清空旧链接结果，失败不暴露上一轮结果。
- 新的单路径与双路径请求互相取消旧暂存，即使新头部无效。不存在隐式回退到前一请求。
  分片拒绝仅改变暂存态，不执行 VFS 操作；VFS 多步 I/O 失败语义仍按基础契约，不承诺回滚。

## 实现与验证

`prima-runtime::protocol` 集中 Hello opcode 与验证。`prima-user` 的 no_std library
提取实际由三个服务二进制调用的暂存逻辑，复用一个首片/续片实现；入口、映射、存储和
回复继续留在 service.rs。不引入依赖、堆分配、异步框架或新权限。

host 测试覆盖合法 0..64 B 路径、边界错误、跨事务残留、错槽/错 op/重复提交、恢复重试与
Hello 字段变异。ring3 协商负例见证增加“结构无效 Hello 被拒绝”的位掩码，xtask 同步严格校验。
完整 cargo xtask test 验证现有原生服务与 POSIX 探针；host 负例不冒充 IPC 端到端攻击验证。

## 限制与重审触发

保留单活跃客户端/单在途请求的有界服务协议；尚无认证会话、请求 ID、任意客户端并发、
超时取消或服务自动重启。提取模块的状态供受信任服务代码使用，不是面向不可信客户端的
Authority API。内核不解析路径/服务版本；不修改 syscall 号、COSIR、镜像格式或容量。
不改变阶段 5 未签署、BLK-003 和 fork 能力隔离等限制。新增并发、重传、协议版本或
共享缓冲所有权机制时，必须重审本契约。
