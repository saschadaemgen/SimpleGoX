#![recursion_limit = "256"]
//! # SimpleGoX IoT
//!
//! Companion tooling for SimpleGoX ESP32 Matrix IoT gadgets.
//!
//! The actual ESP32 firmware is written in C using MatrixClientLibrary.
//! This crate provides host-side tools for:
//! - Device provisioning (generating Matrix credentials)
//! - Testing (sending commands to IoT rooms)
//! - Monitoring (watching IoT sensor data in Matrix rooms)

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "sgx-iot",
    about = "SimpleGoX IoT - ESP32 Matrix gadget toolkit",
    version
)]
struct Cli {
    /// Placeholder for future subcommands
    #[arg(short, long, default_value = "status")]
    action: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    println!("SimpleGoX IoT toolkit");
    println!("Coming in Season 1.5 - ESP32 Matrix gadgets!");

    Ok(())
}
