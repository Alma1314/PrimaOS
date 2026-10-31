---
id: report-stage6-exec-frame-capacity
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# S6-M2：526 页映像、额度失败与大映像再次 exec

## 结论与范围

在干净基线 `06f40dc` 上，移除原生 exec 的149页映像限制，改为包含新栈的640页私有
预算；补齐全局帧、段表与逻辑映射预算。大帧向量复用外部 arena，保持每帧身份与顺序。
新增 W7 真实载入526页、逐页检查、部分分配失败、四次回滚及大映像→大映像再次 exec。
本轮按[容量契约](../specs/2026-10-31-stage6-exec-frame-capacity.md)验证，不代表冻结
busybox 已运行，不完成大文件 VFS、LibOS、G1–G3 或任何阶段6退出条件。

上游为[阶段6–7计划](../plans/2026-10-27-stage6-7-refinement.md)、
[COSIR 帧向量/归属](../../../COSIR%20原语规范.md)、
[装载器硬化及旧限制](2026-10-28-stage6-loader-slice.md)、
[输入与失败退栈回收](2026-10-31-stage6-streamed-elf-input.md)。旧失败与旧容量记录保留，
本报告只在上述范围提供后继。工具链为 nightly-2026-07-30，依赖离线；QEMU 沿用
BIOS/TCG/qemu64/单CPU/128 MiB 与30秒门限，没有 KVM、SMP、硬件或性能出口主张。

## 预算与表示

| 项 | 原值→本轮值 | 理由与边界 |
|---|---|---|
| 普通/平台 memseg | 149→149页 | 不扩大用户单段分配权 |
| boot 服务向量 | 192→192页 | 外部池前192槽专用，不能被 exec 借走 |
| 私有 exec | 149→640页（含栈） | 冻结制品526载入页+8栈页=534；不是每任务保证 |
| 数据帧归属 | 640→2048项 | 文件源、旧/新映像、基础服务及 retained/COW 共享；保守算式320+526+534+534+96=2010 |
| 逻辑映射 | 1024→2048条 | 映像与文件源可以共存；延迟页表回收队列随既有公式关联扩大 |
| Segment 对象/归属标签 | 48→64 | 基础对象、分块文件源与并存的暂存对象需要余量；复用既有 u64 标记，新增编译期上界断言 |
| 可归还帧栈 | 896→2304项 | 数据2048+页表256，全部归还也须有位置 |
| 页表 / kernel image | 256 / 2048，均不变 | 仍可因独立额度而拒绝准入 |

`SegmentFrames::External` 的实例句柄/起点/长度引用真实 FrameId 向量，不把它换成连续
物理区间。大 exec 使用 boot 区之后的2048个元数据槽，first-fit 只在独占 State 区间执行；
失败清理实际已填前缀，销毁清理自身 run。物理帧0仍是有效项。碎片可能拒绝某次准入，
不承诺总空闲数足够就能成功，也不保证多个最大映像/服务组合可同时运行。

为了容纳扩大后的归还栈，kernel image 的物化向量移到独立静态数组，以原有 owner-region
标识每个注册表实例。新增实例隔离/失败初始化检查；保留逐帧重复与跨类重叠判定。
注册表副本引用同一存储，与既有 owner arena 一致，不能充当深拷贝事务快照。

实际大小为 BootstrapState=13,312 B、SegmentFrames=1,208 B、FrameRegistry=25,184 B、
FreeFrameAllocator=22,560 B（[实测](#关键输出摘录)）。未扩大13 KiB
State、32 KiB注册表的守卫，也未改变32 KiB syscall栈及其guard；这不是所有路径峰值栈证明。

全局归属表是集合，可用末项补空位；Segment 的有序向量独立保存。整段释放改为一次扫描，
普通单帧释放也不再移动所有后项，分配失败与销毁共用回收路径。分配器的可失败检查仍先于
归属删除。物理帧再次分配次序可以变化，不承诺复用次序；有序映射和归属不能因此改变。

## W7 实际检查

原有 `exec/wait/bad/stream` 保留，新增 `large=1`，完整 stdout 必须逐字节等于：

```text
EXEC|exec=1;wait=1;bad=1;stream=1;large=1|ok=1;err=0
```

合成文件为526页（2,154,496 B），单个 RX PT_LOAD 覆盖完整文件，526页末尾分别写
u64值1…526，入口位于末页。源由四个≤149页普通段提供；新私有 Segment 实际分配534页，
跨越页表的2 MiB边界。目标入口逐页比较这些不同 marker，不能只靠首/末页退出。

原生分支必须依次完成：

1. 普通段150页、总 exec 641页按原生机制错误拒绝，旧程序仍执行。
2. 六个149页未映射普通段向真实全局帧额度施压，大映像部分分配后返回 ENOSPC；
   随后销毁/丢弃压力段，继续使用同一 Task 和文件源。
3. 解除文件第149页，连续四次 exec 返回 EFAULT，旧栈 sentinel 与源 marker 保持；
   每次此前已建立534页暂存 Segment。若丢失一个534槽 run，第四次前元数据池就不足。
4. 恢复源页后载入完整526页。新程序校验所有页、释放原输入段，再把自己现有的只读
   完整映像作为 exec 文件源，构造另一个534页映像；argc从2变1，第二次入口再校验
   全部526页并以42退出。旧大映像在新映像构造完成前仍存活。

Linux 同源只运行同形526页内容检查并退出42；原生额度、故障与再次 exec 分支不在
Linux执行，不把这些输出位当成 Linux errno/进程语义差分。新探针不使用 musl/TLS 启动。

## 失败与整改链

- 首版合成入口的条件跳转偏移错误，Linux子进程SIGSEGV，严格对照拒绝 `large=0`
  （[输出摘录](#关键输出摘录)）。修正字节偏移后主机对照通过。
- 初版压力探针首先耗尽48个 Segment 槽，退出124，`large=0`，未触达要求的帧耗尽
  （[输出摘录](#关键输出摘录)）。同步补齐段表及标签预算到64；
  validator仍严格检查配置值，未接受缺失、任意值或放宽其它断言。
- 段表补齐后，原逐页删除归属表的实现使两个QEMU轮次都触发30秒超时
  （[输出摘录](#关键输出摘录)）。没有增加超时或降低
  页数/失败次数；改为有界批量扫描、末项补位并统一失败回收后，W7双轮通过。
- 测试诊断打印一度缺少 `super::BootstrapState` 类型限定，编译检查拒绝；修正后重新
  完整运行workspace和lint。最终通过结果不混用上述失败构建。

## 命令与结果

| 命令 | 本次结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1055 passed、0 failed、1项外部ELF样本ignored |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机编译、35个内核文件零unsafe |
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib stays_within -- --nocapture` | PASS：2个现有值大小守卫 |
| `CARGO_NET_OFFLINE=true cargo xtask test-process W7` | PASS：Linux参考与QEMU双轮，原30秒门限，串口文件逐轮覆盖，非两轮合集 |
| `CARGO_NET_OFFLINE=true PRIMA_ELF_SAMPLE=/usr/bin/busybox cargo test -p kernel --lib exec::tests::plan_accepts_operator_supplied_static_linux_elf -- --ignored` | PASS：1个解析测试；样本SHA-256及程序头与冻结台账一致；不执行该制品 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：最终单次命令exit=0，36程序/72轮及全部Linux参考 |
| 文档四项与Git空白检查 | PASS：self-test/build/check/check --base 06f40dc，339文档/3324章节 |

host补强了外部 run 的非连续物理帧顺序、零帧、实例隔离/复用/耗尽、原有用户边界与
不变的值尺寸；帧注册表测试覆盖交错归属销毁、精确额度、2048数据帧和256页表帧全量归还
并重新分配。上述host范围与QEMU执行分别记录，不推导任意可达状态的证明。
关键输出内嵌于下文；不提交镜像/第三方二进制。

## 阶段影响与限制

关闭的是原生 exec 私有映像149页限制及其配套有界预算，不是所有装载资源限制。
冻结busybox的4段/非对齐/BSS几何、libc/TLS/信号与8页初始栈仍须取得实际装载/运行证据；
完整VFS大文件来源和可信停车入口仍待实现。Gate能力/陷阱/rax答复、G1–G3与Linux LibOS
不因本轮而完成；没有修改白名单、profile或73–78候选号。当前状态只见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

## 关键输出摘录

以下片段来自本报告原验证记录，仅内嵌支撑结论的输出；本次文档整理未重跑这些验证。
完整旧附件的恢复版本及目录见[日志整理记录](2026-10-31-documentation-log-inline.md)。

原 `sizes.txt` 的关键行：

```text
BootstrapState=13312 B; SegmentFrames=1208 B
FrameRegistry=25184 B; FreeFrameAllocator=22560 B
```

原 `fixture-jump-failure.txt` 的关键行：

```text
ERROR exec-linux mismatch: exit Some(1), stdout "EXEC|exec=1;wait=1;bad=1;stream=1;large=0|ok=0;err=11\n", stderr ""
```

原 `segment-quota-failure.txt` 的关键行：

```text
EXEC|exec=1;wait=1;bad=1;stream=1;large=0|ok=0;err=31744
```

原 `timeout-before-reclaim.txt` 的关键行：

```text
ERROR BIOS W7 run 1 failed: QEMU timed out after 30s; QEMU child terminated
EXEC|exec=1;wait=1;bad=1;stream=1
BIOS W7 run 2 failed: QEMU timed out after 30s; QEMU child terminated
```

原 `W7.txt` 的关键行：

```text
PASS qemu bios W7 run=1
PASS qemu bios W7 run=2
```

完整套件 W7 末轮实际输出；旧串口文件逐轮覆盖，不是双轮合集：

```text
EXEC|exec=1;wait=1;bad=1;stream=1;large=1|ok=1;err=0
```

只作解析的冻结输入身份：

```text
dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14  /usr/bin/busybox
```

原 `full-suite.txt` 的关键行：

```text
PASS full BIOS suite: 36 programs, 72 runs
```
