//! Typed client for the [dev-null.sh](https://dev-null.sh) API.
//!
//! Holds everything that talks to the server and nothing that draws: models, endpoints, device enrollment and
//! SSHSIG request signing. The design notes live in `docs/plan.md` (D8, D9 and "API notes").

pub mod body;
mod client;
pub mod enroll;
pub mod media;
pub mod model;
pub mod signing;

pub use client::{
    Answering, ApiError, Category, Client, Draft, InvalidOrigin, Log, MAX_SEARCH, ORIGIN,
    OrganizationTab, People,
};
