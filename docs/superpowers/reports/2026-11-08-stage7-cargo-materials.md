---
id: report-stage7-cargo-materials
kind: report
authority: scoped-tooling-and-validation
status: cargo-input-materials-and-gaps-verified
lifecycle: historical
updated: 2026-11-08
---

# 7-0 根Cargo图依赖源码与NOTICE材料

## 结论与范围

已为根工作区68包Cargo图收集60份原始注册表.crate、110份许可/NOTICE文本候选，
逐个匹配Cargo.lock校验和；材料包创建、复核、独立重哈希及重复构建字节一致均通过。
57个注册表归档各自含候选文本，另3个没有找到随该归档提供的文本，缺口明确保留。
包不包含可运行镜像，image_release_eligible=false；这不是完整二进制组件清单或镜像许可签署。

本片承接[源码/验收证据包首片](2026-11-08-stage7-evidence-package.md)，
只补充根Cargo锁定输入的源码/文本材料，不关闭外部源码、镜像字节绑定、多核/门槛或任何子轨退出。
不改变依赖版本、原语、ABI、内核、QEMU探针或门限，不发布GitHub Release。
本文是同一会话的工程复核，不是法律归属/兼容性裁决、供应链签名或独立第三方审查。

## 基线、方法与实现

起点98602f62edc5a90dafb99aeb3f9df43144d10531，工作树干净；该提交为前片工作分支，
不把它等同于此前主分支73056ba的干净CI证据。
依据[实现前契约](../specs/2026-11-08-stage7-cargo-materials.md)、
[S7-0.3计划](../plans/2026-11-06-stage7-execution.md)、
[CI身份规则](../specs/2026-11-06-stage7-ci-evidence.md)和[许可登记](../../architecture/licensing.md)。

Python3.12.3、cargo1.99.0-nightly（3efb1f477 2026-07-17）、pinned nightly-2026-07-30。
精确命令为cargo metadata --locked --offline --format-version 1 --all-features。
包集合与根Cargo.lock精确对应，保留未过滤的平台条件、features、依赖kind/target；
包括未必进入当前x86_64镜像的Windows/wasm等条件输入，不将该超集冒充实际链接组件。

- 新增scripts/package_cargo_materials.py，复用CI环境清理/源码身份与前片路径、tar读取和确定性独占发布器；只是把原发布逻辑提成共用publish，既有源码证据包测试仍通过。
- 根/成员Cargo.toml、锁、工具链及仓库Cargo配置逐份与指定Git提交blob相等；其它工作树改动如实记录dirty。收集期间源码变化拒绝，输入manifest未提交变动不参与该提交的材料包。
- 注册表只接受当前crates.io源身份；从Cargo注册表cache读取原始.crate，先验证锁定SHA-256，再读取归档内部Cargo.toml及文本。已解包目录中的声明/文本不是材料来源；没有联网补包、改锁、继承其它归档许可或选择某个OR分支。
- 保留60份原始归档、Cargo解析图、输入配置、原始声明及VCS线索，递归收集LICENSE/LICENCE/COPYING/COPYRIGHT/NOTICE命名路径和显式license-file；crc-catalog的LICENSES子目录在范围内。
- NOTICE.txt为材料索引，随包保留已有项目三份许可全文和工具源码；三处逐归档缺口仍按各自包记录，项目文本不填补外部缺口。
- --verify在本地Git对象中复查输入，再按锁文件重哈希全部归档并重导声明、文本集合、缺口与索引；更新外层哈希不足以让替换的依赖归档通过。源码/归档均不解包执行。
- CI host增加新七项测试，保留前片七项及原四命令，仍是九组预算，不改变镜像作业。

## 材料与可见缺口

| 项 | 实际范围 |
|---|---|
| 根Cargo包 | 68：8个workspace包、60个注册表输入 |
| 原始.crate | 60份，共9789629字节；每份SHA-256与Cargo.lock一致 |
| 外部文本候选 | 110份，来自57个注册表归档；逐份与原归档字节一致 |
| 材料包 | 191个普通文件成员，共10627000字节载荷；无镜像字节 |
| 锁文件 | 14338字节，SHA-256=14fb94fcb38f782eac8feead100db951751a5339d082177e6b47aab35fb45b7c |

| 逐归档文本缺口 | 锁定版本/VCS线索 | 当前处理 |
|---|---|---|
| bootloader-boot-config | 0.11.17；ec8a8b4b59bd94f3c0280adc1bcdae530251b003，common/config | 同版本bootloader归档有根许可全文，但本工具不自动将其它归档文本继承为本包材料 |
| bootloader_api | 0.11.17；同一VCS提交，api | 保留声明和路径，待后继核对根许可的适用/来源材料 |
| r-efi | 6.0.0；7e1b0322d31d625f81a5656096330934f9cd835d | 保留MIT OR Apache-2.0 OR LGPL-2.1-or-later原声明及VCS线索，不用声明替代许可文本 |

这里的缺口是“该发布归档中未发现候选文本”，不是对权利归属、代码未许可或其它材料适用性的裁决。
VCS字段来自校验过的发布归档，不表示本片重新抓取/验证了远端Git对象。

明确排除：bootloader build.rs另行cargo install的五条构建图、Rust工具链/标准库/build-std、
独立prima-rust-probe的锁/依赖图，以及musl/sbase/glibc/busybox/Linux等外部材料和实际二进制链接映照。
独立Rust探针此前缺license字段的事实不因根工作区材料包而改写。完整镜像NOTICE、对应外部源码及镜像字节绑定仍待完成。

## 本次命令与结果

正式构包、host/lint均记录revision=98602f6、dirty=true，同一源码摘要
f29f7495d21bf3409abdd5022c6bf448f100006dc53b21b5daaab553baecb9ca。
当时包括新工具/测试、共享发布器提取、CI host入口及实现前契约；之后只补录本报告、阶段/索引、许可说明和生成目录。
这不是新提交自身干净远端证据；未重跑QEMU或Lean，不将宿主收集等同于新的OS运行/证明。

| 命令 | 本次实跑 | 运行ID / 秒数 |
|---|---|---|
| python3 scripts/test_package_cargo_materials.py | 退出0，7项测试；覆盖原归档与解包文本隔离、缺口、缓存缺失/篡改/链接、图/源/版本错配、归档逃逸、重写外层哈希后替换归档/索引/删除Cargo配置及旧包不覆盖 | 聚焦测试 |
| python3 scripts/test_package_evidence.py | 退出0，既有7项通过 | 共用发布器回归 |
| python3 scripts/package_evidence.py --verify target/release/prima-source-evidence-73056ba-v1.tar.gz | 退出0，旧真实源码证据包仍为九组/97份串口 | 既有包格式复核 |
| python3 scripts/ci.py host | 退出0，Rust1142通过/0失败/1默认忽略；musl7、POSIX std3、CI运行器11、证据包7、Cargo材料7通过 | e9155e3c697a464e8f9fef80fb0068dd / 11.702 |
| python3 scripts/ci.py lint | 退出0，fmt/Clippy/裸机构建/44文件零unsafe与人格护栏通过 | d534364792d3411a993cf0e365107288 / 4.183 |
| python3 scripts/package_cargo_materials.py --output target/release/prima-cargo-materials-98602f6-v1.tar.gz | 退出0：packages=68 registry=60 notice_text_gaps=3 images=0 | 真实离线缓存 |
| python3 scripts/package_cargo_materials.py --verify target/release/prima-cargo-materials-98602f6-v1.tar.gz | 退出0，同一计数 | Git对象、锁和内部材料复核 |
| 同构包命令，output改为prima-cargo-materials-98602f6-v1-repeat.tar.gz | 退出0，完整gzip字节相同 | 没有覆盖原包 |

独立标准库脚本不调用收集器：重算191个成员长度/SHA-256，对照包内Cargo.lock逐份重哈希60个原始归档，
再从归档逐项读取110份文本与拷贝字节比较；包集合、缺口、无镜像标志和重复gzip完整字节均核对通过。

正式包9860852字节，SHA-256=eab822fb64100d9f25759184f48dd1cddd15da0f3b3d598480c340a1f47aec52。
收集器源码15301字节，SHA-256=137ac9cd983ed2a5c42caca56a5aa91bad876f7219731b386a9c9015c1b7bd0b。
开发预览包不作为正式材料身份；所有包、完整日志和独立复核脚本只留在ignored target或/tmp。

TCB为2087 trusted / 39657 mechanism / 41744 total，5.00%；unsafe79/0，
include_bytes=0、prima_user_dep=false、service_name_lines=0、protocol_lines=56。
python3 scripts/docs.py self-test、build、check、check --base 98602f6及git diff --check均退出0；397份文档/3764节。
补录后重新生成并复核目录，不手改生成文件。

## 发现、限制与后继

静态复核发现本片开发期Cargo复核原型只按“包内是否含配置”决定配置集合，可能漏掉已提交配置。
已改为查询指定Git树的配置存在性；测试删除配置并同步更新外层成员表，仍必须失败。
这不是前片源码证据包v1的缺陷，也未据原型或预览包宣称正式验收通过。
其它反例明确拒绝源/锁/包错配和外层哈希更新后的材料替换；缺文本则保留为缺口，不能假报镜像可发布。

哈希与图结构检查只验证内容一致性，不认证上传者、不重新求解或证明Cargo解析边正确，也不证明编译/链接/运行。
源码缓存、工具链、外部源码材料、已提交状态和缺口变化须按契约复审，不能通过换源或放宽校验制造通过。
[阶段跟踪](../../../Prima%20OS%20实现阶段.md)保留镜像字节绑定、其它输入图、逐归档文本缺口、
多核基线、性能门槛/稳定性与九轨剩余工作；本片工具自身干净CI在后继合入中另记。
