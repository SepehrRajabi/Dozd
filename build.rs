//! Generates `embedded_assets.rs`: every file under `assets/` as `(path, bytes)`,
//! so release builds ship as a single executable. Also sets `DOZD_BUILD`, a
//! fingerprint of what the game is built from, so only identical builds play
//! together (see `net::VERSION`).

use std::path::{Path, PathBuf};
use std::{env, fs};

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        // Hidden files, like a Finder's `.DS_Store`, aren't part of the game.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// A fingerprint of everything that decides how the game plays: the same on
/// every machine and platform for the same files, whoever built them. Carriage
/// returns are skipped, as git may check text out with Windows line endings.
fn fingerprint(manifest: &Path) -> String {
    let mut files = Vec::new();
    collect(&manifest.join("src"), &mut files);
    collect(&manifest.join("assets"), &mut files);
    files.push(manifest.join("build.rs"));
    files.push(manifest.join("Cargo.toml"));
    files.push(manifest.join("Cargo.lock"));

    let mut named: Vec<(String, PathBuf)> = files
        .into_iter()
        .map(|path| (path.strip_prefix(manifest).unwrap().to_string_lossy().replace('\\', "/"), path))
        .collect();
    named.sort();

    // FNV-1a: small, and stable from one toolchain to the next.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |byte: u8| hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    for (name, path) in &named {
        name.bytes().for_each(&mut mix);
        mix(0);
        fs::read(path).unwrap().into_iter().filter(|byte| *byte != b'\r').for_each(&mut mix);
        mix(0);
    }
    format!("{:08x}", hash >> 32)
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    for watched in ["src", "assets", "build.rs", "Cargo.toml", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={}", manifest.join(watched).display());
    }
    println!("cargo:rustc-env=DOZD_BUILD={}", fingerprint(&manifest));

    let root = manifest.join("assets");

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
