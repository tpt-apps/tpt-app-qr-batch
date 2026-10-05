#!/usr/bin/env sh
# TPT QR Batch - build pipeline (macOS/Linux).
#
# Free edition (default):  ./build.sh
# Pro desktop edition:     ./build.sh --pro
#   (adds trunk build --release --features pro + the native webview exe)
#
# Env:
#   TPT_WEB_REPO  path to the web repo; the bundle is copied to
#                 $TPT_WEB_REPO/public/apps/qr-batch/ (glue named
#                 qr-batch.js, wasm keeps its _bg.wasm name),
#                 otherwise dist/hub in this repo.

set -eu
cd "$(dirname "$0")"

CRATE=tpt-app-qr-batch
TARGET=wasm32-unknown-unknown
OUT_NAME=qr-batch

PRO=0
for arg in "$@"; do
  case "$arg" in
    --pro) PRO=1 ;;
    *) echo "usage: build.sh [--pro]" >&2; exit 2 ;;
  esac
done

if [ -n "${TPT_WEB_REPO:-}" ]; then
  DEST="$TPT_WEB_REPO/public/apps/qr-batch"
else
  DEST="dist/hub"
fi

echo "== TPT QR Batch build =="
echo " edition: $([ "$PRO" = 1 ] && echo 'pro (bulk CSV + branding/logo + print sheets)' || echo 'free (one code at a time)')"
echo " dest:    $DEST"

if [ "$PRO" = 1 ]; then
  cargo build --release --target "$TARGET" -p "$CRATE" --features pro
else
  cargo build --release --target "$TARGET" -p "$CRATE"
fi

WASM="target/$TARGET/release/tpt_app_qr_batch.wasm"
[ -f "$WASM" ] || { echo "missing $WASM" >&2; exit 1; }

if [ "$PRO" = 1 ]; then
  # The Pro wasm is embedded into the desktop exe; the hub bundle stays free.
  trunk build --release --features pro --dist dist
  cargo build --release -p tpt-qr-desktop
  echo "   exe: target/release/tpt-qr-batch-pro (embeds dist/)"
else
  mkdir -p "$DEST"
  wasm-bindgen --target web --out-name "$OUT_NAME" --out-dir "$DEST" "$WASM"

  GLUE="$DEST/$OUT_NAME.js"
  BG_WASM="$DEST/${OUT_NAME}_bg.wasm"
  [ -f "$GLUE" ] || { echo "wasm-bindgen did not produce $GLUE" >&2; exit 1; }
  [ -f "$BG_WASM" ] || { echo "wasm-bindgen did not produce $BG_WASM" >&2; exit 1; }

  RAW=$(wc -c < "$BG_WASM" | tr -d ' ')
  if command -v gzip >/dev/null 2>&1; then
    GZ=$(gzip -9 -c "$BG_WASM" | wc -c | tr -d ' ')
    echo "-- $(basename "$BG_WASM"): $RAW bytes raw, $GZ bytes gzipped"
    if [ "$GZ" -gt 409600 ]; then
      echo "warning: payload exceeds the ~400 KB gzip budget" >&2
    fi
  fi
fi

echo "-- done"
[ -n "${TPT_WEB_REPO:-}" ] || echo "   (set TPT_WEB_REPO to copy the bundle into the hub web repo)"
