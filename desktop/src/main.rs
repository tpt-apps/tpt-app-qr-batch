//! TPT QR Batch Pro — desktop shell (paid edition).
//!
//! A native host binary that opens the OS webview (WebView2 on Windows) and
//! serves the `trunk build --release --features pro` output of the appfront
//! DOM bundle, embedded into the exe at compile time so the Gumroad download
//! is a single self-contained file. The same DOM bundle ships to the hub for
//! the free WASM edition; the Pro difference at runtime is that the bundle
//! is built with `--features pro` (bulk CSV, branding/logo, print sheets).
//!
//! Resolve order for the bundle directory:
//! 1. `TPT_QR_DIST` environment variable,
//! 2. `dist\desktop` next to the current working directory (the pipeline's
//!    trunk output),
//! 3. `dist` next to the current working directory (legacy layout),
//! 4. `dist` next to the executable (portable package layout),
//! 5. the embedded copy extracted to a temp directory (single-file exe).

use std::path::PathBuf;

use include_dir::{include_dir, Dir};
use tpt_appfront_webview::{AppBuilder, WindowConfig};

const APP_ID: &str = "nz.co.tptsolutions.qr-batch-pro";

/// The `trunk build --release --features pro` output (dist\desktop),
/// embedded at compile time. `build.ps1 -Pro` runs the trunk build before
/// this crate, so that directory always exists by the time this macro
/// expands.
static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/../dist/desktop");

fn main() {
    let dist_dir = resolve_dist_dir();
    if !dist_dir.join("index.html").exists() {
        eprintln!(
            "tpt-qr-batch-pro: no index.html under {} and embedded-asset extraction failed — \
             this build is missing its web bundle",
            dist_dir.display()
        );
        std::process::exit(1);
    }

    let result = AppBuilder::new(APP_ID)
        .with_window(WindowConfig {
            id: "main".to_string(),
            title: "TPT QR Batch Pro".to_string(),
            width: 1100,
            height: 860,
            dist_dir,
        })
        .with_single_instance(true)
        .run(|_action, _params| Ok(()));

    if let Err(error) = result {
        eprintln!("tpt-qr-batch-pro: {error:#}");
        std::process::exit(1);
    }
}

fn resolve_dist_dir() -> PathBuf {
    if let Ok(from_env) = std::env::var("TPT_QR_DIST") {
        return PathBuf::from(from_env);
    }
    let mut candidates = vec![PathBuf::from("dist").join("desktop"), PathBuf::from("dist")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("dist"));
        }
    }
    candidates
        .into_iter()
        .find(|dir| dir.join("index.html").exists())
        .unwrap_or_else(extract_embedded_dist)
}

fn extract_embedded_dist() -> PathBuf {
    let out_dir = std::env::temp_dir().join(format!("{APP_ID}-dist"));
    if !out_dir.join("index.html").exists() {
        let _ = std::fs::create_dir_all(&out_dir);
        if let Err(error) = DIST.extract(&out_dir) {
            eprintln!("tpt-qr-batch-pro: failed to extract embedded assets: {error:#}");
        }
    }
    out_dir
}
