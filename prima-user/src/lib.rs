#![no_std]
#![forbid(unsafe_code)]

//! 原生服务的可 host 验证逻辑；ring3 入口与 I/O 留在各二进制中。
pub mod staging;
