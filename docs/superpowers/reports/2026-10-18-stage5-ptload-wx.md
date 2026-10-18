# 阶段 5：`PT_LOAD` 写段/BSS 私有装载与 W^X

**日期**：2026-10-18。**范围**：把 ring3 服务装载从"整段共享映像只读映射"推进到"按可写
`PT_LOAD` 建私有段、清零 BSS、W^X 分离"。这是 profile v0 登记的第一项阻塞（`putword.c`
的 `static`/musl stdio 需要可写 `.data`/`.bss`）的装载侧能力；不代表 libc、`mmap` 或完整
动态链接。

## 实现

- `prima-user/src/servicemgr.rs`：管理器在创建端点时自解 ELF program header，找出所有
  `PT_LOAD` 且 `PF_W` 的范围；每个范围：
  1. `memseg_create(pages, R|W)` 建**私有**段（BSS 由新段自然清零）；
  2. 临时映射到 `DATA_PROBE_VA`，按 **vaddr** 从已装载映像复制 `p_filesz` 字节；
  3. 解临时映射，`task_create` 后先 `memseg_unmap` 子任务中的只读代码页，再把私有段以
     `R|W` 映射到同一 VA。
- 复制源必须按 vaddr 而不是 ELF `p_offset`：内核 `load_runtime_elf` 已按 vaddr 打包
  并**应用了 `R_X86_64_RELATIVE` 重定位**。用 `p_offset` 会把代码字节复制进 `.data`/GOT，
  端点的首个 `call *%rax` 即跳到数据字节（本轮实测）。
- 可写段上限 `WRITABLE_SEGMENTS = 4`；解析失败/映射失败/复制失败都回滚已建的段与凭据。
- `prima-user/c/stage5_probe.c`：新增 `static volatile uint64_t bss_probe`，`main` 先断言
  初值为 0、写入 `0x5a5a` 再读回，失败返回 5。该变量位于可写 `PT_LOAD` 的 BSS，故同时
  证明**私有可写映射**与**BSS 清零**。

## QEMU 见证（`cargo xtask test` 两轮）

```
CIO|task=17;fd=1;ch=104;fd=2;ch=105
TTYIN|task=17;fd=1;ch=104;fd=2;ch=105
TTYOUT|task=17;in_magic=1;out_of_range=1;out_ok=1
TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=131
CAPACITY|caps_live=131;caps_peak=137;caps_limit=144;segment_slots=41;segment_limit=48;messages_queued=1;messages_peak=14;messages_limit=32;audit_used=757;audit_limit=768
```

- C 探针以 37 退出；若 BSS 非零或不可写，`main` 返回 5，`xtask` 的返回码断言会拒绝。
- 所有服务（alpha/beta/log/ns/store/driver/tty）在替换数据页后继续完成协商、探针与收尾，
  证明 GOT/`.data.rel.ro` 的已重定位副本被正确带入私有段。
- 串口行 `h` / `i` / `xyz` 与 `CIO`/`TTYIN`/`TTYOUT` 保持不变。

## 容量与既有证据变化（逐条登记）

| 项 | 旧值 | 新值 | 原因 |
|---|---:|---:|---|
| `MAX_SEGMENT_FRAMES` | 80 | 96 | 装载器增长使服务映像总量 82 页（14+8+20+8+8+8+5+6+4+目录 1）；留 14 页余量 |
| 管理器用户栈 | 2 页 | 4 页 | `create_service` 栈帧越过 8 KiB（实测在栈底下方 0x40 缺页） |
| `task_ready` / `TASKCAP` 存活 Capability | 123 | 131 | 每个由管理器创建的端点各持一块私有数据段 cap（6 服务 + tty + C 探针 = 8） |
| `CAPACITY` caps_peak | 128 | 137 | 同上 |
| `CAPACITY` segment_slots | 32 | 41 | 8 个私有数据段 + 1（装载顺序） |
| `CAPACITY` audit_used | 746 | 757 | 额外的 create/map/unmap/destroy 审计；余量 11/768，需在后续片评估容量策略 |
| `service_negotiated` manager cap | 0x39 | 0x3c | 端点创建前多一次段 cap 分配，句柄槽位平移；token 仍为 0x37 |
| `TASK4_CODE_OFFSET` | 0x100 | 0x800 | 0x100 覆盖首个服务 ELF 的 `p_paddr/p_filesz/p_memsz`；0x800 位于 `.rela.dyn`，重定位已在装载期消费 |
| initgen 生成码指纹 | `0xd323…c45a` | `0x3a41…4987` | 上述立即数变化 |

## 已知边界

- 每个服务的可写 `PT_LOAD` 上限 4 个；当前每个映像 1 个。
- 数据段在端点生命周期内保持映射；管理器持有其 cap，C 探针的栈段与数据段在退出时销毁并
  由 `TASKCAP` 的 `caps=131` 见证净变化。普通服务的私有段随任务销毁回收，未单独计数。
- 复制的是**已重定位**的映像字节；动态链接、`PT_INTERP`、符号解析仍不在本切片。
- `GNU_RELRO` 未单独降权：可写 `PT_LOAD` 整段以 `R|W` 映射（含 `.data.rel.ro`/GOT），
  这是"可写但不可执行"的 W^X，不是完整 RELRO。后续需要在重定位后把 RELRO 页改回只读。
- `putword.c`/sbase `echo` 的剩余阻塞是 musl stdio（P5-LIB-01）与完整 auxv/argv
  （P5-BOOT-01 仍为 `partial`），不是可写段装载。

## 复现

```sh
cargo test --workspace          # 含解析/回滚与 initgen 指纹
cargo xtask lint                # fmt/clippy/裸机 unsafe 守卫
cargo xtask test                # BIOS 双轮 + 上述串口证据
```
