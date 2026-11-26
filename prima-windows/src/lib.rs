#![no_std]
#![forbid(unsafe_code)]

//! Windows LibOS 的宿主可验证模型。
//!
//! 当前只有 WIN-M1 的 PE32+ 有界解析与映射计划，没有 Win32 API、装载器或运行证据。
//! 解析器只读字节切片，不分配、不执行、不假设文件已可信；所有 RVA 与长度都做溢出和
//! 文件范围检查。

pub mod load;
pub mod pe;
