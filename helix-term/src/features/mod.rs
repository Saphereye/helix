//! Fork-specific features live in self-contained submodules.
//!
//! Each feature owns its command(s), hooks, and UI helpers. Register new
//! features in [`setup`] and export public command fns for `commands.rs`.
//!
//! ```text
//! features/my_feature/mod.rs   — impl + register_hooks()
//! commands.rs                  — pub use crate::features::my_feature::my_command;
//! keymap/default.rs            — bind keys to the command
//! ```

pub mod breadcrumb;
pub mod local_search;

use crate::handlers::Handlers;

/// Register event hooks for all fork features.
pub fn register_hooks(handlers: &Handlers) {
    breadcrumb::register_hooks(handlers);
}
