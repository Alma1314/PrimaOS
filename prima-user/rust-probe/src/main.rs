#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(all(feature = "std", not(feature = "posix")), feature(restricted_std))]
#![cfg_attr(not(feature = "posix"), no_main)]

#[cfg(feature = "std")]
#[cfg(not(feature = "posix"))]
mod pal;

#[cfg(not(feature = "posix"))]
include!("native.rs");
#[cfg(feature = "posix")]
include!("posix.rs");
