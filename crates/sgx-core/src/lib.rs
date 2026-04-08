#![recursion_limit = "256"]
#![allow(clippy::result_large_err)]
//! # sgx-core
//!
//! Shared Matrix client logic for all SimpleGoX products.
//!
//! This crate provides the common foundation used by:
//! - `sgx-terminal` - Dedicated Matrix hardware terminal
//! - `sgx-iot` - ESP32 Matrix IoT gadgets
//!
//! ## Architecture
//!
//! Built on top of [`matrix-sdk`], this crate handles:
//! - Client initialization and authentication
//! - E2E encryption lifecycle (via vodozemac)
//! - Persistent storage (SQLite)
//! - Sliding Sync for fast startup
//! - Homeserver configuration and management

pub mod client;
pub mod config;
pub mod error;

pub use client::{
    IncomingMessage, IncomingReaction, IotDevice, IotStatusPayload, RoomDetail, RoomMemberInfo,
    RoomSettings, RoomSummary, SgxClient, TypingPayload, UserProfile,
};
pub use config::SgxConfig;
pub use error::SgxError;
