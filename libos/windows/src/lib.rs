#![no_std]
#![forbid(unsafe_code)]

//! Windows LibOS 的宿主可验证模型与 ring3 装载原型。
//!
//! PE32+ 有界解析、映射计划与平铺/重定位/IAT 填充是宿主可验证模型；`winpe` 是
//! WIN-M2 的 ring3 原型，只有启用 `winpe-probe` 时才编译。没有 Win32 API 或参考差分。
//! 解析器只读字节切片，不分配、不执行、不假设文件已可信；所有 RVA 与长度都做溢出和
//! 文件范围检查。

pub mod load;
pub mod pe;
pub mod teb;

#[cfg(feature = "winpe-probe")]
pub mod winpe;
