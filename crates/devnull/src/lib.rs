//! The `dn` client: a Neovim-flavoured terminal UI for [dev-null.sh](https://dev-null.sh).
//!
//! Design decisions and their reasons live in `docs/plan.md`; module docs link to the relevant entries.

pub mod app;
pub mod doc;
pub mod hl;
pub mod key;
pub mod keymap;
pub mod runtime;
