#[allow(dead_code)]
mod agent_confirmation;
#[allow(dead_code)] // used end-to-end by Phase 3+ (SendText handler) and gRPC handler
mod contact_session;
mod crypto;
#[allow(dead_code)]
mod e2e_crypto;
mod invitation;
// Briefing 044g.1a: versioned schema migrations (sgx_migrations table).
mod migrations;
mod protocol;
mod queue_store;
mod service;
mod smp_client;
#[allow(dead_code)]
mod smp_commands;
#[allow(dead_code)]
mod smp_protocol;
mod tls_verifier;

use clap::Parser;
use sgx_proto::messenger::v1::messenger_service_server::MessengerServiceServer;
use tonic::transport::Server;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "sgx-simplex", about = "SimpleGoX SimpleX sidecar")]
struct Args {
    /// gRPC listen port
    #[arg(short, long, default_value_t = 50053)]
    port: u16,

    /// Data directory for SQLite and keys
    #[arg(long, default_value = "simplex-data")]
    data_dir: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let addr = format!("127.0.0.1:{}", args.port).parse()?;

    info!("Starting SimpleX sidecar on {addr}");
    info!("  data-dir: {:?}", args.data_dir);

    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("simplego-x")
        .join(&args.data_dir);
    std::fs::create_dir_all(&data_dir)?;

    let store = queue_store::QueueStore::open(&data_dir)?;
    let svc = service::SimplexService::new(store);

    // Briefing 044g.2: respawn persisted contacts before gRPC starts.
    // The auto-resume foundation (044d/e/f/g.1a/g.1a-fix1/g.1b) put every
    // key the BG-loop needs on disk; this call iterates connections WHERE
    // to_subscribe=1 AND <required-fields IS NOT NULL> and spawns
    // ContactSession tasks for each. Best-effort: a query-level error
    // logs but does not abort the sidecar (fresh handshakes still work).
    match svc.spawn_persisted_contacts().await {
        Ok(counts) => {
            info!(
                spawned = counts.spawned,
                skipped = counts.skipped,
                failed = counts.failed,
                "Boot-spawn: persisted contacts initialised"
            );
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                "Boot-spawn: query failed, no contacts spawned (sidecar continues)"
            );
        }
    }

    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(sgx_proto::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    Server::builder()
        .add_service(reflection)
        .add_service(MessengerServiceServer::new(svc))
        .serve(addr)
        .await?;

    Ok(())
}
