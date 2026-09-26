//! Generates `embedded_assets.rs`: every file under `assets/` as `(path, bytes)`,
//! so release builds ship as a single executable.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets");
    println!("cargo:rerun-if-changed={}", root.display());

    let mut files = Vec::new();
    // Debug builds read assets from disk, so skip the (slow to compile) byte blobs.
    if env::var("PROFILE").as_deref() == Ok("release") {
        collect(&root, &mut files);
        files.sort();
    }

    let entries: String = files
        .iter()
        .map(|path| {
            let relative = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            format!("    ({relative:?}, include_bytes!({:?})),\n", path.display().to_string())
        })
        .collect();

    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("embedded_assets.rs");
    fs::write(out, format!("pub const ASSETS: &[(&str, &[u8])] = &[\n{entries}];\n")).unwrap();
}
