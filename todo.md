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
- [ ] `tpt-appfront init tpt-app-qr-batch --target dom` (path deps on the tpt-appfront checkout)
- [ ] Restructure scaffold to hub contract: `#[wasm_bindgen] pub fn mount(el)` + `#[wasm_bindgen(start)]` no-op unless `#tpt-appfront-root` present
- [ ] `index.html` carries the `#tpt-appfront-root` marker div
- [ ] Build pipeline script mirroring fea-lite/cutlist-optimizer's (`--target web` glue → web repo `public/apps/qr-batch/`)
- [ ] Hello-world build loads through the standalone dev page (`trunk serve`)
- [ ] git init + first commit

## Phase 1 — Core engine
- [ ] Choose/vet a QR/barcode generation crate that compiles to wasm32
- [ ] Adapter crate `engine/`: data string(s) + branding spec in → QR/barcode
      image(s) out; CSV in → batch of codes out — as plain functions, no UI
      deps, host-unit-testable
- [ ] Print-ready sheet layout: grid layout with configurable label size,
      spacing, page size
- [ ] Validation: generated codes scan correctly with a standard phone camera

## Phase 2 — Free tier
- [ ] Single data-string input + one QR/barcode generation, download result
- [ ] Free-tier upsell copy pointing at bulk CSV generation, branding/logo,
      print-ready sheets
- [ ] Browser-verified: mount, generate, code scans correctly

## Phase 3 — Paid desktop edition (cargo feature `pro`)
- [ ] `pro` gates: bulk CSV import/generation, custom branding/logo overlay
      on codes, print-ready sheet layout export (PDF)
- [ ] Pro UI: CSV upload, branding settings, sheet-layout preview, "Export PDF"
- [ ] Desktop shell via `tpt-appfront-webview`; Windows exe build + smoke test
- [ ] Gumroad product from apps.md listing copy ($9), exe upload, URL into
      registry — manual step, listing copy at apps.md §T2-14

## Phase 4 — Ship & measure
- [ ] Registry entry (`utility` category, `wasm: { entry: '/apps/qr-batch/qr-batch.js' }`)
      per the "Adding a finished app to the hub" runbook at the bottom of apps.md
- [ ] Verify hub card/detail/sitemap/runner in dev, then prod after deploy
- [ ] WASM size budget check on the shipped build
