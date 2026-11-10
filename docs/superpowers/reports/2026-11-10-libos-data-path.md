---
id: report-libos-data-path
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: historical
updated: 2026-11-11
---

# LibOS 数据通路改造：共享批量文件首片

## 范围与实际状态

基线63c555e2661b371454521a2afe70f5b1010b6765，初始工作树干净。
作者要求全部改造；[完整计划](../plans/2026-11-10-libos-data-path.md)保留P1–P6。
本报告只记录P1共享页批量协议及原生/POSIX读写接线，**完整改造尚未完成**。
本片最终完整BIOS为36组72轮通过；workspace/lint与文档检查结果见下文。
双personality共同文件服务、异步队列、公平背压、Gate优化、多核、撤销/恢复及证明
仍须各自实施和验收，不能由本片host结果替代。阶段7退出勾选保持不变。

## 改动与边界

遵循[实现前契约](../specs/2026-11-10-libos-data-path.md)。
runtime/bulk提供1页、8描述符的显式小端协议；user/bulk把完整私有快照预检后交给既有
OpenFiles逐项处理。Store维护按认证Task incarnation绑定的缓冲，客户端只获映射能力，
序号保持在同一会话关闭/重新建立之间，不因重新OPEN允许旧批次重放。
服务逐项沿用owner检查、访问模式、位置读写/游标及错误映射，不把fd当Capability。
批次不是事务，后项失败不回滚前项。
整块写入返回ENOSPC时，服务内沿用旧3B/2B量子处理并返回部分进展；host与旧路径对照，
保留同样文件内容/长度/OFD游标，不增加跨域分片。

C适配层把普通文件read/write/pread/pwrite接入共享页，单次数据上限3808B；目录解析、
路径及控制仍使用现有协议。信号在单Task缓冲所有权及关闭卸载期间屏蔽；线程使用不同VA。
fork前释放当前缓冲，子会话重新建立；exec保存会话前释放旧映射；正常线程/进程退出释放。
EXE1仍使用原4个u64 session wire，整体2200B，session/forks/termios偏移1800/1832/2120，
以编译期断言防止运行态缓冲字段再次扩大交接；新映像本地缓冲状态清零，重新OPEN取下一序号。
上述布局与基线63c555e的宿主sizeof/offsetof核对一致；不是跨版本ELF机器执行证据。
Store保留尚不能销毁的根能力以便后续回收，不为性能放宽内核对象或审计容量。
没有新增syscall、内核personality语义或unsafe；共享页仍有客户端/服务私有快照复制，
不是端到端零拷贝。本片未修改Linux Gate/RAM fixture路径，也没有性能收益测量。
当前每批仍整页清零/快照/回写，并检查会话存活；小I/O的固定开销尚未分解，
不能由较少IPC推导所有负载更快。C普通fd操作仍沿用进程级文件锁；每Task一页
不构成并行I/O验收。后继性能切片需按数据规模同时测量这些成本。

## 检查记录

原始日志在/tmp/prima-bulk-*与/tmp/prima-r3-*，不入库。下列是本片实际结果，不签收P2–P6。

| 命令 | 结果与范围 |
|---|---|
| cargo test -p prima-user -p prima-runtime bulk（CARGO_NET_OFFLINE=true） | 最终退出0，3个测试；batch预检/重叠/越界/序号、owner/游标/短读/逐项错误及ENOSPC对照；非QEMU攻击验证 |
| cc -std=gnu11 -m64 -ffreestanding -fno-stack-protector -fsyntax-only -Wall -Wextra -Werror prima-user/c/prima_syscall.c | 退出0；语法检查，不是运行证据 |
| cargo xtask lint（CARGO_NET_OFFLINE=true） | 退出0；fmt、workspace/all-targets Clippy、裸机构建与44文件零unsafe、personality边界 |
| cargo xtask test-vfs V4（CARGO_NET_OFFLINE=true） | 退出0；同源Linux参考及QEMU双轮，首版P1功能验证 |
| cargo test --workspace（CARGO_NET_OFFLINE=true） | 最终退出0，1145 passed、1 ignored；含xtask精确基线/负例与ENOSPC对照 |
| cargo xtask test（宿主权限、CARGO_NET_OFFLINE=true） | 最终退出0，36组72轮全部通过；full-suite-errors.log为0B；含V4批量正反例、fork/exec/线程及R3；前序失败/中止独立保留 |
| python3 prima-user/musl/test_port.py | 最终退出0，7 tests；实际适配层wire边界、局部拒绝和null路径 |
| cargo xtask run（CARGO_NET_OFFLINE=true） | 最终退出0；默认轨迹51/119/128，容量134/148/40/1144；此命令本身不运行验收解析器 |
| docs self-test/build/check/check --base 63c555e | 全部退出0，400文档；补录最终结果后重新生成复核 |

## 失败、限制与未完成项

- 初始cargo test --workspace --offline：bootloader嵌套Cargo仍尝试联网，沙箱DNS失败；
  随后获准宿主权限以CARGO_NET_OFFLINE=true重跑。该次运行期间源码发生变更，
  rustdoc使用旧依赖而读取新增bulk模块，报unresolved import，不能当最终版本完整回归。
- 首个bulk host夹具未为根目录开放创建权限，返回Access；修复夹具后2测试通过。
  随后的错误mode字段只属于未通过编译的夹具修订，移除后编译/测试通过。
- 本worktree缺少ignored musl/sbase输入，从同机主checkout复制锁定源码快照，
  交由现有构建器重新验证commit/许可证及重建；未复制旧运行结果作为本次PASS。
- 非合作退出后，若存活Process仍保留客户端映射，Store不能销毁该段；保留根能力并拒绝
  同Task槽位新建缓冲，直到映射释放或Process回收。这是代码边界分析，未作异常窗口机器验收；
  不能将正常线程退出PASS当作强制撤销/恢复完成。
- 没有SMP、异步共享队列、完整恢复或新的Lean证明；版本化batch及直接读写不能
  当作7-A/B/D/G完成。后续仍须解决Linux独立服务接线和明确性能门槛。

## 首轮完整回归发现与修复

默认双轮均观察51/119/128，旧断言为51/123/132；业务输出与退出成功，不能直接记整轮PASS。
原默认file_api_probe的write(6)、read(4)、read(8返回7)合计减少4个分片请求，
即8条消息；新增OPEN/CLOSE往返共4条消息，净减少4次直投/阻塞，排队51不变。
实际容量行caps_live=134、caps_peak=148、segment_slots=40、audit_used=1144；
旧行分别134/146/39/1158。新缓冲增加峰值2个能力和1个段，正常退出后live保持134。
在业务、生命周期和负例验证后按此明确传输契约更新精确断言，容量上限不变。
musl/test_adapter.c仍调用已移除的prima_file_at_arg，须改为实际批量wire/边界检查，
不能通过恢复无生产调用的旧helper使检查空转。

首轮全量探索还出现V4第二轮30秒超时；相同首版此前独立双轮通过，不能据此抹去失败。
补入协议负例探针后，独立V4/Linux参考与QEMU双轮再次退出0；再补512B探针后纳入最终全量重跑。
首轮在W5期间由代理以SIGINT中止（退出130），用于修复已发现的问题；期间开始补丁，
故整轮不是固定最终版本的验收。该轮其余PASS仅作探索记录，不替代最终固定源码重跑。
修复包括旧codec主机测试接入实际新wire、保留ENOSPC部分进展、关闭映射时屏蔽信号，
以及新incarnation序号重置边界和CLOSE保留位检查；没有增加超时或放宽安全/容量断言。

第二轮全量在V2阶段中止（SIGINT/退出130）：代码复查发现运行态session扩展会扩大
EXE1交接而保持同一版本标记，旧新映像跨版本exec存在提交后失败风险。将交接session
与运行态结构分离，保留基线2200B及既有偏移；C语法/7项musl检查再次通过。
此前测试均未触发该风险，只执行了同版镜像，不能宣称跨版本验收；修复后再次完整重跑。

## 完整回归超时调查与启动包修复

第三轮固定版本完整BIOS退出1：71/72次运行通过，仅R3第一轮30秒超时，第二轮通过。
超时前RUST posix/fs/env/udp/thread=1和ECHO status=0已输出，停在authority_ready之后。
不将业务成功替代完整验收。独立R3重跑第一次被沙箱回环网络权限拒绝；按要求转宿主权限，
随后又在seq11之后超时，不能判定为同一数据面死锁。

同配置的HMP/串口文件诊断实例退出33；分段计时24.689s，其中20.369s才进入内核，
frames_ready距authority_ready约0.187s。该样本将主要延迟定位到内核启动前，
不是对Native/LibOS文件I/O延迟的测量，也不据此宣称所有超时根因均已证明。
检查7341178B的ramdisk：8个服务各含约0.80–0.84MB非装载数据，主要为调试信息。

xtask对8个编译服务复用现有strip_debug（保留Cargo原始调试制品），ramdisk降为884626B。
逐个比较entry、程序头及PT_LOAD载荷，8/8一致；比较时仅归一化ELF头中节表位置/数量字段，
这些元数据由objcopy随剥离重写。没有剥离raw Linux ELF或C/Rust应用，也没有变更加载器。
修复后cargo xtask test-rust R3（宿主权限/CARGO_NET_OFFLINE=true）退出0，参考及QEMU双轮通过；
30秒阈值未改。最终完整BIOS须在此修复后再次运行，不能用针对性PASS覆盖先前整轮FAIL。

同一诊断命令（python3 /tmp/prima-r3-timing.py，临时脚本，轮询串口文件记录monotonic时间；
沿用qemu_args的单核TCG配置，仅改为monitor stdio/serial file以诊断）在剥离后
退出33、8.358s完成、5.563s进入内核。前后各一个样本，不是统计性能验收。
标准R3双轮已通过；后继完整回归使用原monitor none/serial stdio及30秒阈值。

## I1输入合并与严格消息归账

剥离后的第四轮完整BIOS退出1：71/72轮通过，唯一失败为I1第二轮用户消息总数
420对首轮423；业务摘要两轮均status=0、141B/hash3712305136，接收归账等式均成立。
两轮queued/direct/blocks分别84/339/353与84/336/351，kernel/UART分别3/3与4/4，
interrupted/retired/faults/live均2/1/0/8。该次不能记完整PASS。

`stage5_ttyin.c`原raw循环请求剩余2B，UART时机不同会使`xy`由一次或两次read完成。
每次`prima_tty_read`发送请求、接收回复、发送ACK，共3条用户消息，符合420/423差异。
将用例固定为两次单字节read，仍检查raw模式及完整`xy`，避免把输入合并当作消息丢失。
生产TTY协议、信号/取消/重启/EOF检查和严格`validate_interactive_ipc`均未放宽。
历史报告的423是历史用例的实际观察，不保证可变调用次数下恒定；保留原报告。
修复后`cargo xtask test-tty`退出0，同源Linux参考及QEMU双轮通过。
第五轮固定代码完整BIOS退出0，36组72轮全部通过，错误汇总为0B；日志
`/tmp/prima-bulk-bios-fixture-fixed.log`。该轮I1分别84/339/353、83/340/353，
kernel/UART分别3/3、2/2，其余终态2/1/0/8；两轮用户消息均423且严格归账。
R3亦双轮通过。第五轮不覆盖或删除前四轮的失败/中止记录。
