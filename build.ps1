# TPT QR Batch - build pipeline (Windows).
#
# Outputs (separate directories - they never overwrite each other):
#   dist\hub\      free wasm bundle for the website
#                  (qr-batch.js + qr-batch_bg.wasm)
#   dist\          trunk bundle the Pro exe embeds (-Pro)
#   release\TPT QR Batch Pro\   staged exe package (-Pro)
#   release\TPT-QR-Batch-Pro.zip  zip-ready for Gumroad (-Pro)
#
# Website upload:
#   .\build.ps1 -WebRepo D:\path\to\web-repo
#     (or set $env:TPT_WEB_REPO and just run .\build.ps1)
#   -> copies dist\hub\* into <web repo>\public\apps\qr-batch\
#   then set the app's registry entry in the web repo:
#     status: 'live'
#     wasm: { entry: '/apps/qr-batch/qr-batch.js' }
#
# Pro edition:  .\build.ps1 -Pro
#   pro wasm (bulk CSV + branding/logo + print sheets), trunk bundle
#   embedded into the exe, and a staged+zip desktop package.

[CmdletBinding()]
param(
    # Build the Pro bundle (--features pro) instead of the free one.
    [switch]$Pro,
    # Only run the wasm-bindgen step against an existing release build.
    [switch]$SkipBuild,
    # Web repo checkout; the hub bundle is copied into it.
    # Defaults to $env:TPT_WEB_REPO.
    [string]$WebRepo = $env:TPT_WEB_REPO
)

$ErrorActionPreference = "Stop"
Set-Location -LiteralPath $PSScriptRoot

function Show-GzipSize {
    param([string]$WasmPath)
    $raw = (Get-Item $WasmPath).Length
    Add-Type -AssemblyName System.IO.Compression
    $rawBytes = [System.IO.File]::ReadAllBytes($WasmPath)
    $memoryStream = [System.IO.MemoryStream]::new()
    $gzipStream = [System.IO.Compression.GZipStream]::new($memoryStream, [System.IO.Compression.CompressionLevel]::Optimal)
    $gzipStream.Write($rawBytes, 0, $rawBytes.Length)
    $gzipStream.Dispose()
    $gzipped = $memoryStream.ToArray().Length
    $memoryStream.Dispose()
    $gzKb = [math]::Round($gzipped / 1KB, 1)
    Write-Host "-- $(Split-Path $WasmPath -Leaf): $raw bytes raw, $gzipped bytes gzipped ($gzKb KB gz)" -ForegroundColor Cyan
    if ($gzipped -gt 400KB) {
        Write-Warning "payload exceeds the ~400 KB gzip target for first-load comfort"
    }
}

$Crate = "tpt-app-qr-batch"
$Target = "wasm32-unknown-unknown"
$OutName = "qr-batch"

$HubDir = Join-Path $PSScriptRoot "dist\hub"
$Dest = if ($WebRepo) { Join-Path $WebRepo "public\apps\qr-batch" } else { $HubDir }

Write-Host "== TPT QR Batch build ==" -ForegroundColor Cyan
Write-Host " edition: $(if ($Pro) { 'pro (bulk CSV + branding/logo + print sheets)' } else { 'free (one code at a time)' })"
Write-Host " hub bundle:  $HubDir"
if ($WebRepo) { Write-Host " deploy to:   $Dest" }

if (-not $SkipBuild) {
    Write-Host "-- cargo build --release --target $Target" -ForegroundColor Cyan
    if ($Pro) {
        cargo build --release --target $Target -p $Crate --features pro
    } else {
        cargo build --release --target $Target -p $Crate
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}

$Wasm = Join-Path $PSScriptRoot "target\$Target\release\tpt_app_qr_batch.wasm"
if (-not (Test-Path $Wasm)) { throw "missing $Wasm - run the cargo build step first" }

if ($Pro) {
    # The Pro wasm is bundled by trunk below; dist\hub keeps the FREE bundle
    # for the website and is deliberately left untouched by -Pro builds.
    Write-Host "-- hub bundle left untouched (the website always gets the free edition)" -ForegroundColor Cyan
} else {
    # The hub bundle is always written to dist\hub (never clobbered by trunk),
    # then copied into the web repo when one is configured.
    Write-Host "-- wasm-bindgen --target web --out-name $OutName -> dist\hub" -ForegroundColor Cyan
    New-Item -ItemType Directory -Force -Path $HubDir | Out-Null
    wasm-bindgen --target web --out-name $OutName --out-dir $HubDir $Wasm
    if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen failed" }

    $Glue = Join-Path $HubDir "$OutName.js"
    $BgWasm = Join-Path $HubDir "$($OutName)_bg.wasm"
    foreach ($artifact in @($Glue, $BgWasm)) {
        if (-not (Test-Path $artifact)) { throw "wasm-bindgen did not produce $artifact" }
    }

    if ($WebRepo) {
        Write-Host "-- copying hub bundle into $Dest" -ForegroundColor Cyan
        New-Item -ItemType Directory -Force -Path $Dest | Out-Null
        Copy-Item $Glue, $BgWasm -Destination $Dest -Force
    }

    # Payload size report (target < ~400 KB gzipped).
    Show-GzipSize -WasmPath $BgWasm
}

if ($Pro) {
    Write-Host "-- trunk build --release --features pro --dist dist" -ForegroundColor Cyan
    trunk build --release --features pro --dist "dist"
    if ($LASTEXITCODE -ne 0) { throw "trunk build failed" }
    $desktopWasm = Get-ChildItem "dist" -Filter "*_bg.wasm" -Recurse | Select-Object -First 1
    if ($desktopWasm) { Show-GzipSize -WasmPath $desktopWasm.FullName }

    Write-Host "-- cargo build --release -p tpt-qr-desktop (webview exe)" -ForegroundColor Cyan
    cargo build --release -p tpt-qr-desktop
    if ($LASTEXITCODE -ne 0) { throw "desktop build failed" }
    $exe = Join-Path $PSScriptRoot "target\release\tpt-qr-batch-pro.exe"

    # Stage a portable, upload-ready package (the exe embeds its bundle; the
    # dist copy is a convenience for anyone who wants the raw web assets).
    $stageDir = Join-Path $PSScriptRoot "release\TPT QR Batch Pro"
    New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
    Copy-Item $exe $stageDir -Force
    Set-Content -Path (Join-Path $stageDir "README.txt") -Value @(
        "TPT QR Batch Pro",
        "",
        "Run tpt-qr-batch-pro.exe.",
        "Requires Windows 10+ with the WebView2 runtime",
        "(preinstalled on Windows 10 20H2+ and Windows 11)."
    )
    $zip = Join-Path $PSScriptRoot "release\TPT-QR-Batch-Pro.zip"
    Compress-Archive -Path $stageDir -DestinationPath $zip -Force
    Write-Host "   package: $stageDir" -ForegroundColor Cyan
    Write-Host "   zip:     $zip" -ForegroundColor Cyan
}

Write-Host "-- done" -ForegroundColor Green
Write-Host "   website upload: dist\hub\ holds the FREE bundle — copy its two files into the"
Write-Host "   web repo's public\apps\qr-batch\ (or rerun with -WebRepo <path>)."
if ($Pro) {
    Write-Host "   (Pro wasm is embedded into the exe + the release package; dist\hub was left as the free edition.)"
}
