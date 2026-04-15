pub mod messenger {
    pub mod v1 {
        tonic::include_proto!("messenger.v1");
    }
}

pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("messenger_descriptor");
