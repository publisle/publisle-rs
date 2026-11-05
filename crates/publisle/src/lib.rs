#![doc = include_str!("../README.md")]
#[cfg(feature = "math")]
pub use publisle_math as math;
#[cfg(feature = "pdf")]
pub use publisle_pdf::{compile, model};
