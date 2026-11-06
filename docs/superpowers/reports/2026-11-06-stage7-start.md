---
id: report-stage7-start
kind: report
authority: scoped-implementation-and-validation
status: first-slice-verified-with-limits
lifecycle: historical
updated: 2026-11-06
---

# 阶段7启动与首个7-0工程增量

## 结论与范围

作者于2026-11-06指示“启动阶段7”，阶段7进入进行中，当前首片为7-0工程底座。
本轮实现共享镜像工件互斥，开始解决[前序完整验收与compat相撞](2026-11-06-stage0-6-review-remediation.md)的问题。
九轨退出仍全部未勾选；CI、五项基准及数值门槛、真实双personality、SMP、配额、撤销、动态链接、
设备/持久化、透明恢复与AArch64均没有被本片宣布完成。

## 基线与契约

- 基线：`d171139014ab9b8aa0b67fef3a5d79ae93a00c3a`，阶段0–6整改已合入master；本轮分支`codex/stage7-foundation`。
- 范围：[九轨裁决](../specs/2026-10-31-stage7-scope-decisions.md)与[白皮书](../../../Prima%20OS.md#阶段-7--扩展与硬化)。
- 实现前契约：[共享工件互斥](../specs/2026-11-06-stage7-artifact-exclusion.md)；
  [执行计划与作业分组](../plans/2026-11-06-stage7-execution.md)记录依赖及仍待冻结的具体工件。
- 工具链为仓库pinned nightly-2026-07-30；使用其标准库`File::try_lock`，无新增依赖。
- 阶段1–6原退出范围保持；阶段6技术复核不因本次启动自动获得组织级签署。

## 实现与观察

`xtask/src/artifact_lock.rs`在工作区`target/prima-os-artifacts.lock`申请非阻塞独占锁。
`main.rs`共同入口在建立业务输出目录和调用各工作流前持有句柄，直到整个命令返回。
普通镜像、交互run、所有test系列、Linux/Gate专用镜像、compat和linux-trace共享这一互斥点；
lint/tcb可继续执行其各自工作。

竞争立即产生包含命令和锁路径的`workspace artifacts busy`诊断并退出1；不进入构建/参考/QEMU流程。
I/O错误同样失败关闭。锁释放后保留锚点文件，文件存在不能被误认为仍被占用；没有强制绕过或删锁重试。
内部验收双轮、30秒门限、stdout/IPC/审计/容量断言均复用原代码。

## 命令与结果

原始日志位于`/tmp/prima-stage7-*.log`及ignored target；不提交运行日志或镜像。

| 命令/检查 | 结果 | 范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p xtask artifact_lock -- --nocapture` | 退出0；3项通过 | 真实跨进程竞争、drop释放、保留锚点文件、路由/独立目录、I/O失败 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | 退出0；1135通过、0失败、1默认忽略 | host；比基线新增3项工具测试 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | 退出0；fmt/Clippy、裸机构建、41文件零unsafe、personality守卫通过 | 源码/编译；lint不申请图像锁 |
| 下方Python跨进程CLI反例（默认xtask由本轮lint预先构建） | 退出0；三个命令均退出1、stdout为空、stderr含busy；3个既有镜像哈希及整个业务目录的路径/大小/mtime不变 | 共同入口在业务分派前拒绝，未启动QEMU |
| `CARGO_NET_OFFLINE=true cargo xtask test-process W5` | 退出0；双轮通过 | 原CPL3共享能力、trace、容量、IPC判据与30秒门限 |
| `CARGO_NET_OFFLINE=true cargo xtask compat` | 退出0；`PASS compat cow-write-fault` | 正常构建/运行与COW有界差分路径；不是同ELF Linux专项 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base d171139` | 全部退出0；377文档，元数据/目录/链接与基线变更检查通过 | 新契约/计划/报告、当前阶段与检索索引 |

CLI反例需要已有镜像，使用已构建的默认`target/debug/xtask`；关键命令完整如下。
它持有与Rust标准库相同的宿主`flock`锁，不使用跳过开关：

```sh
python3 - <<'PY_CHECK'
import fcntl, hashlib, subprocess
from pathlib import Path
root = Path.cwd()
artifacts = root / 'target/prima-os'
images = sorted(artifacts.glob('prima-os-*.img'))
assert images
snapshot = lambda: {str(p.relative_to(artifacts)): (p.stat().st_size, p.stat().st_mtime_ns)
                    for p in artifacts.rglob('*') if p.is_file()}
hashes = lambda: {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in images}
before, digests = snapshot(), hashes()
with (root / 'target/prima-os-artifacts.lock').open('a+b') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    for arguments in [['build'], ['compat'], ['test-process', 'W5']]:
        result = subprocess.run([str(root / 'target/debug/xtask'), *arguments],
                                cwd=root, capture_output=True, text=True, timeout=10)
        assert result.returncode == 1
        assert 'workspace artifacts busy' in result.stderr and not result.stdout
assert before == snapshot() and digests == hashes()
PY_CHECK
```

前序镜像缺失/写锁、W5计数冲突与M4审计耗尽的发现及修复过程继续保留在
[阶段0–6整改报告](2026-11-06-stage0-6-review-remediation.md)。本轮所有计划内检查均按上述范围通过；
未伪称运行新的完整72轮、远端CI、Linux专项或阶段7证明。

## 限制与后继

下列限制按本首片运行时点保留。CI接线/输入准备/验收身份的后续实现与实跑见
[CI后继报告](2026-11-06-stage7-ci-evidence.md)，该后继也不代表远端CI或7-0已完成。

- 本片是宿主工具互斥，不是OS的资源配额、审计隔离或7-G子进程监督。
  直接执行QEMU、外部改写/删除工件和中止后残留子进程的清理仍需后继处理。
- 锁仅覆盖合作xtask的图像生命周期，不能限制Cargo先行编译、不同工作区的CPU/RAM/端口总量，
  也不保证在宿主负载下全部QEMU运行都满足既有时限。
- CI没有在本轮接线或远端执行；G6五项基准清单/方法和实测后的门槛仍待冻结。
  [执行计划](../plans/2026-11-06-stage7-execution.md)的下一包是可复现CI分组/资源预算与制品身份。
- 本轮没有新增OS ABI、内核机制、阶段7应用白名单、性能基线或形式证明。
  前序72/72与Linux/Gate双轮是基线历史证据，不能当成本片新运行。
- 本片验证不关闭7-0或其它子轨；阶段7状态与冻结门的当前摘要见[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)。
