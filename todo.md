# TPT QR Batch — Build Todo

> Hub listing: `tptsolutions.co.nz/tools/qr-batch` (registry slug `qr-batch`,
> category `utility`). Spec: `apps.md` §T2-14 in the tpt-electrician-nz-2
> repo. Free version: one code at a time. Paid offline edition ($9 USD,
> Gumroad): bulk CSV generation, branding/logo, print-ready sheets. UI on
> `tpt-appfront`.
>
> Hub contract: `wasm-bindgen --target web` glue exposes `mount(container)`
> (called by `WasmAppRunner` after `default()` init). Standalone `trunk serve`
> mounts into the `#tpt-appfront-root` marker div instead — never
> `document.body` — so the two hosts never double-mount.

## Phase 0 — Scaffold & pipeline proof
- [x] `tpt-appfront init` scaffold (restructured by hand to the mirror apps'
      workspace layout — path deps on the tpt-appfront checkout)
- [x] Restructure scaffold to hub contract: `#[wasm_bindgen] pub fn mount(el)` + `#[wasm_bindgen(start)]` no-op unless `#tpt-appfront-root` present
- [x] `index.html` carries the `#tpt-appfront-root` marker div
- [x] Build pipeline script mirroring fea-lite/cutlist-optimizer's (`--target web` glue → web repo `public/apps/qr-batch/`)
- [x] Hello-world build loads through the standalone dev page (browser-verified
      via the hub-contract test page + the Pro exe smoke test; `trunk serve`
      kept stalling in this environment, so standalone verification used the
      trunk release bundle served statically)
- [x] git init + first commit

## Phase 1 — Core engine
- [x] Choose/vet a QR/barcode generation crate that compiles to wasm32
      (`qrcodegen` 1.8 + `barcoders` 2.0, both no_std-capable, zero-dep)
- [x] Adapter crate `engine/`: data string(s) + branding spec in → QR/barcode
      image(s) out; CSV in → batch of codes out — as plain functions, no UI
      deps, host-unit-testable (`tpt-qr-batch-engine`)
- [x] Print-ready sheet layout: grid layout with configurable label size,
      spacing, page size (vector PDF writer, A4/Letter, cut guides)
- [x] Validation: generated codes scan correctly — verified with two
      independent decoders (rqrr round-trip tests in CI + OpenCV against the
      actual browser-downloaded PNGs), including logo-overlaid codes

## Phase 2 — Free tier
- [x] Single data-string input + one QR/barcode generation, download result
      (PNG + SVG; QR, Code 128, EAN-13, Code 39)
- [x] Free-tier upsell copy pointing at bulk CSV generation, branding/logo,
      print-ready sheets
- [x] Browser-verified: mount, generate, code scans correctly (14/14
      automated checks incl. downloads + error paths)

## Phase 3 — Paid desktop edition (cargo feature `pro`)
- [x] `pro` gates: bulk CSV import/generation, custom branding/logo overlay
      on codes, print-ready sheet layout export (PDF + bulk-PNG ZIP)
- [x] Pro UI: CSV upload, branding settings, sheet-layout preview (stats),
      "Export PDF" (15/15 automated checks incl. CSV batch → PDF/ZIP
      round-trips and the logo-upload flow)
- [x] Desktop shell via `tpt-appfront-webview`; Windows exe build + smoke test
      (single-file exe, bundle embedded at compile time)
- [x] Gumroad product from apps.md listing copy ($9), exe upload — live at
      https://tpt12.gumroad.com/l/tpt-qr-batch (URL into registry still pending)

## Phase 4 — Ship & measure
- [x] Registry entry (`utility` category, `wasm: { entry: '/apps/qr-batch/qr-batch.js' }`)
      per the "Adding a finished app to the hub" runbook at the bottom of apps.md
- [x] Verify hub card/detail/assets in dev (`/tools` card, `/tools/qr-batch`
      detail, glue + wasm served 200). Note: the long-running dev instance on
      :3210 stalls every app's dynamic import (fea-lite/micro-vault too) —
      the runner contract itself is browser-verified against the real bundle
      (test page mirrors `WasmAppRunner` exactly). Final prod check after deploy.
- [x] WASM size budget check on the shipped build (free: 135.5 KB gzipped vs
      the ~400 KB budget; pro: 194 KB gz, desktop-embedded)
