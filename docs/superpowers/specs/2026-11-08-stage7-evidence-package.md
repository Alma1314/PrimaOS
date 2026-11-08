---
id: spec-stage7-evidence-package
kind: specification
authority: scoped-tooling-contract
status: reviewed-for-source-evidence-package
lifecycle: current
updated: 2026-11-08
---

# 7-0 源码与验收证据包首片

## 范围与实现前复核

承接[执行计划S7-0.3](../plans/2026-11-06-stage7-execution.md)、
[CI身份契约](2026-11-06-stage7-ci-evidence.md)、
[L1及本机超时链](../reports/2026-11-08-stage7-libos-benchmark.md)和
[仓库许可登记](../../architecture/licensing.md)。已复核CI的source_identity、逐步归档与失败状态、
Git归档/文件模式、许可文本和八crate声明；本片只增加宿主打包/复核工具。
不改OS、ABI、构建镜像或QEMU协议，不新增性能门槛，不勾选任何阶段退出项。

首片输出源码/证据包，包含指定Git提交的tracked源码快照、九组CI清单及其命令日志/归档串口，
以及实际许可文本和声明清单。现有Actions没有镜像字节，不能将其制品身份当成可运行镜像包。
完整二进制组件/第三方NOTICE、对应外部源码/构建材料、镜像字节与验收绑定继续由后继完成；
包内明确image_release_eligible=false。本片不创建GitHub Release或发布镜像。

## 输入与接受条件

`python3 scripts/package_evidence.py --revision COMMIT --evidence-root DIR --output FILE.tar.gz`：

1. COMMIT解析为完整commit，使用git archive取得历史快照，不复制工作树或target/Git凭证。
   按CI相同的路径字节排序、可执行位和文件/链接内容算法重算files_sha256；不靠commit字符串相同代替内容校验。
2. DIR恰含host/lint/docs/bios/linux/gate/gate-audit/domains/formal各一份schema_version=1清单。
   所有source均为指定commit、dirty=false、同一实算摘要；组/步骤passed、exit0且有真实非空命令列表。
   缺组、重复/未知组、混合版本/摘要、失败/中断/脏树或空步骤均拒绝。
3. 每一步命令日志及每份trace都必须存在，trace长度/SHA-256精确一致。只收清单引用的文件，
   不收同目录的旧日志、任意额外文件或镜像；制品清单只保留身份，不能标作镜像字节复核。
4. 所有引用路径必须规范相对路径，无绝对路径、回退、反斜杠和符号链接组件；归档不解包到磁盘。
   Git快照仅接受普通文件、目录和不逃出快照的相对符号链接；拒绝其它类型及重复路径。
   单文件64MiB、总载荷256MiB、10000成员上限；超限明确失败，不静默截断。
5. 附根LICENSE、prima-ostd/LICENSE-MPL及musl COPYRIGHT，校验现有登记哈希；
   从快照Cargo.toml逐项记录license原值，另列三个MPL文件的SPDX/哈希。
   crate字段、逐文件声明与现有许可政策分别保留，不由脚本裁决法律归属或宣称二进制合规。
6. 包内manifest记录源码身份、工具源码哈希、九组摘要、逐成员长度/哈希、许可事实和仍待二进制材料。
   同包附工具源码；校验和只建立内容一致性，不是签名、运行真实性证明或组织签署。

## 输出、复核与失败语义

外层tar.gz按固定顺序、mtime=0、uid/gid=0写普通文件；同一输入及工具版本应得到相同字节。
源码内层git archive保留指定提交自身的时间/模式，输出身份不受打包时工作树的未提交内容影响。
全部输入通过后在输出目录临时写入，最后独占发布；失败清理临时文件，不覆盖已有包。
`python3 scripts/package_evidence.py --verify FILE.tar.gz`重新检查成员集合/长度/哈希、
九组清单和trace，并用本地Git对象核对source.tar及源码摘要。`--repo`可指定包含该commit的仓库。
没有该Git对象时失败，不把内层归档自报的commit当作认证。

测试覆盖正例/确定性、历史提交与工作树隔离、真实CI摘要等价、缺组/混版/失败/脏状态、
缺失或篡改trace、路径逃逸/链接、归档篡改、许可文本缺失/改变和已有输出不覆盖。
实跑本地host/lint与文档四项检查，并用73056ba主分支真实九组结果构包、复核、独立核对成员。
此处仅复用该提交已实际通过的QEMU/Lean证据，不据宿主打包操作冒充本片新QEMU运行。
输入schema、CI分组、摘要算法、许可声明、包内容或输出类型变化时重审本契约。
