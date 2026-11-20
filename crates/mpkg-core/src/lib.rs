//! Rootfs-agnostic core of the mpkg package manager.
//!
//! Every operation takes a target root path, so the whole crate is testable
//! against a fake rootfs on the host — no VM required.

pub mod archive;
pub mod manifest;
pub mod repo;
pub mod resolve;
pub mod sign;
pub mod store;

pub use manifest::PkgManifest;
