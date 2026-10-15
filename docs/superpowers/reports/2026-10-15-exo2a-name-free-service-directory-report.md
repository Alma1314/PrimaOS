# SPEC-116 / EXO-2A：无服务名的映像目录

**日期**：2026-10-15
**阶段**：阶段 4（Exokernel 对齐系列，EXO-2 的第一小片）
**基线**：EXO-1 / SPEC-115

## 目标

继续把“服务叫什么”从内核移出：内核在服务映像段首页写目录时，不再写服务名，只写
`offset_pages / page_count`。服务名由 ring3 `prima-servicemgr` 的 `SERVICE_TABLE` 持有，
并按目录顺序与表顺序对齐。

这不是完整的 EXO-2（init 机器码、协议 tag、witness 仍未下沉），但把**服务映像目录从
“按名查表”改为“按 opaque 顺序索引”**，是用户态 loader/manifest 接管前的必要一步。

## 设计

### 内核侧目录

布局从：

```text
magic | count | [name | offset_pages | page_count] × 6
```

改为：

```text
magic | count | [offset_pages | page_count] × 6
```

内核仍按固定顺序写 6 项，但不生成名字；顺序为
`alpha / beta / log / nsvc / store / ioprt`，必须与 `prima-servicemgr::SERVICE_TABLE` 一致。

### ring3 服务管理器

`prima-servicemgr::read_image_directory` 改为：

- 读取 `magic` / `count`；
- 对第 `index` 项读取 `(offset_pages, pages)`；
- 直接用 `SERVICE_TABLE[index].0` 作为服务名建立 `Pending`。

因此“名字表”的唯一权威在 ring3，内核不再传递名字。

### 常量与清理

- `prima-runtime::protocol::SERVICE_IMAGE_ENTRY_LEN`：24 → **16**。
- `kernel/src/user.rs` 删除不再使用的 `SERVICE_NAME_LOG`、`SERVICE_NAME_NS`、
  `SERVICE_NAME_STORE`、`SERVICE_NAME_DRIVER`、`SERVICE_NAME_UNKNOWN`。
- 仅保留 `SERVICE_NAME_ALPHA/BETA`，因为现有 EXO-1 过渡 witness 仍需核验这两个名字；
  这些 witness 属 EXO-3 的迁移对象。

## 改动

| 文件 | 内容 |
|---|---|
| `kernel/src/user.rs` | `write_image_directory` 写无名字目录；删除未用服务名常量 |
| `prima-user/src/servicemgr.rs` | 目录按索引读取；名字由 `SERVICE_TABLE` 持有 |
| `prima-runtime/src/protocol.rs` | 目录项长度 16、布局注释更新 |
| `docs/superpowers/specs/2026-10-15-exokernel-alignment-design.md` | EXO-2 状态更新 |

## 证据

- `cargo xtask test`：**6/6 PASS**，BIOS 两次退出码 33，**43 条事件**。
- 直接 QEMU 捕获与 EXO-1 基线的 43 条事件流逐字节相同（无事件/字段漂移）。
- `cargo test --workspace`：**795 passed, 0 failed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted_lines=1262 mechanism_lines=21677 ratio=5.50%`；机制层 `unsafe=0`。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 未决

1. `user_code()` 中的 init 机器码、服务协议 tag、共享段 witness 仍在 `kernel/src/user.rs`。
2. 目录顺序由内核与 `SERVICE_TABLE` 两处注释约束，尚无共享 manifest 类型；完整 EXO-2 应把
   顺序/manifest 与加载器一起移到 ring3。
3. RAMDISK 仍是 EXO-1 的固定 7 项格式，服务名未随 ramdisk 传送。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件；它是 EXO-2 的用户态目录语义小片。
