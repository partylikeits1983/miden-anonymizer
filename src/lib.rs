#![no_std]

//! # miden-anonymizer
//!
//! Pass-through account (PTA) on Miden. See the crate README for a design
//! overview. This crate exposes:
//!
//! - [`account::VaultEmptyAuth`] - custom auth component that asserts the
//!   account's vault is empty at the start and end of every transaction.
//! - [`account::PassThroughAccount`] - builder helper for constructing a
//!   public, immutable-code, regular account with `VaultEmptyAuth` +
//!   `BasicWallet`.
//! - [`note::P2idForwardNote`] - builds P2IDF (pay-to-ID-forward) notes that,
//!   when consumed by a PTA, emit a P2ID note to the final recipient.

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod account;
pub mod errors;
pub(crate) mod library;
pub mod note;

#[cfg(feature = "cli")]
pub mod cli;

pub use library::{pta_auth_library, pta_standards_lib};
