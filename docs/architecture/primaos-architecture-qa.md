# PrimaOS 架构问答整理

> 历史解释材料：本页保留原对话整理时的实现描述与建议。部分“当前/未实现”表述已由后续阶段5切片推进；当前事实请查[阶段跟踪](../../Prima%20OS%20实现阶段.md)、[VFS主题](topics/vfs.md)与[进程主题](topics/process.md)。本页不是规范或进度权威。

**整理日期**：2026-10-19  
**性质**：对话内容整理，不是 POSIR/COSIR 规范、实现承诺或阶段退出依据。  
**当前状态权威入口**：[`Prima OS 实现阶段.md`](../../Prima%20OS%20实现阶段.md)。

本文把围绕 PrimaOS 文件系统、动态库、多 LibOS 通信、微内核性能、Linux 兼容、原生应用、shell、虚拟机和桌面组件等问题的讨论压缩成一份可检索的架构说明。具体实现与证据应回到对应 spec/plan/report。

> **证据边界**：本文区分“已实现”“部分实现”“拟议设计”“明确不支持”。凡涉及当前支持范围，以实现阶段跟踪、冻结 profile 和最新报告为准。

---

## 0. 结论速览

| 主题 | 当前结论 |
|---|---|
| 文件系统 | 内核不提供文件对象；文件系统应由用户态 VFS/rootfs 服务实现。当前只有单文件内存存储原型。 |
| 动态库 | 尚未实现。当前只支持静态 PIE 和内核 `R_X86_64_RELATIVE` 重定位。 |
| 多 LibOS 通信 | 通过显式 Capability、消息、共享 MemorySegment、Namespace 和公共用户态服务沟通；没有内核级 LibOS 自动关联。 |
| 微内核性能 | 采用测量优先和机制内优化：直接 IPC、批处理、共享页、减少服务跳数、PCID/TLB 优化等；多数目标尚未实测。 |
| Linux 兼容 | 三阶段路线：原生 → POSIX 源码兼容 → 白名单 Linux 静态 ELF；当前处于阶段 5。 |
| 原生应用 | 阶段 4 已完成有界纵向切片；有原生运行时、ring3 服务和 8 KiB 共享段应用，但不是通用应用平台。 |
| Shell | 当前没有。缺 stdin、VFS、exec/fork/wait、完整 tty 和进程语义。 |
| 虚拟机 | 可在 QEMU x86_64 运行；通过串口自动跑启动和验收，不是交互式桌面系统。 |
| 桌面组件 | 核心显示/合成/输入/会话宜原生；应用不必原生，可通过 LibOS 和统一协议接入。 |
| 适配层 | personality-specific 适配应由 LibOS 提供；通用 IPC/ABI/Capability 基础设施应共享。 |

---

## 1. 文件系统设计

### 1.1 内核边界

PrimaOS 的核心原则是：

> **内核不理解 inode、dentry、文件描述符、目录权限、挂载选项或具体文件系统格式。**

内核只提供：

```text
Path → Capability
```

也就是 Namespace 机制。文件系统属于用户态服务。

相关权威说明：

- [`Prima OS.md`](../../Prima%20OS.md) 第 11 章；
- [`kernel/src/namespace.rs`](../../kernel/src/namespace.rs)；
- [`kernel/src/bootstrap.rs`](../../kernel/src/bootstrap.rs) 中的 `ns_*` 操作。

### 1.2 内核 Namespace 的职责

Namespace 条目大致为：

```rust
NsEntry {
    ns: u32,
    path: Path,
    kind: Bound | Mount,
    cap: Handle,
    creator: u32,
    target_ns: u32,
}
```

职责包括：

- `ns_bind`：把 Capability 移到 Namespace 路径；
- `ns_unbind`：删除条目并取回 Capability；
- `ns_mount`：挂载另一个 Namespace；
- `ns_unmount`：清理挂载条目；
- `ns_resolve`：按路径解析并派生权限子集 Capability；
- `ns_destroy`：销毁 Namespace 自身条目。

当前实现限制：

- 路径最多 4 个分量；
- 单分量最多 16 字节；
- ASCII；
- 不允许前导 `/`、尾随 `/`、`//`、`.`、`..`；
- Namespace 数量、条目数量、Capability 槽位均为固定容量。

Mount 图要求无环；目标销毁后允许留下 stale Mount，由 `ns_unmount` 清理，解析时返回 `MountTargetDead`。

### 1.3 当前文件服务原型

当前 `prima-store` 只是阶段性文件原型：

- 唯一文件名 `f`；
- 单层、短路径；
- 一个 4 KiB `MemorySegment`；
- 布局：
  - `0..8`：原有服务状态见证；
  - `8..16`：文件长度；
  - `16..4096`：文件内容，容量 4080 字节；
- 最多 4 个服务端打开句柄，对应 fd 3..6；
- 句柄保存 `open/offset/access`；
- 单条 IPC 最多传 3 字节，由适配层分片。

它不是 VFS，也不是持久化文件系统。

### 1.4 完整文件系统推荐架构

```text
应用 / libc / LibOS fd 表
        │
        │ open/read/write/lseek/stat/getdents...
        ▼
用户态 VFS / rootfs 服务
  - 路径解析、cwd、. / ..
  - 目录树、inode/dentry
  - mount 表
  - open file table
  - 权限和 root view
        │
        ├── ramfs
        ├── initramfs / cpio / tar 只读后端
        └── 持久化 FS 后端
                  │
                  ▼
            用户态块驱动
              ATA PIO / virtio-blk
                  │
                  ▼
          Device / MMIO / IRQ / DMA Capability
```

内核 Namespace 只负责找到 rootfs 服务入口，不负责完整路径解析。

### 1.5 fd 和 open file 语义

POSIX fd 由 libc/LibOS 本地维护：

```text
fd → { open handle, offset, flags, refcount }
```

要求：

- `dup`/`dup2`/`dup3` 共享同一个 open file 和 offset；
- 最后一个引用关闭时才向 VFS 发送 CLOSE；
- 路径解析只发生在 open 阶段；
- `read/write` 直接使用打开句柄，不再逐次解析路径；
- 不要把裸 fd 整数跨 LibOS 传递；跨域共享需显式协议或会话 Capability。

### 1.6 数据路径

路径、flags、offset、length、status 放消息；大块数据使用共享 `MemorySegment`：

```text
客户端创建 Segment
  → 映射到自己的地址空间
  → 把 Segment Capability 随请求发给 VFS
  → VFS 映射同一 Segment
  → 数据按 offset/length 读写
  → 返回完成状态
```

后续再评估 `page_loan` 或零拷贝优化。

### 1.7 推荐实现顺序

1. 内存 VFS：目录、文件、open/read/write/lseek/close/stat/mkdir/unlink/rename；
2. 只读 initramfs：支撑 ELF、脚本和共享库读取；
3. 接入 libc/LibOS fd 表；
4. 用户态块驱动 + 持久化文件系统；
5. 文件 mmap、页缓存和动态链接；
6. 权限、配额、崩溃恢复和跨 LibOS 共享。

**建议不要一开始自己发明磁盘格式。** 可先做 initramfs 和 ramfs，再考虑 ext2/FAT 等现有格式。

### 1.8 当前已验证接口

当前 C 适配层已有：

- 单文件 `f` 的 `openat/read/write/lseek/close`；
- 本地 fd 3..10；
- `dup/dup2/dup3` 与共享 offset；
- `write(1/2)` 到 tty；
- `printf`/`fflush` 输出路径。

但目录、多级路径、通用 VFS、通用 `mmap`、持久化和文件共享尚未实现。

---

## 2. 动态库实现思路

### 2.1 当前状态

**动态库尚未实现。**

当前只支持静态 PIE：

- `ET_DYN`；
- 无 `PT_INTERP`；
- 无 `DT_NEEDED`；
- 无外部共享库；
- 内核只处理 `R_X86_64_RELATIVE` 重定位；
- `xtask` 构建校验会拒绝 `PT_INTERP`、`SHT_REL`、`SHT_RELR` 和非 RELATIVE 重定位。

相关代码：

- [`kernel/src/user.rs`](../../kernel/src/user.rs)：`elf_layout`、`apply_relative_relocations`；
- [`xtask/src/image.rs`](../../xtask/src/image.rs)：`validate_static_pie`。

### 2.2 拟议目标结构

```text
应用 ELF
  ET_DYN + PT_INTERP=/lib/ld-ruxos.so
  DT_NEEDED=libc.so, libfoo.so
  NT_RUXOS_ABI note
        │
        ▼
进程创建服务 / itss / 服务管理器
  - 创建 Task/AS
  - 装载主程序和 ld.so
  - 注入 phdr 能力与 auxv
        │
        ▼
用户态 ld.so
  - 自身重定位
  - 解析 DT_NEEDED
  - 通过 rootfs 读取 .so
  - 映射 PT_LOAD
  - 符号查找与重定位
  - 初始化 TLS
  - 调用 __libc_start_main 或程序入口
```

### 2.3 关键设计

- 动态链接器在用户态；
- 内核只提供地址空间、MemorySegment、Capability 和映射机制；
- `.so` 通过 rootfs/文件服务读取；
- 代码段 `R-X`，数据段 `R-W`，遵守 W^X；
- 支持 `RELATIVE/GLOB_DAT/JUMP_SLOT/64/COPY` 等重定位；
- 初版不支持 `IRELATIVE`、`DT_RELR`、IFUNC；
- TLS V1 只承诺 `Initial-Exec`，应用侧可用 `Local-Exec`；
- 每个动态对象带 `NT_RUXOS_ABI`，major 必须完全一致；
- 符号版本脚本和默认 hidden visibility；
- `dlopen`/`dlmopen` 放到后续阶段。

### 2.4 阶段归属

- 阶段 5 profile v0 明确不支持动态链接；
- 白皮书把动态链接列为阶段 7 候选；
- 较早的 baselib 候选规格把最小 `ld.so` 列为 P0；
- 两处阶段归属尚未统一裁决。

---

## 3. 多 LibOS 如何沟通

### 3.1 基本模型

应用域由 Task、LibOS 实例、私有/共享 Segment 和 Capability 组成。应用域不是内核对象，而是用户态资源管理器维护的组合。

LibOS 之间的核心原则：

> **默认隔离；跨域通信必须显式获得 Capability；内核不通过全局对象表自动建立语义关联。**

### 3.2 四种合法沟通方式

1. **服务 Capability 委托**
   - 一方获得指向另一方服务的 Capability；
   - 通过消息协议发送请求/应答；
   - 当前发送需要指向目标 Task 且含 `TaskSend` 的 Capability。

2. **共同访问独立服务**
   - 例如两个 LibOS 都访问同一个 rootfs、log、net 或 display 服务；
   - 避免 LibOS 两两适配。

3. **共享 MemorySegment**
   - 创建 Segment；
   - 派生只读/读写子 Capability；
   - 随消息转移；
   - 双方映射同一物理页；
   - 消息只传控制信息。

4. **Namespace 组合**
   - 通过 `ns_bind`/`ns_mount` 为不同应用域组合服务视图；
   - Namespace 只解决 Authority 路由，不是文件系统。

### 3.3 当前基础能力

已具备：

- `SYS_SEND`/`SYS_RECV`；
- 阻塞接收直投；
- 消息携带 Capability；
- `CAP_DERIVE`/`CAP_DROP`；
- MemorySegment 创建/映射/销毁；
- 8 KiB 共享段数据路径；
- Namespace 七操作；
- 用户态服务管理器注册和路由。

尚未具备：

- 真正多 LibOS 实例互通验收；
- 通用服务调用/回复 ABI；
- 稳定的服务发现和版本协商；
- 通用 timeout、cancel、backpressure；
- 通用文件/socket session Capability；
- 跨 LibOS fd/进程/信号语义转换。

---

## 4. 微内核性能问题

### 4.1 主要开销

- syscall/trap 陷入；
- Task 调度切换；
- IPC 消息和 Capability 校验；
- 大块数据复制；
- TLB/缓存失效；
- page fault/COW 用户态往返；
- 服务链路过长。

典型问题不是抽象的“微内核慢”，而是“一次 read 需要多次陷入和多次服务跳转”。

### 4.2 优化原则

1. 先测量再优化；
2. 优化 COSIR 细化层，不改 POSIR 语义；
3. 不绕过 Capability 校验；
4. 不把文件系统、网络、socket 等 OS 专属语义塞回内核；
5. 不提供无法审计的通用后门；
6. 性能不变量不能替代安全不变量。

### 4.3 优化组合

- syscall/sysret 快速入口；
- 寄存器消息；
- 直接 IPC：接收方 Ready 时直接切换 TCB，减少调度往返；
- 阻塞唤醒直投；
- batched IPC；
- 服务端点缓存；
- `page_loan`；
- 共享 MemorySegment 零拷贝；
- fd 直绑文件服务会话，`open` 才解析路径；
- rootfs 缓存和批量块 I/O；
- futex/wait_multi，避免用户态轮询；
- PCID/ASID、per-CPU 数据和批量 TLB 优化；
- COW 路径预映射、批量和共享段替代。

### 4.4 当前状态

已具备：

- `syscall/sysret` 快速入口；
- 排队 IPC 和阻塞接收直投；
- 共享 Segment 8 KiB 正确性证据；
- 单核调度和真实 TLB/COW 机制。

未完成或未验证：

- 同步 `call/reply` 快路径；
- ≤250 ns 小消息 IPC 目标；
- `futex`、`wait_multi`；
- 批处理 IPC；
- `page_loan`；
- PCID/ASID 和多核优化；
- 完整性能基线。

因此目前只能声称“有优化设计”，不能声称已达到微内核高性能目标。

---

## 5. Linux 兼容计划

### 5.1 三阶段路线

```text
阶段 1：Prima 原生应用
阶段 5：POSIX 源码兼容
阶段 6：选定 Linux x86_64 静态 ELF
阶段 7：动态链接、SMP、配额、恢复等扩展
```

### 5.2 阶段 5：POSIX 源码兼容

目标：

> 选定 C/POSIX 程序用 Prima sysroot 重新编译后，在 Prima 上保持冻结功能语义。

冻结 profile v0：

- 只承诺静态链接 C/POSIX 源码兼容；
- 不是 Linux ABI 仿真；
- 动态链接、`ld.so`、`PT_INTERP`、共享库不在阶段 5；
- 每项 API 标记 `supported/partial/planned/unsupported`；
- 每项须有 Linux 参考语义和测试；
- 不得以“能编译”代替兼容完成。

当前已有证据：

- 静态 C ELF 和初始栈；
- `write(1/2)` tty 字节流；
- 退出码观察；
- 静态 musl 基座；
- 64 页 arena 上的 `malloc/free/realloc`；
- 单文件 fd 生命周期；
- 本地 fd 3..10 和 `dup`；
- `printf`/`fflush`；
- `getpid`；
- `CLOCK_MONOTONIC` 和轮询式 `nanosleep`；
- sbase `echo` E1–E6 双轮通过。

仍缺：

- 完整 auxv、任意 argv；
- 目录和多级路径；
- 通用 VFS 和文件映射；
- 线程、TLS、futex；
- 完整 signal；
- `fork/execve/wait4`；
- 网络；
- stdio 文件 API；
- 动态链接；
- Rust std 全量；
- LTP/PISA 达标。

### 5.3 阶段 6：Linux 静态 ELF

交付项：

- S6-D1 静态 ELF 加载器和 Linux 初始栈；
- S6-D2 OS 中立 syscall/exception 递送；
- S6-D3 版本化 Linux LibOS syscall profile；
- S6-D4 futex、TLS 和进程环境；
- S6-D5 未修改静态 ELF 白名单与测试镜像。

退出条件：

- S6-X1 白名单静态 ELF 无需修改即可运行；
- S6-X2 支持的 syscall 与 Linux 参考通过差分；
- S6-X3 内核没有 Linux 专属对象或 syscall 语义；
- S6-X4 未支持 syscall 有确定、可审计失败。

### 5.4 明确不在前三阶段承诺

- Linux 动态链接器和完整用户态 ABI；
- 完整 signal、`ptrace`、`io_uring`、eBPF、容器语义；
- Linux 内核模块或驱动二进制兼容；
- Windows NT / macOS ABI；
- 图形子系统；
- 未经 profile 定义的“兼容一切应用”。

当前 `cargo xtask compat` 只覆盖 `cow-write-fault` 有界差异，不是完整 Linux 兼容证明。

---

## 6. Prima 原生应用进展

### 6.1 结论

阶段 4“Prima 原生运行时与服务”已经完成并签署。但它是**原生应用的纵向切片**，不是通用原生应用平台。

### 6.2 已实现

原生运行时 `prima-runtime` 提供：

- Task id/create/destroy/yield；
- Capability derive/drop；
- IPC send/receive；
- MemorySegment create/map/unmap/destroy；
- Namespace 七操作；
- time/timer；
- Device I/O port；
- 协议协商和 trace 辅助。

独立原生程序：

- `prima-smoke`：客户端 + ALPHA 服务端点；
- `prima-beta`：第二个服务端点；
- `prima-servicemgr`：服务管理器；
- `prima-log`、`prima-ns`、`prima-store`：基础服务；
- `prima-driver`：COM1 用户态驱动；
- `prima-tty`：tty 字节流服务。

### 6.3 已实现的完整原生数据路径

`prima-smoke`：

- 创建 4 页 MemorySegment；
- 映射 8 KiB 数据窗口；
- 写入模式数据；
- 通过 IPC 只发送 op/length；
- 服务管理器映射同一 Segment 并计算 FNV-1a；
- 客户端核对结果；
- 验证执行前取消、越界长度拒绝和恢复重试。

阶段 4 还验证了：

- 独立 ring3 服务；
- 最小权限拆分；
- 端点独立空 Namespace；
- 驱动越权写触发 `#PF`；
- 驱动 Faulted 后无关客户端继续运行。

### 6.4 尚缺

- 通用 `exec` 和应用安装/启动器；
- 从文件系统加载任意程序；
- 通用同步 call/reply、timeout、backpressure；
- 通用标准服务协议；
- 通用文件/fd/VFS/socket；
- Process/Thread/信号/wait；
- 动态加载；
- 资源配额、服务自动重启；
- SMP；
- 面向外部开发者的原生 SDK。

---

## 7. Shell

### 7.1 当前状态

**没有可交互 shell。**

当前串口存在的是：

- 调试输出；
- `prima-tty` 只写 tty 服务；
- 服务管理器自动启动固定程序；
- 固定测试探针和 sbase `echo`。

这些都不是 shell。

### 7.2 缺什么

- fd 0/stdin；
- 可读 tty；
- 行规程、行编辑、Ctrl-C/Ctrl-D；
- 文件系统和 rootfs；
- 目录、PATH、cwd；
- `fork/execve/wait4`；
- 从文件加载 ELF；
- 进程/退出状态/父子关系；
- 管道、重定向、作业控制；
- 一个 shell 程序本身。

### 7.3 应该在哪里实现

**shell 不进入内核，也不进入 Linux syscall 翻译核心。** 它可以有两种形态：

1. **原生最小 shell**：`prima-shell`，通过 `prima-runtime` 调用原生服务；适合调试、恢复和系统管理。
2. **Linux/POSIX shell**：作为普通 ELF 程序运行在 Linux LibOS 上；适合复用现有 shell 和工具生态。

推荐双 shell 路线：

```text
原生 shell：管理原生服务、Capability、系统状态
Linux shell：兼容 Linux/POSIX 应用和脚本
```

两者均属用户态应用，不特殊化内核路径。

---

## 8. 虚拟机运行

### 8.1 可以运行

PrimaOS 当前可在 QEMU x86_64 中运行：

```bash
cargo xtask build
cargo xtask run
cargo xtask test
```

QEMU 配置大致为：

- `qemu-system-x86_64`；
- `-display none`；
- `-serial stdio`；
- 无网络；
- raw disk image；
- `isa-debug-exit` 用于测试退出。

### 8.2 当前运行形态

启动后自动执行：

```text
Limine → Prima 内核 → ring3 init → 服务管理器
→ 基础服务 / 驱动 / tty → 原生应用 → C 测试程序 → 验收 → 退出
```

它是一个自动验收镜像，不是桌面或可登录系统。

### 8.3 其他虚拟机

仓库验证目标是 QEMU。理论上是 x86_64 启动镜像，但没有 VMware、VirtualBox、Hyper-V 或真机启动证据，不能宣称已在这些环境验证通过。

---

## 9. 桌面组件是否需要原生开发

### 9.1 推荐结论

**桌面核心原生，桌面应用不必原生。**

核心原生组件：

- display server / compositor；
- input 服务；
- session manager；
- window manager；
- GPU/display 用户态驱动适配层；
- clipboard、drag-drop、屏幕捕获等安全策略。

不必强制原生：

- 浏览器、编辑器、终端、文件管理器、办公软件；
- 现有 Wayland/X11 客户端库；
- 字体、编解码、软件渲染器；
- 现有桌面应用和工具链。

### 9.2 目标形态

```text
原生应用       Linux LibOS 应用       其他 LibOS 应用
     \               |                  /
      \              |                 /
       统一的 surface / input 协议
                  │
          原生 compositor / display server
                  │
          GPU / framebuffer / input 驱动
                  │
        Device / MMIO / IRQ / DMA Capability
```

所有 LibOS 共享同一个桌面服务，而不是每个 LibOS 各自实现桌面。

### 9.3 关键接口

- surface 创建/销毁；
- 共享 Buffer；
- frame 提交和 damage；
- 输入焦点；
- 键盘/鼠标/触摸事件；
- 窗口层级和生命周期；
- 剪贴板和拖放；
- 屏幕捕获权限。

内核不应知道窗口和桌面语义。

### 9.4 当前状态

- 没有图形子系统；
- 没有 framebuffer 服务；
- 没有输入服务；
- 没有 compositor；
- QEMU 当前 `-display none`；
- 图形/Windows NT ABI 明确不在前三阶段兼容承诺内。

建议顺序：

1. framebuffer 控制台；
2. input 服务；
3. shared Buffer / surface 协议；
4. 最小 compositor；
5. 客户端库和 Linux LibOS 适配；
6. panel/launcher/session manager；
7. GPU 加速和 3D。

---

## 10. 适配层是否应该由 LibOS 提供

### 10.1 结论

**personality-specific 适配层应由对应 LibOS 提供；OS 中立的公共基础设施应由共享基础库提供。**

不能把 Linux/POSIX 语义放进内核，也不能把全部适配职责塞给某一个 LibOS。

### 10.2 推荐分层

```text
应用
  → libc / GUI toolkit / 语言运行时
  → LibOS personality 层
       - syscall 解码
       - fd 表、进程/线程、signal
       - errno、Linux/POSIX 语义
  → 共享基础库
       - ABI/IDL
       - IPC 编解码
       - Capability 包装
       - MemorySegment/Task/Namespace 客户端
       - 公共 tty/VFS/display 协议
  → Prima 原生 syscall
  → 内核机制层
```

### 10.3 责任划分

LibOS 负责：

- Linux syscall 号和寄存器 ABI；
- fd、dup、close；
- fork/execve/wait4；
- signal/进程组/作业控制；
- TLS/futex；
- Linux errno；
- `/proc`、`/sys`、`/dev` 视图；
- Linux 网络语义；
- `compat-state` 和偏差记录。

共享基础库负责：

- syscall 传输封装；
- IPC 消息编解码；
- Capability 包装；
- MemorySegment 映射辅助；
- Namespace 客户端；
- 公共服务协议；
- 版本协商和 trace。

### 10.4 GUI 适配层

```text
Linux 应用
  → Wayland/X11 客户端库
  → Linux LibOS GUI 适配层
  → Prima 原生 display/input 协议
  → 原生 compositor
```

compositor 核心是原生服务；Linux LibOS 只提供客户端语义适配，不重新实现一套桌面。

### 10.5 当前代码

- `prima-runtime` 是原生应用 API，不是通用 LibOS 适配层；
- `prima-user/c/prima_syscall.c` 是当前 C 兼容原型，同时承担 Linux 号映射和 Prima 传输；
- `prima-servicemgr` 是服务注册/路由，不是 LibOS；
- 阶段 6 才计划真正的 Linux static ELF LibOS 和 syscall trap 递送。

未来应拆分为：

```text
共享 abi/ipc/runtime 基础设施
+ Linux LibOS personality
+ musl backend
+ 公共 VFS/tty/display 服务客户端
```

---

## 11. 综合架构判断

### 11.1 已经具备的底座

- Capability、Task、IPC、MemorySegment；
- Namespace 路径授权；
- 用户态服务管理器；
- ring3 服务和用户态驱动；
- 共享内存数据路径；
- 真实 trap/page fault/COW/TLB 机制；
- 静态 C/POSIX 纵向切片。

### 11.2 尚未具备的关键系统能力

- 通用 VFS/rootfs；
- 可读 tty 和 shell；
- 通用进程/线程/信号；
- 通用 mmap 和文件映射；
- 动态链接；
- 网络；
- 图形/输入/桌面；
- 资源配额、撤销和故障恢复；
- 完整多 LibOS 互操作协议；
- 系统性能实测和回归基线。

### 11.3 设计主线

```text
内核：机制与隔离
用户态服务：文件、网络、驱动、显示、策略
LibOS：OS personality 和 ABI 语义
应用：普通用户态程序
```

内核不因为某个 OS personality 而增加专属文件、socket、窗口或桌面对象。

---

## 12. 相关文档入口

| 主题 | 文档 |
|---|---|
| 总体架构 | [`Prima OS.md`](../../Prima%20OS.md) |
| 原语语义 | [`COSIR 原语规范.md`](../../COSIR%20原语规范.md) |
| 当前实现状态 | [`Prima OS 实现阶段.md`](../../Prima%20OS%20实现阶段.md) |
| 阶段 5 profile | [`stage5-profile-v0.md`](../superpowers/specs/2026-10-18-stage5-profile-v0.md) |
| 阶段 5 计划 | [`stage5-revised-plan.md`](../superpowers/plans/2026-10-18-stage5-revised-plan.md) |
| 兼容状态 | [`compat-state.md`](../superpowers/compat/compat-state.md) |
| 文档总索引 | [`docs/README.md`](../README.md) |

---

## 13. 维护说明

- 本文是对话整理，不替代 spec、report 或阶段跟踪；
- 实现进展变化时，优先更新实现阶段跟踪和对应报告；
- 本文只在架构解释发生实质变化时同步更新；
- 新增具体设计请放入 `specs/`，执行计划放入 `plans/`，证据放入 `reports/`；
- 不要把当前 `partial` 或 `planned` 的能力写成已支持。
