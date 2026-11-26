#![no_std]
#![forbid(unsafe_code)]

//! Linux x86_64 personality：Linux 号、errno、flags、stat/dirent 布局只在本 crate
//! 解释，经原生 Gate 控制与用户态 Store 会话落到通用机制；内核不识别该 personality。
//! 语义范围与证据以阶段 6/7 报告和 `docs/superpowers/compat/` 台账为准。

pub mod linux;
pub mod linux_store;
