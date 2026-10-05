# TPT QR Batch

Bulk QR/barcode generation — free in your browser, Pro desktop edition for
bulk CSV, branding and print-ready sheets.

- **Free web edition** (this repo's WASM build): one code at a time — QR,
  Code 128, EAN-13, Code 39 — PNG/SVG download. Live at
  `tptsolutions.co.nz/tools/qr-batch`.
- **Pro edition** ($9 one-time, Gumroad — `cargo feature pro`): bulk CSV
  generation, colour branding + centre-logo overlay, print-ready label
  sheets (vector PDF) and bulk-PNG ZIP export, in an offline Windows desktop
  app.

Built on [TPT AppFront](../tpt-appfront) (`tpt-appfront-dom` backend,
`tpt-appfront-webview` desktop shell). Generation happens entirely on-device
— no uploads, no tracking, no subscription.

## Layout

- `engine/` — `tpt-qr-batch-engine`: encoding (qrcodegen + barcoders),
  rendering (PNG/SVG), branding/logo compositing, CSV parsing, and the Pro
  PDF sheet writer + ZIP packager. Pure Rust, no UI deps, host-testable.
- `src/` — the DOM app (free tier + Pro-gated UI).
- `desktop/` — the Pro webview shell (single-file exe; embeds the trunk
  bundle).

## Develop

```
trunk serve          # standalone dev page (mounts via #tpt-appfront-root)
just test            # engine suite (free + pro features)
```

## Build & ship

```
./build.ps1                      # free WASM -> dist/hub (+ gzip size report)
./build.ps1 -WebRepo <web repo>  # ... and copy into public/apps/qr-batch/
./build.ps1 -Pro                 # pro trunk bundle + desktop exe + release zip
```

Hub registry entry (web repo `src/lib/tpt-apps-registry.ts`):

```
status: 'live'
wasm: { entry: '/apps/qr-batch/qr-batch.js' }
```

Gumroad listing copy lives in [GUMROAD.md](GUMROAD.md); the build plan in
[todo.md](todo.md).
