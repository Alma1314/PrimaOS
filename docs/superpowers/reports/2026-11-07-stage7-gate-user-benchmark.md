---
id: report-stage7-gate-user-benchmark
kind: report
authority: scoped-implementation-and-validation
status: user-roundtrip-locally-verified
lifecycle: historical
updated: 2026-11-07
---

# 7-0 G5用户态Gate往返基线与普通回归

> 同日后继：[N1推进报告](2026-11-07-stage7-native-syscall-benchmark.md)记录fe83e59自身干净远端九组通过、
> 88份归档及G5原始分位数独立核验；本文当时的本地运行身份与限制保留。

## 结论与合并范围

按作者“合并到主分支并继续推进”的指令，前序[原始证据归档修复](2026-11-07-stage7-ci-green-gate-baseline.md)
以`faf551ab47a14a791e6245a3c2fecbfb31e3d701`快进合入master并推送origin/master。
该提交的干净远端九组全部通过；原始日志和探针子目录镜像身份的遗漏取得远端修复证据。

本轮随后实现独立G5 selector，在真实用户态为每次Gate调用记录前后端点，覆盖硬件陷入、生产Gate/wait/reply、
最终iret和用户态续执行；每轮128个样本，两轮独立统计。端点必须包住原五个内核采样点，
不能拿pre-iret数据代替完整用户调用区间。G4仍为1000样本的独立内核路径记录。
本地六配置双轮、host/lint、特性Clippy和完整普通BIOS回归均通过。

依据为[实现前G5方法契约](../specs/2026-11-07-stage7-gate-user-benchmark.md)、
[G4方法与归档契约](../specs/2026-11-07-stage7-gate-baseline.md)及[五项基准裁决](../specs/2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)。
这是Gate一类的有界单核基线；其余四类、2/4核、性能数值门槛和发布检查仍待完成，不关闭7-0或其它子轨。

## 归档修复的远端后继

[Actions运行42342676851](https://github.com/Alma1314/PrimaOS/actions/runs/42342676851)已完成，整体success，
九个job及九份manifest的全部子步骤均通过。所有manifest均为干净faf551a，源摘要一致：
`c1ba55ba61b5914515a7f17033834b780f9551107dfd25d25a082e574a9dbbc1`。

下载命令为`gh run download 42342676851 --repo Alma1314/PrimaOS --dir /tmp/prima-stage7-archive-green`；
详情用`gh run view 42342676851 --repo Alma1314/PrimaOS --json jobs,conclusion,headSha,status,url`取得，均退出0。
Python逐份核对九组集合、源身份、全部退出值、86份归档的字节长度/SHA-256，以及三种探针BIOS镜像身份登记，退出0。

| 组 | manifest秒数 | 原始归档数 | 最终制品身份数 / 范围 |
|---|---|---|---|
| host | 128.372 | 0 | 0；Rust1135及Python运行器11项等host检查 |
| lint | 146.414 | 0 | 0；fmt/Clippy、裸机、零unsafe及TCB |
| docs | 9.339 | 0 | 0；382文档，push事件无PR基线检查 |
| bios | 573.746 | 38 | 38；36组72轮，含默认启动双轮；归档含空的错误汇总日志 |
| linux | 200.018 | 22 | 12；11案例双轮及linux-probe镜像身份 |
| gate | 185.876 | 10 | 5；原五配置各双轮及gate-probe镜像身份 |
| gate-audit | 311.249 | 14 | 5；七site双轮及gate-audit-probe镜像身份 |
| domains | 162.210 | 2 | 5；仍限D14人格桩 |
| formal | 329.413 | 0 | 0；既有主Lean/有限槽表/源码细化及变异范围 |

这次远端运行尚不包含G5。镜像条目是实际文件的身份登记，Actions仍仅上传验收目录，不宣称上传了镜像字节。
前序1921757缺少原始样本及探针身份的记录保持原状，不回填旧manifest。

## G5实现与检查

kernel/src/gate_bench.rs复用既有两个Task、Code发射器和生产采样挂钩。
客户端用LFENCE/RDTSC取得完整u64，保存起点后准备参数并int80，返回后先保存rax，再取得终点，最后验证答复。
128对时间戳只占现有USER2_Y_VA页的2048字节，编译期检查页内上界；没有追加ABI或修改映射。
服务仍逐次wait/reply并让出。结束检查128条trap/delivered/replied审计、退休请求根历史及Gate/等待状态良构。

S7HISTORY/S7BENCH/S7TIME与G4的S6记录分开；每条G5含索引及七个时间点。
xtask共用校验/分位数逻辑，拒绝错误头/数量/索引、零频率/起点、时钟回退、用户区间未包住内核区间及panic。
新增变动时长的host反例还核对128样本的nearest-rank排名64/127，避免只用常量时长掩盖分位数错误。
CI Gate组在原五配置后增加G5，仍每配置双轮、单轮30秒。

## 本地命令与结果

基线为faf551a；运行时包含本轮未提交代码/测试/方法契约，明确dirty=true。
Gate、host、lint和普通BIOS运行的源摘要相同：
`a0b1aee9e52afe978e2c24a63d82ef72a61d0a835c60a0e282d0372477a33942`。
后续只补录文档/生成目录，不回填这些manifest；提交后的revision也不冒充原运行身份。
原始输出留ignored target/ci及/tmp/prima-stage7-g5-*.log。

| 命令 | 本次结果 / 运行ID | 证据范围 |
|---|---|---|
| CARGO_NET_OFFLINE=true cargo test -p xtask benchmark | 退出0；2项通过 | G4格式回归与G5端点/数量/分位数反例 |
| python3 scripts/ci.py gate | 退出0；243.582秒；4692448057f941bbaf959281c2b32bf9 | 六配置各双轮；12份逐步原始串口，G5两轮共256样本，5个最终探针制品身份 |
| python3 scripts/ci.py host | 退出0；10.638秒；7c9579f8d50149f2a1fb86e2fbded667 | Rust1136通过/0失败/1默认忽略；musl7、POSIX std3、运行器11项 |
| python3 scripts/ci.py lint | 退出0；8.220秒；cfb47956a5964ea4b8127bbf288b5fd7 | fmt/Clippy、裸机构建、41文件零unsafe、personality守卫；TCB2087/40942=5.10% |
| CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G5 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings | 退出0；最终复核1.16秒 | 含G5特性的完整静态检查；此前首次编译56.94秒亦通过 |
| python3 scripts/ci.py bios --prepare（允许网络/缓存写入和本机UDP） | 退出0；1460.741秒；d633123b0d0e40df907ae94f5750495e | 准备61.635秒；普通BIOS36组72轮（含默认启动双轮）及原Linux参考通过，38份日志归档 |
| Python独立重算与归档核对 | 退出0 | 12份原始字节长度/哈希匹配；G5两轮各128连续有序样本、端点覆盖和八组分位数均匹配xtask摘要 |
| python3 scripts/docs.py self-test / build / check / check --base faf551a | 全部退出0；384文档 | 元数据、目录、链接/锚点及基线变更守卫；补录结果后重新生成并检查 |

## 用户调用往返基线v0.1

本机AMD Ryzen 5 3500U、逻辑CPU8、Linux7.0.0-42/glibc2.39，QEMU8.2.2。
采用默认dev构建、pc-i440fx-8.2、TCG单线程、qemu64、1 vCPU、128M、BIOS，未实施宿主独占负载控制。
每轮校准换算分别为2045006849Hz和2087941401Hz；包含冷样本、用户端点和五点内核插桩开销，串口在测量结束后输出。
下表每格为独立计算的nearest-rank p50/p99，单位ns。

| 轮次 | user-entry | kernel-total | user-return | user-total |
|---|---|---|---|---|
| 1 | 114231/378820 | 3398928/5886080 | 11501/29707 | 3595526/6071957 |
| 2 | 98988/252439 | 2868370/5893763 | 9554/53788 | 2985805/6114219 |

| 轮次 | trap→queue | queue→deliver | deliver→reply | reply→resume |
|---|---|---|---|---|
| 1 | 251629/646325 | 1618237/3010455 | 956877/2749039 | 512583/1320068 |
| 2 | 199153/848029 | 1381824/3351504 | 810785/1846362 | 430431/917961 |

user-total包含起点保存、参数准备、完整陷入/Gate/wait/reply/iret、返回rax保存及终点前LFENCE；
不包含终点保存和答复比较，不能称为仅int80指令成本。各段分位数不相加为总量。
G4的1000样本与G5的128样本、以及不同宿主/提交/加速方式的数据均不混算为同一总体。

必要身份如下；原始样本相对上述Gate运行目录位于traces/05/S6GATE-{1,2}.log，文件名沿用通用日志通道，内容为S7记录。

| 项 | SHA-256 |
|---|---|
| G5轮1原始串口 | a89eb80ce694efaadc453b50aba80adaa9c3caf6c15e822b3c2f0274dc0df977 |
| G5轮2原始串口 | 0f39645713a51041b11c96e56ba968e41bb3dbecaccf3eab92943ae2b6ef1a91 |
| 最终G5 BIOS镜像 | 7a42dfd8f778755adb263eee2b2d85538ba0460697ae478c7abe969997ce706b |
| 最终G5 boot内核ELF | 2a88a752b53a279950554dd2814aa2ee656916b6c0f5029fc91a8fa44a33b436 |

独立重算核心如下；实际检查还验证了唯一历史/校准、128行索引、组内/跨组有序、无panic，
并将全部八组结果组成的完整摘要与05.log逐轮精确比较，全部通过。

```sh
python3 - <<'PY'
import hashlib, json, re
from pathlib import Path
base = Path('target/ci/gate/4692448057f941bbaf959281c2b32bf9')
m = json.loads((base / 'result.json').read_text())
assert m['status'] == 'passed'
step = next(s for s in m['steps'] if s['env'].get('PRIMA_GATE_CASE') == 'G5')
names = ['trap-queue', 'queue-deliver', 'deliver-reply', 'reply-resume',
         'kernel-total', 'user-entry', 'user-return', 'user-total']
for trace in step['traces']:
    raw = (base / trace['path']).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == trace['sha256']
    text = raw.decode()
    hz = int(re.search(r'S7BENCH\|samples=128;hz=(\d+);', text)[1])
    rows = [[int(v) for v in x[7:].split(';')]
            for x in text.splitlines() if x.startswith('S7TIME|')]
    assert len(rows) == 128
    vectors = [[r[j+1]-r[j] for r in rows] for j in range(2, 6)]
    vectors += [[r[6]-r[2] for r in rows], [r[2]-r[1] for r in rows],
                [r[7]-r[6] for r in rows], [r[7]-r[1] for r in rows]]
    print(trace['path'], {name: [sorted(v)[i]*10**9//hz for i in (63, 126)]
                          for name, v in zip(names, vectors)})
PY
```

## 失败、限制与后继

本轮实际G5、Gate回归、host/lint和普通BIOS命令均通过；非法格式/区间等失败仅在host反例中按预期拒绝。
前序G4容量、计时和输入准备失败仍由[原统一报告](2026-11-02-stage6-process-and-differential.md#m1e计量结果)
与[工程整改链](2026-11-07-stage7-ci-remediation.md)保留，不因本轮通过删除。

新G5代码在本报告记录时只有上述本地证据；faf551a远端九组通过不能代替G5提交自己的远端结果。
测量只覆盖固定单客户端/服务的int80 user-wrapper与声明的QEMU/dev配置，不外推fast syscall、多核、KVM或实机。
没有冻结数值门槛、建立统计稳定性保证、扩大兼容profile或新增形式化证明。
原生syscall、IPC往返、页故障/COW、端到端LibOS四类基准、2/4核及发布工件继续待完成；
当前退出勾选仅查[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)。
