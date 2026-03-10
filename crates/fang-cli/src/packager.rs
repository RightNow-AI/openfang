//! Creates a .tar.gz archive from a Hand package directory.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Build a .tar.gz archive from the given directory.
/// Returns (archive_bytes, sha256_hex_checksum).
pub fn build_archive(dir: &Path) -> Result<(Vec<u8>, String)> {
    let mut archive_bytes = Vec::new();
    {
        let gz = flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::best());
        let mut tar = tar::Builder::new(gz);

        for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let rel = path.strip_prefix(dir).unwrap();
                tar.append_path_with_name(path, rel)
                    .with_context(|| format!("Failed to add {} to archive", path.display()))?;
            }
        }
        tar.finish()?;
    }

    let mut hasher = Sha256::new();
    hasher.update(&archive_bytes);
    let checksum = format!("{:x}", hasher.finalize());

    Ok((archive_bytes, checksum))
}

/// Write the archive bytes to a file and return the path.
pub fn write_archive(bytes: &[u8], output_path: &Path) -> Result<()> {
    std::fs::write(output_path, bytes)
        .with_context(|| format!("Failed to write archive to {}", output_path.display()))
}
