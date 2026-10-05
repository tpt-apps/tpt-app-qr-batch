# Gumroad listing

Everything needed to create/update the Gumroad product once this app is
built. Source copy is `apps.md` §T2-14 in the `tpt-electrician-nz-2` repo — if
that changes, update here too (and vice versa).

## Step-by-step: publish this listing

1. Build the Pro edition (`./build.ps1 -Pro`, once the build pipeline exists
   — see `todo.md` and mirror `tpt-app-fea-lite`/`tpt-app-cutlist-optimizer`'s
   pattern) — produces the packaged exe/zip in `release/`.
2. Log in to Gumroad → **New product** → **Digital product**.
3. Set the product name, price, and upload the packaged file (all below).
4. Paste the short description, bullets, and full description (below) into
   their respective fields.
5. Set category/tags (below) and add a cover image (see note below).
6. Publish, then copy the resulting product URL (`https://gum.co/...`).
7. Paste that URL into the `qr-batch` entry's `gumroadUrl` field in
   `tpt-electrician-nz-2\src\lib\tpt-apps-registry.ts`, and set `price: '$9'`
   on the same entry.
8. Flip that entry's `status` to `'live'` once both the free demo and this
   Gumroad listing exist.
9. Commit and push the registry repo.

## File to upload

`release\TPT-QR-Batch-Pro.zip` (built by `./build.ps1 -Pro`; contains the
single-file `tpt-qr-batch-pro.exe` — the web bundle is embedded in the exe —
plus a README). The zip is ~0.6 MB.

## Product name

```
TPT QR Batch — Bulk Branded QR Codes, No Subscription
```

## Price

**$9 USD, one-time purchase** (no subscription).

## Short description / summary field

```
Generate hundreds of branded QR codes at once from a spreadsheet — pay once, print forever.
```

## Bullets / feature list

```
Bulk CSV generation
Custom branding/logo
Print-ready sheets
No subscription
One-time purchase
```

## Full description (long-form field)

```
TPT QR Batch generates branded QR/barcodes in bulk from a CSV, with
consistent logo/color branding and print-ready sheet layout — a one-time
static generation tool, not a subscription tracking platform.

Who it's for: small retail and hospitality owners bulk-generating branded
QR codes — menus, product/asset labels, table tents. "Dynamic QR" platforms
charge monthly for tracking/editing features most small businesses never
use; they just need a static code that works.

Pro edition (this download):
- Bulk generation from CSV (free is one code at a time)
- Custom branding/logo embedding
- Print-ready sheet export (multiple codes per page)

Try the free single-code edition first, right in your browser, no install:
tptsolutions.co.nz/tools/qr-batch

Requirements: Windows 10 (20H2+) or Windows 11. Uses the WebView2 runtime,
preinstalled on both — nothing extra to download.
```

## Category / tags

- Category: **Software > Business & Money** (or Gumroad's closest equivalent)
- Tags: `qr-code-generator`, `barcode`, `small-business`, `retail-tools`,
  `hospitality-tools`, `windows`

## Cover image / thumbnail

Not created yet. A print sheet of several branded QR codes is the most
convincing cover once built. No stock/AI art.

## After publishing

1. Copy the resulting product URL (`https://gum.co/...`).
2. Paste it into the `gumroadUrl` field of the `qr-batch` entry in
   `tpt-electrician-nz-2\src\lib\tpt-apps-registry.ts`, and set `price: '$9'`.
3. Set `status` to `'live'`.
4. Commit and push the registry repo.
