use crate::{manifest::load_manifest, packager::{build_archive, write_archive}};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub async fn run(output: Option<String>) -> Result<()> {
    let dir = std::env::current_dir()?;
    let manifest = load_manifest(&dir)?;
    let hand = &manifest.hand;

    println!("Packaging {} v{} ...", hand.id, hand.version);

    let (bytes, checksum) = build_archive(&dir)?;
    let out_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join(format!("{}-{}.tar.gz", hand.id, hand.version)));

    write_archive(&bytes, &out_path)?;

    println!("✓ Archive: {}", out_path.display());
    println!("  Size:     {} bytes", bytes.len());
    println!("  SHA-256:  {}", checksum);
    Ok(())
}
