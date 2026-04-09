fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use locally installed protoc
    let protoc = if cfg!(target_os = "windows") {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
        format!("{}/protoc/bin/protoc.exe", home)
    } else {
        "protoc".to_string()
    };
    std::env::set_var("PROTOC", &protoc);

    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/messenger.proto"], &["proto/"])?;
    Ok(())
}
