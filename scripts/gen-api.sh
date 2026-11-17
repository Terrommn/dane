#!/usr/bin/env bash
# Genera packages/api-client desde el contrato del servidor. Con --check falla si el contrato versionado está desactualizado.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=packages/api-client
# utoipa fusiona en silencio schemas con el mismo nombre: se prohíben duplicados.
python3 scripts/check-schema-names.py
cargo run -q -p server --bin dane-server -- openapi > "$OUT/openapi.json.tmp"
mv "$OUT/openapi.json.tmp" "$OUT/openapi.json"
python3 - "$OUT/openapi.json" <<'PY'
import collections, json, sys
o = json.load(open(sys.argv[1]))
c = collections.Counter(op.get("operationId") for p in o["paths"].values() for op in p.values() if isinstance(op, dict))
d = [k for k, v in c.items() if v > 1]
if d:
    sys.exit(f"✗ operationId duplicados: {d}")
PY
pnpm -C "$OUT" exec openapi-typescript openapi.json -o src/schema.d.ts >/dev/null
if [ "${1:-}" = "--check" ]; then
  if ! git diff --quiet -- "$OUT/openapi.json" "$OUT/src/schema.d.ts"; then
    echo "✗ El contrato OpenAPI/cliente generado no coincide con el servidor. Ejecuta: pnpm gen:api" >&2
    git --no-pager diff --stat -- "$OUT"
    exit 1
  fi
  echo "✓ Contrato OpenAPI y cliente TypeScript al día"
else
  echo "✓ packages/api-client regenerado"
fi
