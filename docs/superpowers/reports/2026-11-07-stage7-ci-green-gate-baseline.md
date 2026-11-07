---
id: report-stage7-ci-green-gate-baseline
kind: report
authority: scoped-implementation-and-validation
status: remote-nine-green-local-evidence-verified
lifecycle: historical
updated: 2026-11-07
---

# 7-0九组CI通过与可复核Gate内核路径基线

> 后继：[G5用户态Gate往返基线](2026-11-07-stage7-gate-user-benchmark.md)记录faf551a合入master后的
> 干净远端九组、86份原始归档和探针镜像身份，以及独立G5的用户端点、本地六配置双轮和普通72轮。
> 本文的“新增归档器仅本地/用户态测量待完成”保留为当时状态；G4原始数据与pre-iret范围不扩大。

## 结论与范围

承接[首次远端失败及bootloader准备整改](2026-11-07-stage7-ci-remediation.md)，提交
`19217573e641948308085a6d344f603cc9d84f4e`已取得干净远端九组全部通过的真实结果。
逐份核对九个manifest，所有步骤退出0、源身份一致。前序七组编译失败在此范围取得远端后继，
不是把本地PASS替代远端执行，也不抹除原失败。

复核同时发现原始串口未进入附件，以及探针子目录镜像身份漏记。本轮修复宿主运行器的两处证据缺口，
本地Gate五配置双轮通过，归档10份逐步串口与最终探针制品身份；G4的2000个原始样本经Python独立重算。
本轮新增归档器尚无其自身远端CI结果。G4止于最终iret之前，只提供Gate内核路径基线；
完整用户态Gate往返、其余四类基准、性能门槛与发布清单仍待完成，不关闭7-0或其它子轨。

## 干净远端九组结果

[Actions运行42336056301](https://github.com/Alma1314/PrimaOS/actions/runs/42336056301)
于北京时间2026-11-07 11:54至12:19执行，整体success。下列秒数来自各组manifest，含准备与最终身份检查，
不是每个GitHub job的全部墙钟时间。

| 组 | job ID | 结果 / 秒 | 实际范围 |
|---|---|---|---|
| host | 128293130023 | passed / 164.401 | Rust1135通过、0失败、1默认忽略；musl7、POSIX std3、运行器9项 |
| lint | 128293129979 | passed / 113.577 | fmt/Clippy、裸机编译、41文件零unsafe、personality守卫；TCB2087/40871=5.11% |
| docs | 128293130029 | passed / 12.600 | self-test/build/check，380文档；push事件不含PR基线检查 |
| bios | 128293130035 | passed / 957.815 | 36组72轮（含普通启动双轮），原输出/IPC/容量等判据 |
| linux | 128293130086 | passed / 313.933 | pinned guest同ELF参考、11案例各双轮，stdout/stderr/exit/audit全通过 |
| gate | 128293130204 | passed / 289.765 | G1/G2/G2-death/G3/G4各双轮，G4各1000样本 |
| gate-audit | 128293130198 | passed / 310.679 | 七个site各双轮，指定审计停止exit35与unchanged断言 |
| domains | 128293130077 | passed / 118.692 | D14人格桩双轮，roots2/shared_log3/denied3/survivor1；不是7-A真实应用退出 |
| formal | 128293130133 | passed / 317.121 | 主Lean、有限槽表、生产源码细化/新鲜度/公理审查及generation重置变异；既有证明范围 |

九份manifest均为dirty=false，revision为上述提交，files_sha256为
`a0b063238bb11682fa8852403227d90c65f6f3161d4876b1c95ef30ff0cccf91`。
Ubuntu 24.04 runner镜像20261101.360.1，逻辑CPU4；实际rustc为1.99.0-nightly
`1a833e165`、Clang18.1.3、GCC13.3.0、QEMU8.2.2。构建输入仍按既有锁核对，
包括musl `0784374d561435f7c787a555aeab8ede699ed298`、sbase
`c546c3a5724c81cee9a11d816a38ccdf17472129`及D12 ELF/参考内核身份。

读取命令为`gh run view 42336056301 --repo Alma1314/PrimaOS --json jobs,conclusion,headSha,url`与
`gh run download 42336056301 --repo Alma1314/PrimaOS --dir /tmp/prima-stage7-ci-green`，均退出0。
Python核对九组集合、源身份和全部步骤退出值，退出0。原始附件只留/tmp与Actions短期附件。

## 新发现与工程修复

1. qemu.rs把原始Gate串口写入target/prima-os/S6GATE-{1,2}.log，后续配置覆盖同名文件。
   旧工作流只上传target/ci，命令摘要保留但成功测量的原始时间戳没有进入附件。
2. image.rs的package_images将探针制品放入gate-probe、gate-audit-probe、linux-probe子目录；
   原outputs只扫描根目录。上述远端Gate和审计manifest的artifacts为空，Linux也没有探针镜像身份。
   九组功能通过不等于这两项证据已经完整，不能回填旧manifest或恢复已覆盖的旧原始样本。

实现前补充[测量与归档契约](../specs/2026-11-07-stage7-gate-baseline.md)，再修改scripts/ci.py：
每个命令前快照直接*.log的大小/mtime，结束后将新增/改写日志按步骤复制到本次运行目录，
记录来源、归档路径、大小和归档SHA-256；未变旧日志及符号链接排除。
失败/超时/中断也保留已落盘的日志，诊断不进入成功artifacts；复制失败使组非通过，并保留子命令实际退出值。
镜像/ELF身份扫描扩至根目录和一层子目录。没有新增依赖或OS测量挂钩，QEMU双轮与30秒门限保持。

## 本地命令与结果

本地基线为同一1921757，运行时包含本片未提交的运行器/测试/契约，明确dirty=true。
Gate、host和lint的运行时源摘要相同：
`be27dd6dda126b35faaaf61304d58df947f5959181ee4d1dfcf48a190044bdfd`；后续文档补录不回填历史manifest。
原始输出仅保留ignored target/ci和/tmp/prima-stage7-*.log。

| 命令 | 结果 / 运行ID | 范围 |
|---|---|---|
| python3 scripts/test_ci.py | 退出0；11项通过 | 新增逐步覆盖/失败日志保留、归档I/O错误不能PASS；扩展子目录制品与旧文件排除回归 |
| python3 scripts/ci.py gate --prepare（允许网络及工具缓存写入） | 退出0；334.385秒；38de050e490a419db257a82c097c1415 | 五配置各双轮，10份原始串口按步骤归档，5个最终探针制品身份 |
| Python独立核对归档与统计（下节） | 退出0 | 10份归档字节长度/哈希匹配；G4唯一校准/历史记录、1000连续单调样本和两轮五组分位数匹配xtask |
| python3 scripts/ci.py host | 退出0；9.293秒；dcef31dd16fa4e96a279d1a5f8d2c2e5 | Rust1135、musl7、POSIX std3、运行器11项通过；1项Rust默认忽略 |
| python3 scripts/ci.py lint | 退出0；4.479秒；43673f7d421f4b6ca7e40ea5fa5526a5 | fmt/Clippy、裸机构建、41文件零unsafe、personality守卫、TCB5.11% |
| python3 scripts/docs.py self-test / build / check / check --base 1921757 | 全部退出0；382文档 | 元数据、目录、链接/锚点及基线变更守卫；补录结果后重新生成并检查 |

## Gate内核路径基线v0.1

方法按[本片契约](../specs/2026-11-07-stage7-gate-baseline.md#gate内核路径基线v01)：
单客户端/服务、每轮1000次生产int80/Gate/wait/reply，保留冷样本，不扣除采样器开销。
测量从Gate适配器入口到最终iret前，不含硬件陷入入口、最终iret或用户态续执行。
每轮独立计算nearest-rank p50/p99，总量由首末点独立计算，不是各段p99之和。

本机为AMD Ryzen 5 3500U、逻辑CPU8、Linux7.0.0-42/glibc2.39；准备期观测loadavg约4.10/2.96/2.28，
没有宿主独占或固定负载保证。QEMU8.2.2，pc-i440fx-8.2、TCG单线程、qemu64、1 vCPU、128M、BIOS。
以下单位ns，每格p50/p99；两轮校准分别为2073235123Hz和2069166985Hz。

| 轮次 | trap→queue | queue→deliver | deliver→reply | reply→resume | total |
|---|---|---|---|---|---|
| 1 | 194306/456356 | 1391970/2668176 | 730621/1374305 | 416609/846630 | 2756745/5033016 |
| 2 | 189451/459892 | 1360515/2999104 | 724984/1542193 | 403697/1002072 | 2730804/5466166 |

相对本次运行目录，原始样本位于traces/05/S6GATE-{1,2}.log，均已通过生产验证器。
必要身份如下；不是跨路径逐字节可复现或UEFI执行主张。

| 项 | SHA-256 |
|---|---|
| G4轮1原始串口 | 1cac5fe79ac36f91501798cd0068a5ab8ccae9412976beeca7264834251c60e3 |
| G4轮2原始串口 | 1829282e84a0f3a796e595e57e43a7b5f33e2a8c53a9f1df6e39a98f907d400d |
| 最终Gate BIOS镜像 | cda2d58dc5d304d940d7e4dc1775e5f7bfb8abc58dc4171da357b38c20e0820f |
| 最终Gate boot内核ELF | 31222b7a08a5a0b0bf7cef0694bd0d971da78aa3918456a7f56c7d9d4de5116f |

独立重算的核心命令如下；实际检查还逐份核对了10份归档及G4的唯一历史/校准、连续索引、
组内/跨组单调与无panic，并将生成的完整摘要逐轮和05.log中的唯一对应行比较，全部通过。

```sh
python3 - <<'PY'
import hashlib, json, re
from pathlib import Path
base = Path('target/ci/gate/38de050e490a419db257a82c097c1415')
manifest = json.loads((base / 'result.json').read_text())
assert manifest['status'] == 'passed'
step = next(s for s in manifest['steps'] if s['env'].get('PRIMA_GATE_CASE') == 'G4')
names = ['trap-queue', 'queue-deliver', 'deliver-reply', 'reply-resume', 'total']
for trace in step['traces']:
    raw = (base / trace['path']).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == trace['sha256']
    text = raw.decode()
    hz = int(re.search(r'S6BENCH\|samples=1000;hz=(\d+);', text)[1])
    rows = [[int(v) for v in line[7:].split(';')]
            for line in text.splitlines() if line.startswith('S6TIME|')]
    assert len(rows) == 1000
    vectors = [[r[j+2] - r[j+1] for r in rows] for j in range(4)]
    vectors.append([r[5] - r[1] for r in rows])
    quantiles = {name: [sorted(v)[rank] * 10**9 // hz for rank in (499, 989)]
                 for name, v in zip(names, vectors)}
    print(trace['path'], hz, quantiles)
PY
```

## 失败、限制与后继

- 首个本地Gate运行0f6a4ad10d87466f9da24130a2a0a37f在准备时主动中止，以先修复刚发现的探针目录漏记；
  退出1、status=interrupted、44.293秒、无通过制品，未进入QEMU。保留该记录，不计作双轮。
- 上述远端九组属于1921757；新增归档/子目录扫描只在本地host/lint/Gate实跑。其它组共享路径扫描有host反例，
  但本片没有重跑其本地QEMU，也没有新提交的远端九组结果。
- 对已被覆盖的旧日志和中间镜像不作恢复主张。大小/mtime快照只面向合作写入者；命令内部未留在磁盘的中间版本
  或未落盘的超时串口不在归档保证内。Actions仍是7天证据保留，不是完整发布包。
- 本地TCG数据和Azure/前序宿主数据不混算、不作回归判定；没有KVM/实机、2/4核曲线、完整用户态往返或数值门槛。
  [阶段6原测量及失败链](2026-11-02-stage6-process-and-differential.md#m1e计量结果)保留其原范围。
- 下一包需补齐完整用户态测量边界及其余四类基准的具体方法/探针，取得基线后确认门槛；
  发布许可/制品清单和其余必交付轨仍待完成。当前状态见[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)。
