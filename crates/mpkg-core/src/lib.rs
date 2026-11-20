//! mpkg-core: rootfs-agnostic core library for the Rustpackages package manager.

pub mod archive;
pub mod error;
pub mod lockfile;
pub mod manifest;
pub mod remote;
pub mod repo;
pub mod resolve;
pub mod sign;
pub mod store;

pub use error::{MpkgError, Result};
