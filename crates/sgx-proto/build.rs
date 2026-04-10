fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use PROTOC env var if set, otherwise rely on system PATH.
    // Install protoc: https://github.com/protocolbuffers/protobuf/releases
    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/messenger.proto"], &["proto/"])?;
    Ok(())
}
