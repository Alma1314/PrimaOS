# SPEC-120 / EXO-2C：init 机器码迁移到 ramdisk 的 ring3 opaque ELF

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-2 第三小片）
**基线**：SPEC-119 / EXO-3B

## 目标

把 `kernel/src/user.rs` 中的 `user_code()` 机器码生成器从内核移出：改由 host 侧
`prima-initgen` 生成同一段 8192 B x86-64 init 代码，`xtask` 把它封装为第 0 个
opaque ramdisk ELF；内核只按通用 ELF program header 把它装入 task 1 的两个代码帧，
再映射到既有的 `USER_CODE_VA` / `USER_CODE2_VA`。内核不再包含 init 编排机器码。

本片不改变 init 的外部行为，也不改变服务 ELF 的 7 项顺序、服务映像目录格式和阶段 4
事件流；协议 tag、共享段 witness 偏移仍留在内核，留给 EXO-3。

## 设计

### `prima-initgen`

新增 host 侧 crate `prima-initgen`，冻结提取：

- `generate(BootLayout) -> [u8; 8192]`；
- init 代码内嵌的 user VA、回执偏移、协议 tag、capability 句柄镜像；
- 只服务生成器的 `put_*` 指令编码 helper。

生成器只消费 `alpha` / `servicemgr` 的页偏移、页数与入口，与原 kernel 版本一致；
其字节级回归测试来自原 `user_code_bytes_are_fixed`，在 host 上继续守住冻结布局。

### ramdisk 第 0 项

`xtask/src/image.rs` 先读取 7 个服务 ELF，计算与内核一致的页布局，再调用
`prima_initgen::generate`，把结果封装成两个连续 `PT_LOAD`（vaddr `0` / `0x1000`）的
最小 ELF。ramdisk 条目变为：

```text
init / alpha / beta / servicemgr / log / ns / store / driver
```

内核 `prima-abi::boot_image::ENTRY_COUNT` 从 7 改为 8；服务映像段仍只包含第 1..7 项，
目录仍写 6 个无名字服务条目。

### 内核装载

`kernel/src/user.rs` 的 `BootLayout` 增加 `init: ImageEntry`：

- `install_boot_image` 解析 8 项，第 0 项作为 init，不进入服务映像段；
- `prepare` 用既有 `load_runtime_elf(layout.init.bytes, ...)` 把两个 ELF 段写入
  `SEG_TASK1_CODE` 的两帧；映射 VA / 栈 / X / Y / 权限保持不变；
- 删除 `user_code()` 及只服务它的 `put_syscall` / `put_store_rax` / `put_load_reg` /
  `put_add_rdi` / `emit_send` 和未再使用的 imports/常量。

## 改动

| 文件 | 内容 |
|---|---|
| `prima-abi/src/lib.rs` | `boot_image::ENTRY_COUNT` 7 → 8 |
| `prima-initgen/` | 新 host crate：冻结生成器、ABI 常量镜像与字节回归测试 |
| `xtask/Cargo.toml` | 依赖 `prima-initgen` |
| `xtask/src/image.rs` | 计算服务 ELF 布局、生成 init 代码、封装最小 ELF、ramdisk 打包 8 项 |
| `kernel/src/user.rs` | `BootLayout` 增加 init；从 ramdisk ELF 装载 init；删除 1480+ 行 `user_code()` 及相关 helper/import |
| `Cargo.toml` | workspace 增加 `prima-initgen` |

## 证据

### QEMU

- `cargo xtask test`：**6/6 PASS**（build kernel / image bios / image uefi /
  qemu bios run=1 / qemu bios run=2 / stage1 boot slice）。
- BIOS 两次运行均退出码 **33**、**43 条 PRIMA 事件**、`boot_complete` 正常。
- 原始 BIOS 串口观测：
  - `frames_ready.image=1101`（EXO-1 报告同字段为 1117）；
  - `self_root_ready.pages=357`（EXO-1 报告为 372）。

### Host 回归

- `cargo test --workspace`：**796 passed, 0 failed**
  （cosir 146、cosir_state 42、lean_backtrace 3、kernel 376、prima-initgen 1、
  prima-ostd 21、prima-runtime 1、xtask 206）。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：
  `trusted_lines=1262 mechanism_lines=19995 total_lines=21257 ratio=5.94%`；
  机制层 `unsafe=0`；
  耦合面 `include_bytes=0 prima_user_dep=false service_name_lines=2 protocol_lines=146`。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 未决

1. 协议 tag、共享段 witness 偏移与内核 witness 读取仍在 `kernel/src/user.rs`，属完整
   EXO-3 的通用 trace/调试通道工作。
2. init 生成器仍内嵌 capability 句柄与固定服务布局镜像；完整 EXO-2 需要通用 entry ABI
   和真正的 ring3 manifest/loader，而不是 host 侧预生成。
3. `prima-initgen` 的 ABI 常量是冻结镜像；本片以字节回归 + QEMU 门禁防漂移，尚未收敛到
   编译期共享 crate。
4. 内核仍按固定 8 项顺序解析 ramdisk，并写服务映像目录；服务选择/注册仍由 ring3
   `prima-servicemgr` 承担，但内核知识面尚未完全归零。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件；它是 Exokernel 对齐系列 EXO-2 的实现证据。
