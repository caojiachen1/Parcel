//! Parcel Core — shared types, configuration schema, and utilities.
//!
//! This crate defines the `parcel.json` configuration structure that both
//! the CLI tool and the installer runtime rely on.

pub mod config;
pub mod safety;

pub use config::ParcelConfig;
