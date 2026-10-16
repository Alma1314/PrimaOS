# SPEC-121 / EXO-2D：通用 entry ABI + 用户态 manifest

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-2 第四小片）
**基线**：SPEC-120 / EXO-2C

## 目标

把 boot image 从“固定 8 项、内核按索引解释服务角色”推进为**带 opaque `kind` 的通用
manifest**；内核只按 `kind == KIND_INIT` 找 init，其余条目按 manifest 顺序通用打包为
服务映像段和目录。init 不再镜像具体 capability 句柄或 alpha/servicemgr 布局，而是由
`task1_frame.r15` 指向内核写入的 **boot info 通用 entry ABI**，通过 `[r15+disp]` 读取。

## 设计

### manifest v2

`prima-abi::boot_image`：

```text
magic | version=2 | count
[kind | offset | len] × count
```

内核不限制固定条目数，只校验 `0 < count <= MAX_ENTRIES`；`KIND_INIT` 是唯一保留
`kind`。服务 `kind` 复用用户态 `SERVICE_BEHAVIOR_*`（1..6），服务管理器自身用
`SERVICE_IMAGE_KIND_SERVICEMGR = 0x100`。

### service image directory v2

服务映像段首页目录项扩展为：

```text
magic | count | [kind | offset_pages | page_count] × count
```

内核只按 manifest 顺序复制 `kind`，不解释服务名/协议；ring3 `prima-servicemgr`
按 `kind` 建端点并跳过自身。这样 manifest 顺序可在用户态/构建工具层改变，内核逻辑不变。

### boot info entry ABI

`prima-abi::boot_abi` 定义 init 用户栈页内的 boot info 区（`BOOT_INFO_VA =
USER_STACK_VA + 0x100`）：

- `magic/version/entry_count`；
- 每个 manifest 条目的 `kind/offset_pages/pages/entry_va/code_end_va`；
- boot capability 句柄：image / namespace / task / platform task1 / task2 send /
  service image / service image manager / servicemgr send / service3 stack /
  task_create send / task4 send / exc source ×2 / platform segment caps /
  task_create root rights。

`task1_frame.r15 = BOOT_INFO_VA`。`prima-initgen` 用 10 字节等长
`mov reg, [r15+disp32] + nop` 槽替换原来的 `mov reg, imm64`，保留所有手写机器码偏移，
并删除 `prima-initgen/src/bootstrap.rs` 的 capability 句柄镜像。

### 内核侧

- `BootLayout` 改为 `count + entries[] + pages_off[] + total_pages`；
- `install_boot_image` 只循环 manifest 条目、解析通用 ELF，并按 `KIND_INIT` 定位 init；
- `prepare_task_create` 按 manifest 顺序把所有非 init 条目打包；
- `write_image_directory` 写 `kind / offset_pages / pages`；
- `install_boot_info` 在 all caps mint 完成后写 boot info；
- `task1_frame` 设置 `r15`。

## 改动

| 文件 | 内容 |
|---|---|
| `prima-abi/src/lib.rs` | manifest v2 + `KIND_INIT` + `boot_abi` entry/capability ABI |
| `prima-runtime/src/protocol.rs` | `SERVICE_IMAGE_KIND_SERVICEMGR`；目录长度 24 |
| `prima-initgen/` | BootInfo 加载生成器；删除本地 capability 句柄镜像 `bootstrap.rs` |
| `xtask/src/image.rs` | 生成带 `kind` 的 8 项 manifest；init 只接收 entry indices |
| `kernel/src/user.rs` | 通用 manifest 解析、BootInfo 写入、generic directory、`r15` entry ABI |
| `prima-user/src/servicemgr.rs` | 按 `kind` 读目录，跳过 servicemgr，建立服务槽 |
| `kernel/src/main.rs` | `prepare_task_create` 后调用 `install_boot_info` |

## 证据

- `cargo xtask test`：**6/6 PASS**；BIOS 两次退出码 **33**、**43 条 PRIMA 事件**。
- 原始 BIOS 串口：`frames_ready.image=1114`、`self_root_ready.pages=359`。
- `cargo test --workspace`：**796 passed, 0 failed**
  （kernel 376、prima-initgen 1、xtask 206）。
- `cargo xtask lint`：**4/4 PASS**。
- EXO-2D 完成时 `cargo xtask tcb`：
  `trusted_lines=1262 mechanism_lines=20126 total_lines=21388 ratio=5.90%`；
  机制层 `unsafe=0`；耦合面 `include_bytes=0 prima_user_dep=false
  service_name_lines=2 protocol_lines=153`。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 未决

1. init 仍由 host 构建工具生成，并非运行时 Rust ring3 loader；manifest 的“用户态解析”
   目前主要发生在 `prima-servicemgr` 与生成期。
2. `prima-initgen` 的 init 机器码仍在若干位置使用确定性 syscall 返回句柄常量
   （`CAP_CHILD*`/`MEMSEG_CAP*`）作为后续输入；这些是用户态测试期望，不是内核 boot
   capability 镜像，但完整迁移仍应改为动态结果值。
3. 协议 tag/共享段 witness 偏移与内核 witness 读取尚未进入 EXO-3 通用 trace；见
   `docs/superpowers/reports/2026-10-16-exo3c-protocol-tag-decoupling-report.md`。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
