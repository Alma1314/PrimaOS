---
id: report-stage6-managed-linux-loader
kind: report
authority: scoped-implementation-progress
status: built-not-runtime-verified
lifecycle: historical
updated: 2026-11-01
---

# 阶段6续做：停车装载、用户态LibOS与冻结制品镜像

## 结论与范围

承接[上一增量](2026-11-01-stage6-measurement-and-boot-span.md)，基线仍为
`4335e184ef5dd10679d72ecf6a0189b7672d9cd2`加本工作树未提交修改。
本轮已接线受管理的初始装载、用户态Linux personality和六applet差分工程，并构建Linux专用BIOS/UEFI镜像。
**没有执行host测试、QEMU或参考Linux程序，不能宣称任何白名单成员已经运行。**
用户要求全部实现完成后统一测试；本轮只进行编译、Clippy、构建和静态工件检查。
P1/P2、futex、M4进程/线程/信号后继、逐调用差分和整体退出评审仍未完成，阶段退出保持0/4。

契约入口：[受管理的停车/装载](../specs/2026-11-01-stage6-managed-launch.md)、
[现行Gate协议](../specs/2026-10-29-stage6-m1-protocol.md)、[D17–D22](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)、
[D12白名单](../compat/stage6-static-whitelist.md)。[Linux profile v1草案](../compat/stage6-linux-profile-v1.md)
列出代码变体与限制；本轮不冻结profile、不改阶段5签署、不提供新的形式证明。

## 内核与运行时

1. 追加79–82：创建launch-held Prepared任务、从服务内存装载目标私有映像、原子Gate挂接/发布、
   按管理权限设置目标FS base。原73–78、rax-only、独占root、信号保留及1–72冻结号保持。
   `launch_held`区分初始停车与prepared fork；后者仍受ticket协议保护。
2. 装载复用原生exec的计划、八页栈、按需源读取、640页私有Segment与回滚路径。
   先验证/分配再替换目标root，失败清理在大构造栈退出后进行；成功仍Prepared。
   发布前核对实际用户入口/栈页、栈逻辑写权和独占root；挂接失败保持停车。
3. 绑定映射修改仅允许实时Delivered请求的固定服务者，同时仍需TaskManageVSpace；
   其它管理者、Idle/Queued/终结后/孤儿状态拒绝。逐调用重新检查root、cookie、逻辑映射和PTE；
   不跨native调用保存物理写计划，不开放共享root或更换绑定root。
4. 79–82的普通错误先记录原始尝试，容量不足全局fail-stop；启动合并预检挂接与Ready迁移成本。
   追加中立TaskImageLoaded/TaskTlsSet审计；TaskInfo selector8暴露当前已有结构性Namespace句柄。
5. 新增停车不可调度、错误持有者/错误Gate失败保留、prepared fork混淆、重复启动、
   Delivered身份/终结边界、TLS规范地址和启动审计耗尽的host检查。它们只编译，未执行。

Lean和COSIR Gate模型不含这些完整新增迁移，已在[映照遗漏](../../../lean/README.md#已知不覆盖后续切片)登记。
新增内核路径不解释Linux号、errno、fd、stat或ELF的OS人格；本personality的这些语义都在prima-user。
这不代替对既有原生适配器的全树S6-X3审查。

## 大制品载体与发现的旧注入问题

同一D12 busybox原字节嵌入原生只读服务映像，源由装载服务交给通用Task loader；
不重链接、不补丁、不改外来ELF。聚合boot服务的外部向量192→768页，普通Segment149、
私有exec640、全局数据2048和页表256的预算不变，没有增大内核栈内联帧数组。
普通RAM VFS仍为2920字节内容区；该路径不证明大文件VFS execve。

首次v2镜像打包成功后静态检查发现：旧`prepare_task_create`会将Task4机器码写入首个服务的
`+0x800`。本轮服务在该处有实际重定位数据，不能视为空闲空间。尚未启动该镜像，
所以没有“它因此崩溃”的运行证据，也不能把数据覆盖等同于已经观察到的故障。

修正为通用manifest v3的显式平台见证预留位：`magic,3,count,flags`。
v3 flags=0不注入，未知flag拒绝；v2保留历史约定。Linux专用镜像使用v3、2个条目、flags=0。
此选择不识别服务kind或OS；旧普通镜像继续v2。新增格式/截断/未知位检查只已编译。

最后专用服务的PT_LOAD虚拟跨度为541页，聚合含目录542页，低于768；冻结busybox仍需526载入页，
配八页栈共534页。两者不能把文件大小和虚拟跨度混算。内核去调试节文件约2.5MB，
完整BSS跨度仍由PT_LOAD构造自建根；这只是静态工件检查，不是启动成功。

## 用户态LibOS与工程链

`prima-user/src/linux.rs`复用VFS、OpenFiles、私有状态、Gate代理复制与请求答复。
`linux_main.rs`为六个applet分别创建停车客户端；每次先检查未映射栈启动失败、坏ELF失败后仍Prepared，
再装载真实制品、挂接、等待和服务。退出后销毁客户端，再回收fd与匿名段。
服务使用本身的设备能力输出，不向客户端泄露串口或管理权限。

已有接线包括有限文件/目录、stdio、brk/匿名映射、RELRO权限收缩、FS设置/读取、身份与退出；
具体变体和未实现项只按[profile草案](../compat/stage6-linux-profile-v1.md)解释。
未知号/未实现变体返回-38并走Gate abort；服务输出cookie/number/result记录。
宿主要求原始记录数与终结计数一致，但本轮没有执行它，也没有导出并复核整段内核审计。

`build-linux`只校验D12哈希、核对服务内嵌完整原字节并打包。
`test-linux`已接入pinned Linux 6.8.0-31 guest上的同一制品，以及Prima侧两轮：
严格比较六项顺序、stdout/stderr与退出0，拒绝缺失、重复、失败、乱序和未支持计数不符。
保持Prima 30秒门限、参考guest独立180秒门限；无宿主结果替代guest、无重试追PASS。
当前比较是applet观察，不是逐syscall完整差分；参考输入为C locale、相同fixture字节和32KiB栈限制。

## 本轮命令与实际结果

| 命令 | 结果 | 范围 |
|---|---|---|
| `cargo fmt --all` | 退出0 | 格式化 |
| `CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings` | 最终退出0 | 普通配置静态检查 |
| `CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G4 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings` | 最终退出0 | Gate配置静态检查；未执行计量 |
| `CARGO_NET_OFFLINE=true cargo check --workspace --all-targets` | 退出0（早期停车入口版本） | 编译，非执行 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo clippy --workspace --all-targets --features xtask/linux-probe -- -D warnings` | 退出0 | 含Linux入口、宿主反例和harness；非运行验收 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -p xtask --features linux-probe -- build-linux` | v2、v3均退出0；最终为v3 flags=0 | 源校验与BIOS/UEFI打包，不运行QEMU |

最终v3构建输出摘录：

```text
PASS build kernel
PASS image bios
PASS image uefi
```

最终打包工件SHA-256（均为ignored产物，不入库）：

```text
dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14  stage6-pinned-busybox.elf
988b7f7ac2ca9583a0da0bfac6f2887c52e20b34afac2e0536674021201bde64  stage6-native-service.elf
1ecfc64bf0940fd2259bdda4a8573ede5aee13900ad0614e38155ceaee57c0b4  prima-kernel-boot.elf
574c9a2ade6cc3f5bcf4b1c3ccb044284814771a8fb992f56fdf1b13a0f9508c  prima-os-ramdisk.img
c4cd1a7b09620a04ace0532f8cf5776555442e881dd9a62e090442d2c77b67b4  prima-os-bios.img
```

原始构建输出留`/tmp/prima-stage6-linux-build*.log`，镜像在`target/prima-os/`。
未运行workspace tests、QEMU、参考guest、完整lint、TCB、Lean与文档self-test；没有耗时或测试PASS计数可复用。
文档build、check及check --base退出0；只表示目录、元数据与链接的静态校验。

## 开发失败与后继

编译期曾发现with_state错误类型不匹配、越过模块边界直接append审计，以及Path/PathBuf签名不一致；
分别修为类型明确的状态包装、所属模块的审计记录方法及通用Path接口。
Clippy曾拒绝手工C字符串和固定chunks_exact，已使用CStr字面量与as_chunks。
普通配置的artifact依赖还曾编译未启用特性的Linux二进制，导致缺少PRIMA_LINUX_ELF编译失败；
现在只有linux-probe配置嵌入输入，普通manifest不选择未配置的二进制，harness也只在特性开启时注册。
这些是开发期失败，后继编译通过不表示原始运行错误已被修复或触发过。

硬件NX覆盖、合法但未实现的syscall变体、P1/P2、futex与M4进程/线程/信号仍需后继。
先完成剩余实现，再按用户顺序统一运行host/Gate/白名单/差分/普通回归并记录全部失败。
本报告不覆盖上一轮W7/strip/I1历史失败，也不关闭M1e或任何阶段退出条件。
