use bootloader_api::{BootloaderConfig, config::Mapping};

/// 传给 `entry_point!` 的 bootloader 配置：启用物理内存直映射。
pub(crate) const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::new_default());
    config
};
