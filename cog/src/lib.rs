//! Obsidian vault management: syncing, tagging, and tidying notes.

pub mod config;
pub mod error;
pub mod fsutil;
pub mod note;
pub mod organise;
pub mod state;
#[cfg(feature = "steam")]
pub mod steam;
pub mod sync;
pub mod tags;
pub mod title;
pub mod vault;

pub use error::{Error, Result};
