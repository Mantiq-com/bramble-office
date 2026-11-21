#!/usr/bin/env bash
# Build WebAssembly (browser) bundles for Keydeck, Values, and/or Letters using Trunk.
#
# Usage:
#   ./tools/build-web.sh                       # build all three into dist/web/{keydeck,values,letters}
#   ./tools/build-web.sh keydeck values        # build selected app(s)
#   ./tools/build-web.sh --package             # build all three and create release archives in dist/
#   ./tools/build-web.sh letters --package     # build and package Letters only
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PACKAGE=0
APPS=()
for arg in "$@"; do
  case "$arg" in
    --package) PACKAGE=1 ;;
    keydeck|values|letters) APPS+=("$arg") ;;
    all) APPS=(keydeck values letters) ;;
    -h|--help)
      sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg (expected keydeck, values, letters, all, or --package)" >&2
      exit 2
      ;;
  esac
done

if [ ${#APPS[@]} -eq 0 ]; then
  APPS=(keydeck values letters)
fi

if ! command -v trunk >/dev/null 2>&1; then
  echo "error: trunk is required (install with: cargo install --locked trunk)" >&2
  exit 1
fi

build_app() {
  local app="$1"
  local crate_dir=""
  case "$app" in
    keydeck) crate_dir="keynote-linux/crates/kn-web" ;;
    values)  crate_dir="numbers-linux/crates/nm-web" ;;
    letters) crate_dir="pages-linux/crates/pg-web" ;;
  esac
  echo "==> Building $app ($crate_dir)"
  (cd "$ROOT/$crate_dir" && trunk build --release)
  rm -f "$ROOT/dist/web/$app/.trunk-lock"
  cp "$ROOT/LICENSE" "$ROOT/dist/web/$app/LICENSE"
}

zip_dir() {
  local src_dir="$1"
  local out_zip="$2"
  rm -f "$out_zip"
  if command -v zip >/dev/null 2>&1; then
    (cd "$src_dir" && zip -q -r "$out_zip" .)
  else
    python3 - "$src_dir" "$out_zip" <<'PY'
import os, sys, zipfile
src, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for root, _, files in os.walk(src):
        for f in sorted(files):
            full = os.path.join(root, f)
            zf.write(full, os.path.relpath(full, src))
PY
  fi
}

mkdir -p "$ROOT/dist/web"

for app in "${APPS[@]}"; do
  build_app "$app"
done

echo ""
echo "==> Built bundles:"
for app in "${APPS[@]}"; do
  wasm="$(find "$ROOT/dist/web/$app" -maxdepth 1 -name '*.wasm' | head -n 1 || true)"
  total="$(du -sh "$ROOT/dist/web/$app" | cut -f1)"
  if [ -n "$wasm" ]; then
    wsize="$(du -sh "$wasm" | cut -f1)"
    echo "  dist/web/$app  (wasm: $wsize, total with static assets: $total)"
  else
    echo "  dist/web/$app  (total: $total)"
  fi
done

if [ "$PACKAGE" -eq 1 ]; then
  echo ""
  echo "==> Packaging release archives in dist/"
  for app in "${APPS[@]}"; do
    case "$app" in
      keydeck) label="Keydeck" ;;
      values)  label="Values" ;;
      letters) label="Letters" ;;
    esac
    zip_dir "$ROOT/dist/web/$app" "$ROOT/dist/${label}-web.zip"
    echo "  dist/${label}-web.zip ($(du -sh "$ROOT/dist/${label}-web.zip" | cut -f1))"
  done
  if [ -d "$ROOT/dist/web/keydeck" ] && [ -d "$ROOT/dist/web/values" ] && [ -d "$ROOT/dist/web/letters" ]; then
    stage="$(mktemp -d)"
    trap 'rm -rf "$stage"' EXIT
    mkdir -p "$stage/bramble-office-web"
    cp -R "$ROOT/dist/web/keydeck" "$ROOT/dist/web/values" "$ROOT/dist/web/letters" "$stage/bramble-office-web/"
    cp "$ROOT/LICENSE" "$ROOT/README.md" "$stage/bramble-office-web/"
    tar -C "$stage" -czf "$ROOT/dist/Bramble-Office-web.tar.gz" bramble-office-web
    echo "  dist/Bramble-Office-web.tar.gz ($(du -sh "$ROOT/dist/Bramble-Office-web.tar.gz" | cut -f1))"
  fi
fi
