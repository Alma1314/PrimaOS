# SPEC-133 / EXO-3J：原始 trace 进入 QEMU 验收

**日期**：2026-10-17
**阶段**：阶段 4 / EXO-3 后续硬化
**基线**：SPEC-132（`3b80c0a`）

EXO-3I 已把通用 trace 记录以 `TRACE|` 行输出，但 QEMU 验收主要依赖由内核组装的 `PRIMA|` witness；原始服务记录未参与判定。现在 `xtask` 解析全部原始 trace，核对 ID、字段格式、数量及顺序，并把服务记录的所有语义字段与相应 PRIMA witness 逐项对照。两次客户端 `runtime_hello` 上报必须相同。原生应用的 `EVENT_NATIVE_APP` 仍由已有校验器直接判定。

新检查接入 BIOS 双次运行与兼容性捕获路径。负例测试覆盖缺失、重复、格式损坏、双次 hello 分歧及服务字段分歧。真实 BIOS 串口当前有 15 条 trace（其中两条 `runtime_hello`），PRIMA 仍为 43 条事件。

验证：`cargo test --workspace` 通过（`xtask` 210 项）；`cargo xtask lint` 四项通过；`cargo xtask test` 完成 BIOS 双次启动；`cargo xtask compat` 通过。命令均在离线 Cargo 模式下运行。

**边界**：这一步提高外部 harness 对原始通道的独立可见性，尚未移除内核对 trace 的静态缓冲与 witness 组装，也未实现共享 ring 或用户态任意字节流。阶段 4 技术退出条件和正式晋级状态不变。
