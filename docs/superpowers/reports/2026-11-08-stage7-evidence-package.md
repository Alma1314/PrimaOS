---
id: report-stage7-evidence-package
kind: report
authority: scoped-tooling-and-validation
status: source-evidence-package-verified
lifecycle: historical
updated: 2026-11-08
---

# 7-0 源码/验收证据包与73056ba主分支CI后继

## 结论与范围

新增独立的宿主打包/复核入口，将已通过的九组CI清单、命令日志、原始串口、指定Git源码及许可事实
组成可复核tar.gz。真实73056ba包已构建、复核并重复生成相同字节；154个外层成员、655个tracked源码文件、
九组/97份串口及66条组内最终制品身份齐全。包内没有可运行镜像，image_release_eligible=false。
这是S7-0.3发布材料的首片；第三方二进制组件/NOTICE、外部对应源码/构建材料和镜像字节绑定继续待完成。
本片不改变OS/ABI/QEMU门限，不发布GitHub Release，不关闭7-0、G6或其它子轨。

另核验合并后的73056ba自身干净远端九组与全部97份归档，补充[L1报告](2026-11-08-stage7-libos-benchmark.md)
对实现提交e6dce41的证据。L1本机两轮中断及此前失败继续保留；新远端通过不裁决其超时因果。
本文为同一会话的工程复核，不是签名证明、独立第三方审查、许可法律裁决或组织签署。

## 基线与输入

起点73056bafed68afb888c613ee4ce1375182fb1f4b，工作树干净；依据
[发布首片契约](../specs/2026-11-08-stage7-evidence-package.md)、
[CI身份与逐步归档](../specs/2026-11-06-stage7-ci-evidence.md)、
[执行计划S7-0.3](../plans/2026-11-06-stage7-execution.md)、
[仓库许可登记](../../architecture/licensing.md)及L1发现/失败链。
Python3.12.3、Git2.43.0，固定Rust nightly-2026-07-30；本片没有新QEMU执行，QEMU/Lean结论仅引用下节指定提交的真实CI。
原始命令输出与包留在ignored target/release、target/ci及/tmp，不提交日志或独立哈希附件。

## 73056ba 自身干净远端核验

本次gh run view 42446028729和gh run download 42446028729 --dir /tmp/prima-stage7-master730-remote均退出0。
[Actions 42446028729](https://github.com/Alma1314/PrimaOS/actions/runs/42446028729)整体及九job均success；
九份manifest均source.revision=73056bafed68afb888c613ee4ce1375182fb1f4b、dirty=false，摘要
386820a94de8196db92309fdf9ecac94f9a5b652186ab50af044000a0d23372b，全部步骤passed/exit0。
独立核对97份串口长度/SHA-256、普通BIOS36组72个唯一(case,run)、Gate九配置双轮，
并独立解码L1参考/Prima全部192条回执和分位数，均与步骤日志相符。

| 组 | manifest运行ID | 秒数 | 串口归档 / 最终制品身份 |
|---|---|---|---|
| bios | 7048a446da2f47ef8dda6cc6ff58f8bb | 591.006 | 38 / 38 |
| docs | 076e7d42195a498aa33f0569805ef617 | 10.13 | 0 / 0 |
| domains | 37ced5c41c7e4e6fbcb2715a3755f041 | 117.814 | 2 / 5 |
| formal | 10e4497144fd46d9939ff1eec1b471ed | 289.086 | 0 / 0 |
| gate | 2c0100fd268e43c294af69210de3d353 | 369.182 | 18 / 5 |
| gate-audit | d8c54846daf144d0bbe914f7dd8ff235 | 262.116 | 14 / 5 |
| host | 91a3e202dcc844a985b7955f5c5da174 | 207.68 | 0 / 0 |
| lint | 88ea3c1e9aa642a8b900435016a879f5 | 151.46 | 0 / 0 |
| linux | 9d70c092519c44628cabc41c9bf63dd7 | 250.275 | 25 / 13 |

默认双轮已包含在72轮中，不能另加为74；38份BIOS归档不代表72轮各有独立原始文件。
Actions未上传镜像字节；66条是跨组的最终身份记录，允许不同组相同路径有不同身份，不是66份独立镜像。
D14仍仅人格桩，formal仅既有模型/细化范围，不增加任何新的证明主张。

## 实现与许可事实

- scripts/package_evidence.py只使用Python标准库与Git，提供创建和--verify两条入口。快照直接来自指定commit的git archive；工具在脏工作树中仍可打包历史提交，但CI输入自身必须dirty=false，内容摘要须与Git快照实算值精确相等。
- 源码摘要复用CI的路径字节序、文件可执行位、内容和链接目标规则；单测直接与ci.source_identity比较。未提交/未跟踪内容、target与Git凭证不进入历史快照。
- 仅复制九组清单所引用的命令日志与trace，逐份检查长度/哈希；缺组、重组、混版、失败/中断、空命令、缺日志、篡改、逃逸路径和链接组件拒绝。包复核重新核对成员集合及每份CI记录，并用本地Git对象核对内层快照，不能仅相信自报commit。
- 固定顺序与gzip mtime=0；临时文件写完/fsync后用独占链接发布，不覆盖旧包。上限为单成员64MiB、总载荷256MiB、10000成员，失败不静默截断。
- 原样附根GPL许可全文、MPL全文及musl COPYRIGHT，均与既有登记哈希相符。记录九个package声明（八workspace成员及独立Rust探针）和三个MPL文件的SPDX/哈希。
- prima-ostd Cargo字段为MPL-2.0，与根项目政策及逐文件声明并列保留；独立prima-rust-probe的license字段原值缺失，清单保留null。本工具不补填或改归属，不把字段登记当作完整二进制许可结论。
- 包内source.tar是73056ba快照；本片新工具源码另置tools/package_evidence.py并记录哈希，不伪称其属于旧快照。--verify需要包含指定commit的本地Git仓库，可用--repo指定。
- CI host组追加七项打包反例测试，原四条命令及九组编排预算保持；没有新依赖、镜像构建或生产探针。

## 本次命令与结果

host/lint记录source.revision=73056ba、dirty=true，同一源摘要
020a5200fe1befb519c7e39644c853194a9b225f2ef5d898423c19544ddcb41a；当时包括新工具、测试、CI入口及实现前契约。
之后仅补录本报告、阶段/索引、许可说明及生成目录；不把本地dirty结果改称新提交自身干净远端。

| 命令 | 实跑结果 | 运行ID / 秒数 |
|---|---|---|
| python3 scripts/test_package_evidence.py | 退出0，7项测试通过；涵盖确定性/历史快照隔离、CI摘要等价、13类清单变异、缺失/篡改/逃逸/链接、许可改变、归档篡改及旧包不覆盖 | 本地聚焦 |
| python3 scripts/ci.py host | 退出0，Rust1142通过/0失败/1默认忽略；musl7、POSIX std3、CI运行器11、新打包测试7均通过 | 88760065d3244c8b9395c40e22788189 / 10.127 |
| python3 scripts/ci.py lint | 退出0，fmt/Clippy/裸机构建/44文件零unsafe与人格护栏通过 | bd2d33de846848a7b927a5d69fde238b / 3.979 |
| python3 scripts/package_evidence.py --revision 73056ba --evidence-root /tmp/prima-stage7-master730-remote --output target/release/prima-source-evidence-73056ba-v1.tar.gz | 退出0，groups=9 traces=97 images=0 | 真实主分支输入 |
| python3 scripts/package_evidence.py --verify target/release/prima-source-evidence-73056ba-v1.tar.gz | 退出0，同一源码身份/九组/97份串口 | 本地Git对象与成员复核 |
| 同创建命令，output改为prima-source-evidence-73056ba-v1-repeat.tar.gz | 退出0，完整gzip字节与首包一致 | 没有覆盖原包 |

独立标准库脚本不调用打包器：逐一读取154个成员并重算长度/哈希，核对清单集合、九组/97条trace引用、
源码文件数及无Git/target成员、无镜像发布主张，并直接比较两包完整字节；全部通过。
另检查真实内层快照不含ELF/镜像文件，包内工具源码与本次实际实现的长度/哈希一致。
三个许可全文35149/16725/6204字节；九个package声明及三个MPL文件逐项保留原始事实。

正式包2946257字节，SHA-256=badd1c8dcdf60aa08df7de5afbf3edfda094894231efc1cf0c73689efd59fe07。
154个外层成员共18578111字节；内层source.tar为10045440字节，SHA-256=
c718980a2320a7937458beed9b35588e5120d4e7cf533471ef9fccfeea903950。
打包器源码14492字节，SHA-256=0f238481bd585699747ae3467af94398ba561753b5d275f08cbf2c9f1b5d635d。
其中42份命令日志、9份组清单、97份串口，以及source.tar、三许可、工具源码与包清单；不含镜像字节。
早期开发试包不作为此正式包身份或验收基线。

TCB仍为2087 trusted / 39657 mechanism / 41744 total，5.00%；unsafe79/0，
include_bytes=0、prima_user_dep=false、service_name_lines=0、protocol_lines=56。
python3 scripts/docs.py self-test、build、check、check --base 73056ba及git diff --check均退出0；395份文档/3753节。
补录结果后重新生成目录并复核，不手改生成目录。

## 反例、限制与后继

测试明确拒绝失败/中断、dirty、混合commit/摘要、缺组/重复/未知组、缺日志/篡改trace、路径回退/绝对路径/链接，
以及篡改外层成员、伪造源码但更新成员哈希、伪造镜像可发布标志和未引用的额外镜像。
已有输出保持原字节，临时文件不残留；工作树中的临时私有文件没有进入包。
这些是内容一致性测试，不能认证上传者或证明清单所述命令真实执行；可信下载来源、指定CI记录与实际运行观察仍须独立保留。

完整二进制组件清单、第三方NOTICE、外部对应源码/构建材料与实际镜像字节绑定尚未实现；
本包不构成完整Corresponding Source或二进制发布许可验收。归档保留CI已有文件的实际范围，
不声称补回未保存的中间镜像或每一轮串口。五类基线、多核、数值门槛/稳定性与九轨退出仍按各自契约处理。
当前状态见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)，本工具自身的干净远端结果须在后继合入验证中另记。


根工作区Cargo输入材料的后继见[依赖源码/NOTICE收集](2026-11-08-stage7-cargo-materials.md)：
只补充根锁定图的原始归档及文本候选/缺口，不将本片历史源码包改称完整二进制材料。
共享发布器被原样提取复用，既有七项测试仍通过；旧包仍按自身工具版本与哈希解释。
