---
id: report-stage7-native-syscall-benchmark
kind: report
authority: scoped-implementation-and-validation
status: native-roundtrip-locally-verified
lifecycle: historical
updated: 2026-11-07
---

# 7-0 N1原生syscall基线与G5远端后继

## 结论与范围

新增N1探针：未绑定Gate的Task 2在真实用户态执行SYS_TASK_ID（7）int80往返，
两次独立启动各128条完整u64端点和返回回执，逐次返回2。
复用[G5的用户计时发射器](../specs/2026-11-07-stage7-gate-user-benchmark.md)，
不增加生产ABI、映射或内核采样钩子。[N1实现前方法](../specs/2026-11-07-stage7-native-syscall-benchmark.md)
明确user-wrapper、dev/QEMU、样本数和严格接受条件。

本轮七配置14次启动、host1137项、lint/特性Clippy及普通BIOS回归通过；
原始回执、哈希和分位数独立核验通过。
上一提交fe83e59的干净远端九组及G5原始证据亦已核验，补强其
[先前仅本地的报告范围](2026-11-07-stage7-gate-user-benchmark.md)。
原生syscall与Gate两类各有有界单核基线；IPC、页故障/COW、端到端LibOS、
2/4核、数值门槛和发布检查仍待独立完成。九轨退出均不改变，不新增形式化或兼容profile主张。

## G5干净远端证据

本次读取[Actions 42345719829](https://github.com/Alma1314/PrimaOS/actions/runs/42345719829)，
整体及九个job均success，head为fe83e597d5f636e4503f28eabbe876eda5d9ca98。
命令为gh run view 42345719829 --json status,conclusion,headSha,jobs,url，
以及gh run download 42345719829 --dir /tmp/prima-stage7-g5-remote，均退出0。
九份manifest均clean且revision一致，源摘要：
5c6bca735615e6c1ed72ec3e2edfe9b2d5dd3736be8102ee65999425da5c39e0。
Python逐份核对全部步骤状态/退出0、九组集合、88份归档长度/SHA-256，及G5两轮端点/分位数，退出0。

| 组 | manifest运行ID | 秒数 | 原始归档 / 最终制品身份数 |
|---|---|---|---|
| host | 5934abc7ca1d4c0ea7b5ce800b3cc0fa | 198.718 | 0 / 0 |
| lint | 0e5426453c084236a20c4a8672679b76 | 160.128 | 0 / 0 |
| docs | 3bfc2f4ae19e44778775b4bccb96be06 | 9.807 | 0 / 0 |
| bios | d9e757f901d441689ce62eecca8f6ef6 | 968.530 | 38 / 38 |
| linux | ea5433dcacc640479e0ad5218ae988c6 | 252.146 | 22 / 12 |
| gate | da27ec0806384df19a9f1a7bcea19d89 | 291.071 | 12 / 5 |
| gate-audit | 09c5c227dfdf4bab9c5fd947d27d12fe | 263.393 | 14 / 5 |
| domains | e370b48f454a48c5986691c00c13d2d1 | 158.404 | 2 / 5 |
| formal | 6d06953fc85f4826801244f5dd428e32 | 383.833 | 0 / 0 |

BIOS仍为36组72轮，含默认启动双轮；归档数含空的错误汇总日志。
Gate为原六配置双轮，尚不包含N1；domains仍限D14人格桩，formal仍为既有映照范围。
镜像是manifest中的身份登记，Actions未上传镜像字节，不声称本地可复核远端镜像文件或跨宿主重现相同字节。
G5两轮各128条七点样本通过独立有序/覆盖检查，八组分位数均与步骤06.log的摘要精确匹配。
独立user-total的p50/p99为1460030/1607628 ns（2438236317 Hz）及1429207/1695179 ns（2435244862 Hz）。

| G5远端项 | SHA-256 |
|---|---|
| traces/06/S6GATE-1.log | 3f3f93e4c73149bf137db414433bb36085a1dd2dd34fdf4a8b75101fa86a25f7 |
| traces/06/S6GATE-2.log | 3d9e409eacc74c6a7b26d0d305abefbbadd92dabcf72ea517888d215d4fed1a0 |
| 最终G5 BIOS镜像身份 | 4113b716a76dff2169e2af9f8d6c205c413c41ea5042fde5a7a72a72a82312a4 |

这组数据独立于先前本地G5样本，不混入相同统计总体，不据宿主差异宣称优化收益。

## N1实现及调度检查

kernel/src/native_bench.rs只在gate-probe内使用。程序生成前清除bring-up遗留的Task 2续体，
检查未绑定Gate并保存审计起点；校准后通过sched::switch_to从Task 1迁移Running/Ready及当前身份，
再恢复Task 2，沿现有生产int80/serve/iret路径执行。
不能用resume(2)代替switch_to(2)：resume只恢复上下文，不更新当前Task或模型调度状态。

每条样本为起点、终点及返回值，占24字节；128条3072字节位于原私有RW页，编译期检查页界。
用户端先LFENCE/RDTSC、存起点、准备六个零参数/号7、int80，返回后先保存rax到r15、取终点；
回执保存和逐次值检查放在终点之后。结束验证当前Task=2、全部回执=2、有序端点、
Gate/完成等待良构、无绑定及未新增GateTrapped/Delivered/Terminated/ControlRejected审计。
用户初始IF=0，无宿主独占负载保证，不主张抢占/竞争负载性能。

S7NATIVE/S7NTIME与G4/G5记录分开；宿主拒绝缺失/重复/错误头、计数/索引/列数、
回执错误、零频率/起点、时钟回退和panic。
三类测量复用整数分位数格式化函数，不复制统计实现；原有G4/G5反例继续运行。
CI沿原Gate构建通道在六配置后追加N1双轮，不新增作业或放宽单轮30秒门限。

## 本地运行身份与命令

基线fe83e59；本轮代码、测试与N1方法尚未提交，四组运行均dirty=true、源摘要相同：
713f2809d5d3a9ddd3137130306902f6302712c711f1eda06c232f0548c9ea84。
之后仅补录/澄清文档及生成目录，不回填manifest或将提交身份冒充原运行身份。
原始输出留ignored target/ci与/tmp/prima-stage7-n1-*，不提交完整日志或另附哈希文件。

| 命令 | 退出与实际结果 | 证据范围 |
|---|---|---|
| CARGO_NET_OFFLINE=true cargo test -p xtask benchmark | 0；3项通过 | N1变动时长/回执/格式反例及G4/G5回归，nearest-rank排名64/127 |
| CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=N1 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings | 0 | 含新探针的静态检查，调度修正后再次通过 |
| python3 scripts/ci.py gate | 0；281.051秒；18e176ee87be484e827aee796e73e4f1 | 七配置各双轮、14份原始串口及5个最终N1制品身份 |
| python3 scripts/ci.py host | 0；9.752秒；591a199a70bd4352b8d7fc09d8c3f795 | Rust1137通过/0失败/1默认忽略；musl7、POSIX std3、运行器11 |
| python3 scripts/ci.py lint | 0；8.171秒；cb9a8a5ad207412ab39e75c392cdbbaa | fmt/Clippy、裸机、42文件零unsafe、边界守卫；TCB2087/41034=5.09% |
| python3 scripts/ci.py bios --prepare | 0；1468.697秒；618e4b9d45c4492da4686114519b97a6（准备39.631秒，验收1428.137秒） | 普通36组72轮，含默认启动双轮；38份原始归档 |
| Python独立核验 | 0 | 四组同源、52份原始长度/哈希、43个最终制品文件身份；N1两轮128条回执/顺序及摘要精确匹配 |
| python3 scripts/docs.py self-test / build / check / check --base fe83e59 | 全部0；386文档、3694章节；补录后重建并复查 | 文档元数据、目录、链接/锚点与变更守卫 |

## N1单核基线v0.1

宿主AMD Ryzen 5 3500U、逻辑CPU8、Linux7.0.0-42/glibc2.39，QEMU8.2.2；
pc-i440fx-8.2、TCG单线程、qemu64、1 vCPU、128M、BIOS、默认dev/unoptimized内核。
不扣除计时开销、不丢弃冷样本，串口在采样完成后输出。

| 独立轮次 | Hz | 样本数 | user-total p50 / p99（ns） |
|---|---|---|---|
| 1 | 2084664751 | 128 | 28044 / 135459 |
| 2 | 2085485838 | 128 | 31890 / 86467 |

user-total含起点保存、六参数/调用号准备、完整生产陷入/分派/iret、rax暂存与终点前LFENCE；
不含终点存储及回执存储/比较。它不是单独int80指令成本或全部原生原语的平均性能。

| 本地N1项 | SHA-256 |
|---|---|
| traces/06/S6GATE-1.log（8216字节） | eb92da6e07a19519d5e07541b1e7e2ebe6178d87fe54999ea79d5705a0d65e92 |
| traces/06/S6GATE-2.log（8216字节） | 8e1afb500b0eb3292e7265fc00b01f689123f1a5a46b1fc62d70ab29839ec51d |
| 最终N1 BIOS镜像 | 73bee3f2d3522effefef5d4833010fa1da857219026397d460a4670dff7ebcd9 |
| 最终N1 boot内核ELF | 5d77c45e6d754188f96be875d74bf2ee9a41bef1760ece30fd52cb2d3eadd37c |

独立重算核心（实际核验还检查唯一头、128条完整回执/索引、组内/跨组有序和无panic，
并精确匹配06.log中的两轮PASS S7NATIVE摘要）：

```sh
python3 - <<'PY'
import hashlib, json, re
from pathlib import Path
base = Path('target/ci/gate/18e176ee87be484e827aee796e73e4f1')
m = json.loads((base / 'result.json').read_text())
assert m['status'] == 'passed'
step = next(s for s in m['steps'] if s['env'].get('PRIMA_GATE_CASE') == 'N1')
for trace in step['traces']:
    raw = (base / trace['path']).read_bytes()
    assert len(raw) == trace['size']
    assert hashlib.sha256(raw).hexdigest() == trace['sha256']
    text = raw.decode()
    hz = int(re.search(r'S7NATIVE\|samples=128;hz=(\d+);', text)[1])
    rows = [[int(v) for v in x[8:].split(';')]
            for x in text.splitlines() if x.startswith('S7NTIME|')]
    assert len(rows) == 128 and all(r[3] == 2 for r in rows)
    cycles = sorted(r[2] - r[1] for r in rows)
    print(trace['path'], [cycles[i] * 10**9 // hz for i in (63, 126)])
PY
```

## 中断、限制及后继

首次Gate运行dae16f5615d24296bf0e5e4d68446792在G1完成、G2进行时主动中断（整体interrupted，
G2退出130，成功制品清单为空）。复核发现初版N1拟直接resume(2)，会遗漏调度身份迁移，
在N1实际执行前改用switch_to；未取得也未登记错误版本N1的运行结果。
终端外层中断后子Python仍持锁，进一步向该次Python发送SIGINT，由运行器终止整个子进程组并释放锁。
修正后七配置从头实跑；不把中断的前两配置拼接为完整通过。
普通BIOS既有N1标签属于原验收项，与本轮PRIMA_GATE_CASE=N1选择器不同。
方法对“无新Gate事件”的描述明确为实际验证的四类调用/交付/终结/拒绝事件，不声称穷举其它审计种类。

fe83e59远端通过仅补强G5及原六配置；N1本报告为本地证据，新提交仍需自己的远端结果。
不外推fast syscall、任意原语、SMP、KVM/实机或优化构建，不宣称统计稳定性/性能门槛通过。
前序嵌套bootloader输入、容量与计时失败继续由[整改链](2026-11-07-stage7-ci-remediation.md)及
[G4/G5报告链](2026-11-07-stage7-gate-user-benchmark.md)保留。
剩余三类基准、2/4核、门槛、发布许可/制品与真实双personality等后续机制仍按
[执行计划](../plans/2026-11-06-stage7-execution.md)推进；当前退出勾选仅查
[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)。
