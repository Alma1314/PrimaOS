---
id: spec-stage7-cargo-materials
kind: specification
authority: scoped-tooling-contract
status: reviewed-for-cargo-input-materials
lifecycle: current
updated: 2026-11-08
---

# 7-0 Cargo 锁定输入源码与 NOTICE 材料

## 范围与实现前复核

承接[源码/验收证据包](2026-11-08-stage7-evidence-package.md)、
[首片实跑及许可缺口](../reports/2026-11-08-stage7-evidence-package.md)、
[执行计划S7-0.3](../plans/2026-11-06-stage7-execution.md)与[许可登记](../../architecture/licensing.md)。
已检查Cargo.lock、pinned Cargo metadata输出、注册表原始归档、bootloader构建脚本及独立Rust探针。
本片只增加宿主材料收集/复核工具，不改变依赖、OS机制、ABI、探针或QEMU门限。

命令为`python3 scripts/package_cargo_materials.py --output FILE.tar.gz`；使用既有CI环境清理规则，
执行pinned `cargo metadata --locked --offline --format-version 1 --all-features`。
范围是根工作区Cargo图及锁文件的注册表输入超集，保留未过滤的平台条件；不是最终镜像链接组件清单。
不递归声称覆盖build.rs另行cargo install的bootloader图、Rust工具链/标准库/build-std、
独立prima-rust-probe图、musl/sbase/glibc/busybox/Linux等外部材料。

## 内容绑定与材料边界

1. 记录Git revision/dirty/文件摘要及工具版本。Cargo.lock、根/成员Cargo.toml、rust-toolchain.toml、
   仓库.cargo/config.toml必须与该Git提交中的blob相同；其它工作树变动允许但如实标dirty，收集期间源码变化则失败。
2. metadata包集合必须精确匹配Cargo.lock，path包必须为本工作区成员；注册表只支持当前crates.io身份，
   不将git/path/vendor等其它源冒充注册表归档。保留解析节点的features、依赖kind与target条件，不据此推导实际链接。
3. 读取对应Cargo注册表缓存中的原始.crate，逐个核对锁文件SHA-256，再从归档内部读取Cargo.toml及材料。
   不信任已解包目录内的许可文本；缓存缺失/改变、包名/版本错配或图遗漏均失败，不联网补包或改锁。
4. .crate作为原始字节随包保留；附每个归档中LICENSE/LICENCE/COPYING/COPYRIGHT/NOTICE命名路径下的普通文件，
   以及显式license-file。扫描包括子目录，不按SPDX表达式自行选择许可或推断法律兼容性。
   没有找到文本时明确登记notice_text_gaps；缺声明另记，不以声明字符串冒充完整文本。
5. 包含可重放的锁/manifest配置、原始声明及VCS线索、归一化Cargo解析图、材料索引NOTICE.txt、工具源码与逐成员哈希。
   工具随附已有项目GPL/MPL/musl许可全文，不据这些项目文本填补第三方归档自己的缺口。
   image_release_eligible和binary_images_included始终false；NOTICE.txt只是该图的材料索引，不是完整镜像NOTICE。

## 创建与复核

复用源码/证据包的规范路径、非解包tar读取、固定gzip顺序/时间及独占发布；不覆盖旧包。
同样采用单成员64MiB、总载荷256MiB、10000成员上限；不接受归档逃逸、重复成员或链接。
`--verify FILE.tar.gz`核对成员集合与哈希，用本地Git对象重查锁/manifest，再对全部.crate重算锁定哈希，
重新导出声明、文本集合、缺口和NOTICE索引，防止仅更新外层成员哈希就把替换的依赖归档算作原输入。
复核不重新执行Cargo或解包源码，不证明编译/链接/运行或上传者真实性；`--repo`指定含该commit的仓库。

测试覆盖已解包文本与原归档隔离、锁/包/版本/图错配、缓存缺失/篡改、路径与链接拒绝、
子目录许可和缺口、成员/NOTICE篡改、更新外层哈希后的归档替换、确定性及旧输出保留。
运行根workspace host、lint、既有证据包测试及文档四项；真实68包/60注册表归档单独收集并独立重哈希。
这只是Cargo输入材料切片，不关闭二进制发布、镜像字节绑定、多核/性能门槛或任一阶段退出条件。
图范围、源类型、锁/摘要格式、文本识别或包格式变化时重审本契约。
