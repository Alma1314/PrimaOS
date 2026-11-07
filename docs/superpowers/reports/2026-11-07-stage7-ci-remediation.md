---
id: report-stage7-ci-remediation
kind: report
authority: scoped-implementation-and-validation
status: local-remediation-remote-rerun-pending
lifecycle: historical
updated: 2026-11-07
---

# 7-0首次远端CI：嵌套bootloader输入准备整改

## 结论与范围

承接[CI接线与本地证据](2026-11-06-stage7-ci-evidence.md)，本轮读取首次真实九组远端结果，
定位七组共有的bootloader离线依赖缺失，并补上准备阶段的锁定构建及回归检查。
修复后的本地host准备与验收通过；修复版本尚未推送或在远端执行，不能声称九组全绿。
本片不改变OS启动代码、ABI、QEMU判据、双轮或30秒门限，不关闭7-0或其它子轨。

作者在本次继续推进阶段7时，已明确选择原生syscall、IPC往返、Gate往返、页故障/COW、
端到端LibOS操作五类基准。该名称清单已登记到[范围裁决§7.2](../specs/2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)；
本片没有新增这五项的测量实现、采样方法、基线或数值门槛。

## 基线与远端证据

基线为`746b02ae464cf87712ac0c922c07302d82539d4b`。首次
[GitHub Actions运行42247540137](https://github.com/Alma1314/PrimaOS/actions/runs/42247540137)
在北京时间2026-11-06 23:55至11-07 00:06执行，九组均已结束，整体failure。
下载的九份result.json均记录干净checkout、同一revision和源摘要
`d4dbc17307c2fed8a6efcdf49ba0070a7b6296a399b2fa174324afe222bb300f`。
这是原提交的远端证据，不是本轮未提交修复的证据。

| 组 | job ID | 结果 / 秒 | 实际范围 |
|---|---|---|---|
| host | 127993595438 | failed / 36.783 | 准备通过，workspace编译退出101；未运行测试 |
| lint | 127993597765 | failed / 33.582 | xtask编译退出101；未进入lint/TCB |
| bios | 127993595389 | failed / 38.539 | xtask编译退出101；未运行QEMU |
| linux | 127993595412 | failed / 35.471 | 锁定输入准备通过，xtask编译退出101；未运行差分 |
| gate | 127993595388 | failed / 28.771 | 第一配置编译退出101；未运行QEMU |
| gate-audit | 127993595397 | failed / 37.191 | 第一site编译退出101；未运行QEMU |
| domains | 127993595660 | failed / 40.663 | xtask编译退出101；未运行QEMU |
| docs | 127993595240 | passed / 15.147 | self-test/build/check；push事件没有PR基线检查 |
| formal | 127993595012 | passed / 404.572 | 主Lean、有限槽表、生产源码细化及变异；既有有界证明范围 |

读取命令为`gh run list --repo Alma1314/PrimaOS --limit 5 --json databaseId,headSha,status,conclusion,url,name`、
`gh run view 42247540137 --repo Alma1314/PrimaOS --json jobs,conclusion,headSha,url`和
`gh run download 42247540137 --repo Alma1314/PrimaOS --dir /tmp/prima-stage7-remote-evidence`，均退出0。
首次受限网络读取退出1；允许联网后取得上述结果。原始远端附件只在/tmp，不入库。

## 根因、契约与修复

七组01.log均出现同一类错误，例如：

```text
error: could not find `bootloader-x86_64-uefi` in registry `crates-io` with version `=0.11.17`
failed to run custom build command for `bootloader v0.11.17`
```

本地核对锁定bootloader的build.rs：它在构建时并行执行五条`cargo install --version 0.11.17 --locked`，
构建UEFI、BIOS boot sector及stage 2/3/4；这些包不在workspace Cargo.lock的依赖图内。
原准备步骤只执行`cargo fetch --locked`，因此干净runner离线编译时找不到这些包。
已有开发机缓存掩盖了这一缺口；七次失败尚不能说明OS运行判据有问题。

先补充[CI契约](../specs/2026-11-06-stage7-ci-evidence.md)，再在scripts/ci.py的共同Rust准备路径中增加
`cargo build --locked --package bootloader`，允许上游在准备阶段取得并构建嵌套输入。
docs不运行Rust准备；formal不依赖bootloader，仍走自己的工具链准备。
正式验收环境继续设置`CARGO_NET_OFFLINE=true`，没有打开正式步骤下载或使用dummy镜像。
回归检查验证host/lint的准备顺序、联网仅限准备、正式环境离线，以及docs不触发Rust准备。

上游bootloader并行启动五个子Cargo；各自2 worker不构成宿主全局2 worker上限。
本片如实补充这个资源边界，未修改上游构建器；组墙钟预算与进程组终止仍保留。

## 本地命令与结果

原始日志留在`/tmp/prima-stage7-*-20261107.log`及ignored target。

| 命令 | 本次结果 | 范围 |
|---|---|---|
| `python3 scripts/test_ci.py` | 退出0；9项通过 | 新准备回归及原8项故障/身份检查 |
| `python3 scripts/ci.py host --prepare`（允许网络及工具缓存写入） | 退出0；passed；265.100秒 | 准备256.860秒；离线Rust1135通过/0失败/1默认忽略；musl7、POSIX std3、运行器9项通过 |
| `CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=target/stage7-bootloader-offline cargo build --locked --package bootloader`（允许工具缓存写入） | 退出0；1分56秒 | 独立构建目录离线重建，五个真实启动产物均非空；没有使用此前target/debug下的bootloader构建产物 |
| `python3 scripts/ci.py lint` | 退出0；passed；171.798秒 | fmt/Clippy、裸机编译、41文件零unsafe、personality守卫及TCB通过；2087/40871=5.11% |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base 746b02a` | 全部退出0；380文档 | 元数据、目录、链接、基线变更守卫；补录结果后重新生成并检查 |

host运行ID为`5fe4f1b2fb1149f38f6be872794197be`，记录dirty=true；验证时源摘要为
`a05fab9afba81aab9e48404c06461e2588e67705a2510bca8e3653f00b318931`。
后续只补充文档和生成目录，不回填该次manifest。工具链为nightly-2026-07-30，宿主x86_64 Linux。
lint运行ID为`7569e88fe57e4ae4909b67168370e97f`，dirty=true，源摘要
`226626b89aa69f4d95b108222871d12db5a9f40af12283303db977e1031eb064`。
独立离线构建生成UEFI 172032字节、BIOS boot sector 512字节及stage 2/3/4的492031/118786/151960字节。
随后将CARGO_TARGET_DIR设为该目录的绝对路径复核同一命令，退出0；该次命中已完成构建，不另算干净重建。

## 失败与限制、后继

- 本轮最初本地host运行`b6568af37adb4cd6b2eeb7e9eaaf67d3`在所有步骤结束后，因运行中修改源文件
  被源状态守卫拒绝，status=failed；不能把其单步通过当成组PASS。后继固定源版本重跑才获得上表通过。
- 全新`target/stage7-bootloader-offline`构建目录的离线检查，首次在沙箱退出101：上游安装器尝试写
  Cargo registry内的target目录，被只读权限拒绝。原因是本次检查传入的相对CARGO_TARGET_DIR
  被上游子Cargo从registry工作目录解析；允许缓存写入后的同命令重跑成功，随后改绝对路径复核。
  首次失败不归因为缺包或OS失败，也不能把后续缓存命中冒称另一轮干净重建。
- 本轮不新增QEMU或OS机制证据；远端formal通过不扩大现有模型/源码细化证明范围。
- 尚未重跑修复提交的远端九组，不将本机已有Cargo缓存当作完整干净runner安装证明。
  下一步以修复提交收集远端九组实际结果，保留后续失败；五类基准还需冻结具体方法并测基线，
  实测后再确认性能门槛，发布许可/制品清单仍待完成。

当前阶段与退出勾选见[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)，
工作包依赖见[阶段7计划](../plans/2026-11-06-stage7-execution.md)。本报告补强首次远端结果与输入准备主题，
不覆盖或抹除前序本地报告中的失败及未覆盖项。
