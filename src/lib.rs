//! Service Locator
//!
//! # Examples
//! 
//! ```
//! use mini_di::service_locator::*;
//! let mut container = Container::new();
//! container.when::<u32>().clone(42).unwrap();
//! let value: u32 = container.as_locator().locate().unwrap();
//! assert_eq!(value, 42);
//! ```
//!

pub mod service_locator;

#[cfg(test)]
mod tests;
