use bootloader_api::{BootloaderConfig, config::Mapping};

/// 传给 `entry_point!` 的 bootloader 配置：启用物理内存直映射。
///
/// S2-D1 把 `BootstrapState` 扩大（槽位/generation 表、CapRecord 表、更大的 Audit 容量），
/// `create_state` 仍按值返回该结构，boot 早期（`kernel_main`/`create_state`/`install`）的
/// 组合栈帧随之增大；默认 80KiB 启动栈在 QEMU 上实测溢出（#PF/双重故障→三重故障）。
/// 这里把启动栈显式调大到 256KiB（128MiB 客户机内存下代价可忽略），并在
/// `bootstrap::tests::bootstrap_state_stays_within_the_stack_budget` 钉住结构大小上限。
pub(crate) const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::new_default());
    config.kernel_stack_size = 256 * 1024;
    config
};
