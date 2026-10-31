---
id: report-stage6-gate-authority-loss
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# S6-M1/M1b 最后服务授权丢失钩子

## 结论与范围

在 `79161be` 干净基线上推进[门控生命周期报告](2026-10-31-stage6-gate-lifecycle.md)的最后服务授权丢失义务。
`drop_capability`、批量 `drop_task_capabilities`、`move_capability`、`revoke_capability`、
`ns_bind` 和携带能力的 IPC 发送共用只读授权投影、总审计预检和孤儿化提交。
固定服务能力空间失去最后一个 GateServe，或任一 Delivered 请求失去最后一个可答复别名，
就孤儿化整个 Gate，以绑定配置值完成所有客户端等待，只写保存帧 `rax`。
保留客户端绑定，后续 trap 继续失败关闭，不恢复原生分派。

这是内核对象层 host 证据，不分配 ABI、不接 trap、不提供 G1–G3，不勾选 S6-D2 或阶段退出条件。

## trap 接线后继（2026-10-31）

[四个原生控制入口与 G1](2026-10-31-stage6-gate-dispatch.md)补齐本报告未覆盖的
trap/96字节输出/原生ABI与有界ring3见证；75/76与完整G2/G3仍未完成。下文保留原始范围及失败记录。

## 契约、实现与边界

输入为[协议 §1–5](../specs/2026-10-29-stage6-m1-protocol.md)、
[D17–D22](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)、
[历史复审](2026-10-29-stage6-m1-decision-review.md)、
[M1a 整改](2026-10-29-stage6-m1-blocker-closure.md)、前序对象层报告及其失败记录。
沿用 pinned nightly、离线依赖、单核独占 BootstrapState 提交；未引入依赖或扩大结构体。

- 预检按批量将消失的 token 集合计算，检查原固定服务者实际持有的 GateServe 别名；
  GateAttach 或别人的 GateServe 不算服务授权。同一能力空间内移动不触发孤儿化。
- 总预算覆盖原操作、客户端完成/Ready/终结、所有请求别名撤销、服务等待完成及释放、
  GateOrphaned。原操作即使还放得下一条事件，完整事务放不下也先记录全局停止诊断，
  token、位置、队列、等待和保存帧都不变。预算保守包含原操作可能已经撤销的请求别名，
  因此是上界而非精确的实际事件数。
- 销毁与授权丢失复用同一孤儿化提交。服务者仍活着时完成空队列私有等待并释放预留；
  后继原生 wait 适配器必须重验 Gate 后结束为 stale，不能直接把这次私有唤醒当成功输出。
  服务者销毁仍由既有销毁路径回收其等待，已孤儿化 Gate 不重复记事件。
- 请求根与中间父句柄可能先被丢弃。reply、客户端销毁、孤儿化现在按请求对象身份枚举
  活动能力，而不是依赖仍存在的根记录遍历父链，避免遗漏孙别名。
- 孤儿化保留绑定的 Idle 请求槽，修复旧清理将槽删掉后绑定仍存在的投影缺口。
  `gate_well_formed` 的 Gate 授权检查改为实际存活的服务别名，不要求最初根句柄仍在。
- 请求终结会退役别名，容器仍保留原身份。`recv`/`ns_unbind` 交付原消息或条目中的失效
  句柄，不重建位置或权限；Namespace 只对实际位置移动记录 CapabilityMoved。
  这是 Gate 显式撤销的容器清理细化，区别于 COSIR §4.3 的“对象死亡但 token 仍存在”：
  本路径返回的是已退役句柄，不能再次 drop 或授权；不声称它仍是可清理的活动 stale token。

## 反例与验证

新增 7 组测试覆盖：六种 Gate 授权丢失出口（drop/move/revoke/Namespace/IPC 队列/直投）；
根和中间父被丢弃后的孙别名答复；最后请求授权 drop/revoke；请求 token 转入容器后仍可取回
原身份；相同持有者移动、外部祖先子树撤销与无 GateServe 的别名；空队列服务唤醒/释放及
随后销毁；三种审计不足出口逐项验证 token/等待/帧/Gate 不变。

开发反例：首次新增测试误用了不存在的 `Path::from_bytes`；改用现有 `parse` 后又暴露路径为
相对路径（前导 `/` 返回 EmptyComponent）。另一个断言把已退役槽误认作必为 Stale，实际
未重用槽返回 Free；改为验证非 Live。均属夹具修正，未放宽授权、审计或资源限制。

## 命令与结果

| 命令 | 本次结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib gate::tests` | PASS：17 组（新增 7 组）；host 对象/能力/等待/帧，无 QEMU |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1083 passed、0 failed、1 个既有外部 ELF 样本 ignored |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy `-D warnings`、裸机 `deny(unsafe_code)` 编译、kernel/src 37 文件零 unsafe |
| 文档四项 | PASS：`self-test`、`build`、`check`、`check --base 79161be`，348 文档 |
| `git diff --check` | PASS，无空白错误 |

[夹具失败输出摘录](#关键输出摘录)保留上述 Free/Stale 与相对路径反例。
文档首次检查因引用的 `docs.txt` 在检查后才复制而失败（[输出摘录](#关键输出摘录)）；
先生成该工件后完整重跑四项校验。最终验证对应本报告所述完整代码，未改断言门限或跳过失败用例。

## 阶段影响与后续

本报告仅后继补齐前序对象层报告的能力丢失钩子与请求别名清理；不替代其 QEMU 超时记录。
机器 trap/返回帧适配、73/74/77/78、wait 的 96 字节输出事务、M1c copy 与 G1–G3 仍待完成。
未改启动或 trap 入口，本切片没有运行 BIOS 套件；没有新增任何裸机通过主张。
后继若开放共享客户端 root、可警报等待、多核提交或改变容器失效句柄语义，须重新审查。
当前状态见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

## 关键输出摘录

以下片段来自本报告原验证记录，仅内嵌支撑结论的输出；本次文档整理未重跑这些验证。
完整旧附件的恢复版本及目录见[日志整理记录](2026-10-31-documentation-log-inline.md)。

原 `development-failure.txt` 的关键行：

```text
assertion `left == right` failed
  left: Free
 right: Stale
called `Result::unwrap()` on an `Err` value: EmptyComponent
test result: FAILED. 13 passed; 3 failed; 0 ignored; 0 measured; 513 filtered out; finished in 0.02s
```

原 `focused.txt` 的关键行：

```text
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 513 filtered out; finished in 0.02s
```

原 `docs-first-failure.txt` 的关键行：

```text
FAIL docs: 1 error(s)
```
