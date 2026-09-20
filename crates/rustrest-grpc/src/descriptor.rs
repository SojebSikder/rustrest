//! Building `prost_reflect::DescriptorPool`s: one bootstrap pool for the
//! well-known `grpc.reflection.v1alpha` service (embedded here so discovery
//! needs no local `protoc`/network fetch), and one helper for compiling
//! user-supplied `.proto` files the same way.

use prost_reflect::DescriptorPool;
use std::path::{Path, PathBuf};

const REFLECTION_V1_PROTO: &str = include_str!("reflection_v1.proto");
const REFLECTION_V1ALPHA_PROTO: &str = include_str!("reflection_v1alpha.proto");

/// Compiles the embedded `grpc.reflection.v1` and `grpc.reflection.v1alpha`
/// service definitions into one descriptor pool, used to talk to a server's
/// reflection service before we know anything else about it. Both are
/// included since `v1alpha` is deprecated but still the only one some older
/// servers implement, while newer ones (recent grpc-go in particular) often
/// only register `v1`.
pub fn reflection_pool() -> Result<DescriptorPool, String> {
    let dir = std::env::temp_dir().join("rustrest-grpc-reflection-proto");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create temp dir: {e}"))?;

    let v1_path = dir.join("reflection_v1.proto");
    std::fs::write(&v1_path, REFLECTION_V1_PROTO)
        .map_err(|e| format!("Failed to write embedded proto: {e}"))?;
    let v1alpha_path = dir.join("reflection_v1alpha.proto");
    std::fs::write(&v1alpha_path, REFLECTION_V1ALPHA_PROTO)
        .map_err(|e| format!("Failed to write embedded proto: {e}"))?;

    let fds = protox::compile([&v1_path, &v1alpha_path], [&dir])
        .map_err(|e| format!("Failed to compile reflection protos: {e}"))?;
    DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| format!("Failed to build reflection descriptor pool: {e}"))
}

/// Compiles user-imported `.proto` files (plus any of their sibling
/// directories as include paths) into a descriptor pool covering every
/// service/message they define.
pub fn compile_proto_files(files: &[PathBuf]) -> Result<DescriptorPool, String> {
    if files.is_empty() {
        return Err("No .proto files selected".to_string());
    }

    let mut include_dirs: Vec<PathBuf> = files
        .iter()
        .filter_map(|f| f.parent().map(Path::to_path_buf))
        .collect();
    include_dirs.sort();
    include_dirs.dedup();

    let fds = protox::compile(files, &include_dirs)
        .map_err(|e| format!("Failed to compile .proto files: {e}"))?;
    DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| format!("Failed to build descriptor pool: {e}"))
}
