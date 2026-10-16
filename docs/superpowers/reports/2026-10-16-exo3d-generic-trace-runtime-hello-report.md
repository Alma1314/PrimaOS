# SPEC-123 / EXO-3D：通用 trace 通道 + `runtime_hello_ready` 迁移

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第四小片）
**基线**：SPEC-122 / EXO-3C

## 目标

建立内核**只复制/透传、不解释**的通用用户态 trace 通道，并把
`runtime_hello_ready` 从「内核读共享段固定偏移」迁移到该通道。内核不再持有
`RuntimeHelloWitness`，也不再读 `USER_HELLO_VA` / `USER_Y_VA+736` 来生成该事件。

## 设计

### trace ABI

`prima-abi::trace`：

```text
Record { id: u64, args: [u64; 5] }
MAX_RECORDS = 16
EVENT_RUNTIME_HELLO = 1        // 用户态/xtask 约定；内核不分支解释
SYS_TRACE_EMIT = 37            // trace_emit(id, a, b, c, d, e)
```

`prima-runtime::Runtime::trace_emit(id, [a,b,c,d,e])` 提供用户态写入 API。

### 内核侧

- `kernel/src/syscall.rs` 增加 `TraceBuffer`：只保存 `Record`；
- `SYS_TRACE_EMIT` 把 `(id, args)` 原样写入，容量满返回 `ENOSPC`；
- `pub fn trace_record(index)` 供内核收尾读取；
- `resume::user_trap_exit` 在 seq25 取 `trace_record(0)`，调用
  `protocol::runtime_hello_trace(record)`；只输出 `id/a/b/c/d/e`，不解释字段；
- 删除 `kernel::user::RuntimeHelloWitness` 和 `runtime_hello_witness()`。

### ring3 生产侧

`prima-user/src/main.rs` 的存活客户端在映射并写入 PRIM Hello 成功之后，通过
`trace_emit(EVENT_RUNTIME_HELLO, ...)` 上报 5 个原始 u64：

```text
a = map_result
b = magic_version
c = opcode_request
d = length_version
e = features | required << 16 | max_message_size << 32
```

### xtask 判定

`xtask/src/protocol.rs` 的 `validate_runtime_hello_data` 解析通用 trace 原始字段，
并在外部解包/判定版本、opcode、length、features、required、max_message_size。
事件名与 seq 保持 `runtime_hello_ready` / 25 不变。

## 证据

- `cargo xtask test`：**6/6 PASS**；BIOS 两次退出码 **33**、**43 条事件**。
- 原始串口 seq25：
  ```text
  PRIMA|v=1|seq=25|event=runtime_hello_ready|status=ok|data=id=0x1;a=0x0;b=0x14d495250;c=0x300000001;d=0x100000018;e=0x100000010001
  ```
- 原始串口：`frames_ready.image=1116`、`self_root_ready.pages=360`。
- `cargo test --workspace`：**796 passed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=20090 total=21352 ratio=5.91%`，
  机制层 `unsafe=0`；耦合面 `include_bytes=0 prima_user_dep=false
  service_name_lines=2 protocol_lines=133`。
- `cargo xtask compat`：PASS。

## 未决

1. `service_reply_ready`、`service_negotiated_ready`、`service_selected_ready`、
   `servicemgr_ready` 等事件仍由内核按共享段固定偏移读回；本片只迁移了
   `runtime_hello_ready`。
2. 当前 trace 通道是**内核静态缓冲 + syscall**；尚未实现设计中的共享 trace ring /
   串口通用调试通道。
3. 内核 `user_trap_exit` 仍按事件顺序取 `trace_record(0)`；event name 与 trace 记录
   的关联仍是收尾路径的测试 schema，不是内核协议解释。
4. `runtime_hello` 字段现在由用户态自报；xtask 继续做严格结构/值校验，符合
   “证据生产去内核化”方向，但完整 trace 迁移仍需逐事件登记事件流变化。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
