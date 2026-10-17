# SPEC-136 / EXO-3L：共享 trace 成为唯一记录通道

**阶段**：阶段 4 后续硬化。**基线**：SPEC-135 / EXO-3K。

## 改动

- 两次 `runtime_hello` 各写入自己的共享页；第二页随后追加 13 条服务记录。两页的有界记录数分别为 1 和 14，记录均为 48 字节小端原始数据。写方先写数据再更新计数。
- 内核在 init 页表下输出 `TRACEBYTES|slot=0/1;count=...;data=...`，只按页内上限透传字节。删除 `SYS_TRACE_EMIT`、内核静态 `TraceBuffer` 和旧 `TRACE|` 行；servicemgr 不再双写。
- `xtask` 解码两页，要求两条 `runtime_hello` 完全一致，检查 14 条记录的 ID/顺序及所有服务字段与既有 `PRIMA` 事件一致。负例覆盖缺页、重页、计数、非法十六进制、记录 ID 和两页不一致；原生应用验收也直接读取共享字节流。
- 内核删除未使用的服务共享段固定偏移常量；`ServicemgrWitness::evidence_ok` 仅保留任务绑定和 Segment/Device 存活性检查，服务标记、数量和应答值由 `xtask` 判定。
- 用户 ELF 增长使 boot manifest 的通用服务映像段实际需要 66 页；将 `MAX_SEGMENT_FRAMES` 从 64 调到 66。QEMU 首次运行在 `bootstrap_ready` 后因 `ImageSegmentTooLarge` panic，扩容后双次通过。临时诊断代码已移除。

## 验证

- `cargo test --workspace --offline --quiet`：801 passed。
- `cargo xtask lint`：fmt、Clippy `-D warnings`、裸机内核构建、`kernel/src` 零 `unsafe` 均 PASS。
- `cargo xtask test`：BIOS QEMU run 1/2 与 stage1 boot slice 均 PASS。
- `cargo xtask compat`：`compat cow-write-fault` PASS。
- `cargo xtask tcb`：trusted 1262、mechanism 20127、ratio 5.90%；机制层 unsafe 0；`service_name_lines=0`、`protocol_lines=48`。

## 边界

这是页内**有界定长记录流**，尚不是循环 ring 或任意长度消息。为冻结阶段 4 的 43 条事件格式，内核仍从共享记录组装 `PRIMA` 服务事件，并读取少量 Y 页通用机制回执；服务字段到事件字段的映射仍留在内核。EXO-3 的服务 witness 完全移出内核尚未闭合。阶段晋级仍受 S0-X1 独立复核约束。
