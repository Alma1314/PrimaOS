---
id: report-windows-m2-ring3
kind: report
authority: scoped-qemu-evidence
status: ring3-prototype-pass-limits
lifecycle: current
updated: 2026-11-26
---

# WIN-M2 ring3 原型：原样 PE32+ 子任务执行

## 结论与范围

本切片新增 feature-gated 的 ring3 原型：servicemgr 在专用构建中装载一个由 clang + lld-link
生成的原样 PE32+ 控制台程序，经用户态 `prima-windows` 模型平铺、基址重定位并填充 IAT，
由自建 Segment 与 held Task 发布为 R+X 代码段；PE 调用替代导入桩返回 42，退出桩把 42
写入共享页并自毁。专用 QEMU 命令两轮各自装载两次（重定位基址 + 首选基址），观察
`exit=self`、`child1=42;child2=42;parent=alive`、`PASS` 并以 debug-exit=33 退出。

**未达到完整 WIN-M2 验收**：替代导入是手写机器码页，不是 PE DLL/导出表；没有 Win32
控制台 API 或真实输出缓冲；“另一域”只证明父服务在子任务退出后继续，不是 Linux/原生
personality 并存；发布失败后的残留映射负例、TLS/PEB/SEH 和 Windows 参考差分都未覆盖。

## 基线与输入

- 代码基线：`d22bc28`；本切片的 prima-user、prima-ostd、prima-windows、xtask 与文档在
  提交前的工作树。
- 构建开关：prima-user feature `windows-pe-probe`；服务管理器仅在构建时
  `PRIMA_WINPE_PROBE` 存在时执行原型，默认镜像与启动事件不变。
- PE 夹具：`prima-user/windows-probes/{stub.c,winentry.c}` 经 clang 22.1.4
  （`--target=x86_64-pc-windows-msvc -ffreestanding -O2`）与 lld-link 18.1.3 在 build.rs
  生成；本轮产物 SHA-256 `9885791a9c6e30efb567ce7813f02a271d401aa7cfba18db13872c925760c887`
  （3072 字节，entry 0x1010、image 0x5000、1 个导入 `winpe-stub.dll!Stub`、1 个 DIR64
  重定位）。生成产物留在 ignored `target/`，不随仓库提交。
- QEMU：仓库既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、1 vCPU、128 MiB、
  `isa-debug-exit`。

## 实现内容

| 路径 | 内容 |
|---|---|
| `prima-user/windows-probes/` 与 `prima-user/build.rs` | feature 下生成 stub DLL/导入库与 PE 入口映像，不进入默认构建 |
| `prima-user/src/winpe.rs` | 装载编排：4 个 Segment（PE/exe、栈、结果、导入/退出桩），父侧暂存映射、held Task、`task_start_native`、结果轮询、自毁见证与回收 |
| `prima-ostd/src/mm.rs` | 新增 `bytes_mut_at`：把已映射、可写、独占的用户范围作为可变切片；替代 ring3 不可用的内核 `StaticCell` |
| `prima-user/src/servicemgr.rs` | feature + 构建环境变量下在服务初始化前运行原型，永不返回 |
| `xtask` | `test-winpe` 命令、专用 QEMU 运行器与 `WINPE|` 见证校验；要求构建时 env 与 feature |

地址与权限：PE 以重定位基址 `0x1_5000_0000` 与首选基址 `0x1_4000_0000` 各装载一次；
暂存映射 RW；child 代码段由 `task_create_held` 映射为 R+X；栈与结果页 RW；导入/退出桩
R+X。结果页同时映射到父子，子写父读。没有 W+X 映射；子任务使用内核初始帧的 `rdi`
self cap 调用 `SYS_TASK_DESTROY` 自毁。

## 命令与结果

| 命令/输入 | 本次结果 | 证据范围 |
|---|---|---|
| `PRIMA_WINPE_PROBE=1 CARGO_NET_OFFLINE=true cargo run -q -p xtask --features windows-pe-probe -- test-winpe` | PASS：QEMU 2/2，每轮 2 个 PE 子任务，exit=33 | QEMU ring3 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy、bare-metal kernel、零 unsafe、S6-X3 | host |
| `CARGO_NET_OFFLINE=true cargo clippy -p prima-user --features windows-pe-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo test -p prima-windows` | PASS：PE 32/32、ABI 4/4 | host |
| `CARGO_NET_OFFLINE=true cargo test -p prima-ostd` | PASS：含新增 `bytes_mut_at` 往返测试 | host |

两轮串口日志 `target/prima-os/WINPE-{1,2}.log` 逐字节相同，SHA-256
`b273dbfa5fef6ffc41cab1a2e1b87f56d97df888a14bcc8565e0847d68a9f4a9`；关键行：

```text
WINPE|exit=self
WINPE|exit=self
WINPE|child1=42;child2=42;parent=alive
WINPE|PASS
```

`exit=self` 的判定：结果写入后父服务多次让出，再对 manager cap 调用 `task_destroy`；
若子任务仍存活该调用会成功，观察到错误即表明子任务已自行销毁。42 来自 PE 入口调用
替代导入桩（`value + 1`）后的返回值，不是父服务代写。

## 失败、反例与限制

- 首次 ring3 运行在 `load_into` 处挂起：静态 `StaticCell` 的 bare-metal 锁使用特权 `cli`，
  ring3 不能执行。改为把装载暂存段直接作为输出切片（`bytes_mut_at`）后通过；这是实现
  路径错误，不是 PE 模型错误。
- 替代导入是页面内的 `mov eax, ecx; add eax, 1; ret` 机器码，不是从 PE DLL 解析导出；
  没有 API-set、转发导出、DllMain 或引用计数。
- 输出是共享结果页与冻结串口行；没有 `WriteConsoleW`/stdout，也没有 Win32 错误映射。
- 子任务自毁只依赖初始帧 `rdi` 中的 self cap 和 PE entry 对非易失寄存器的保存；未测
  异常路径或 rdi 被破坏的负例。
- 发布失败清理、“无残留可执行映射”、另一 personality 域并存、资源耗尽、撤销和恢复未验证。
- PE 只装载一次代码/数据快照，全部页面 R+X；没有按节设置 RW、TLS、TEB/PEB、异常目录或
  TLS 回调。
- build.rs 需要宿主 clang/lld-link；该命令只在专用 feature 下运行，默认 boot 套件不受影响。
- 未运行 Windows 参考差分；不能用 Wine 或本机 PE 解析替代。

## 阶段/profile 影响

不勾选阶段 7 条件，不改变 personality 组合或默认 boot 事件。Windows 候选扩展从宿主模型
推进到 ring3 有界原型；WIN-D3（停车/发布、分节权限、用户 GS）、WIN-D2/D5/D6 仍未决，
不能据此宣告 WIN-M2 通过。

## 后继关系

- 前序：[WIN-M2 前置平铺装载模型](2026-11-26-windows-m2-preflight.md)；
  [WIN-M1 宿主模型](2026-11-26-windows-m1-pe-model.md)。
- 后继：用真实 PE DLL/导出表替换手写导入桩；补发布失败无残留映射与另一域继续的负例/正例；
  评审 WIN-D3 后按节设置权限并接入 TEB/TLS；再加入真实 Windows 参考差分。
