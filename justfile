# TPT QR Batch - task runner.
# The pipeline logic lives in build.ps1 (Windows) / build.sh (unix); these
# recipes are thin wrappers so `just` works everywhere.

set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# Build the free WASM edition and report the gzip payload size.
build:
    pwsh -NoLogo -NoProfile -File build.ps1

# Build the Pro edition: bulk CSV + branding/logo + print sheets + desktop exe.
pro:
    pwsh -NoLogo -NoProfile -File build.ps1 -Pro

# Re-run only wasm-bindgen + copy (after an existing release build).
bindgen:
    pwsh -NoLogo -NoProfile -File build.ps1 -SkipBuild

# Engine suite (encode round-trips, CSV, PNG/ZIP/PDF) — free and pro.
test:
    cargo test -p tpt-qr-batch-engine
    cargo test -p tpt-qr-batch-engine --features pro

# Type-check the wasm lib (free + pro) and the desktop shell
# (desktop needs one `just pro` first — it embeds dist/ at compile time).
check:
    cargo check --target wasm32-unknown-unknown -p tpt-app-qr-batch
    cargo check --target wasm32-unknown-unknown --features pro -p tpt-app-qr-batch
    cargo check -p tpt-qr-desktop

# Live-reload standalone dev server (index.html mounts via #tpt-appfront-root).
dev:
    trunk serve

# Remove build outputs.
clean:
    cargo clean
    -rd /s /q dist release 2>nul || rm -rf dist release
