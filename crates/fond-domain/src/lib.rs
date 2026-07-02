//! Domain types, traits, and errors for fond.
//!
//! This crate contains pure data structures and type definitions
//! with no I/O or side effects. All entities that flow through
//! fond are defined here.

mod edit;
mod emitter;
mod error;
pub mod filter;
mod parser;
pub mod recipe;
mod slug;
pub mod user;

pub use edit::*;
pub use emitter::*;
pub use error::*;
pub use filter::*;
pub use parser::*;
pub use recipe::*;
pub use slug::*;
pub use user::*;
