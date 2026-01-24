#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="$ROOT_DIR/dist"
ENGINE_JS="$OUTPUT_DIR/engine.js"
ENGINE_MJS="$OUTPUT_DIR/engine.mjs"

mkdir -p "$OUTPUT_DIR"

elm make "$ROOT_DIR/src/Main.elm" --output="$ENGINE_JS" "$@"

if ! grep -q "}(this));" "$ENGINE_JS"; then
  echo "Failed to locate Elm runtime wrapper in engine.js" >&2
  exit 1
fi

{
  echo "export function loadElm(scope) {"
  echo "  if (!scope) {"
  echo "    throw new Error('Elm scope is required');"
  echo "  }"
  sed -E 's/\}\(this\)\);[[:space:]]*$/}(scope));/' "$ENGINE_JS"
  echo "  return scope.Elm;"
  echo "}"
  echo "export default loadElm;"
} > "$ENGINE_MJS"
