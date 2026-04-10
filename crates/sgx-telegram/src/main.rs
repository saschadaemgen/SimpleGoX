#![doc = "SimpleGoX Telegram sidecar - bridges TDLib to the unified gRPC messenger protocol."]

mod auth;
mod convert;
mod service;
mod td;

use clap::Parser;
use sgx_proto::messenger::v1::messenger_service_server::MessengerServiceServer;
use tonic::transport::Server;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "sgx-telegram", about = "SimpleGoX Telegram sidecar")]
struct Args {
    /// gRPC listen port
    #[arg(short, long, default_value_t = 50051)]
    port: u16,

    /// TDLib database directory
    #[arg(long, default_value = "tdlib-data")]
    data_dir: String,

    /// Telegram API ID (from my.telegram.org)
    #[arg(long, env = "TG_API_ID")]
    api_id: i32,

    /// Telegram API hash (from my.telegram.org)
    #[arg(long, env = "TG_API_HASH")]
    api_hash: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let addr = format!("127.0.0.1:{}", args.port).parse()?;

    let cwd = std::env::current_dir().unwrap_or_default();
    info!("Starting Telegram sidecar on {addr}");
    info!("  CWD: {:?}", cwd);
    info!("  data-dir: {:?}", args.data_dir);

    let svc = service::TelegramService::new(args.api_id, &args.api_hash, &args.data_dir).await?;

    Server::builder()
        .add_service(MessengerServiceServer::new(svc))
        .serve(addr)
        .await?;

    Ok(())
}
