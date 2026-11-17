#!/usr/bin/env bash
# `pnpm verify`: checks locales reproducibles de Rust, frontend, contrato y pruebas.
# Ejecuta todos los pasos aunque alguno falle, muestra la tabla de resultados y sale con código ≠ 0 si algo falló.
# Variables: SKIP_E2E=1 omite Playwright; VERIFY_LOG_DIR (por defecto .data/verify).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
LOG_DIR="${VERIFY_LOG_DIR:-$ROOT/.data/verify}"
mkdir -p "$LOG_DIR"
if [ -z "${CI:-}" ]; then
  bash scripts/dev-db.sh start >/dev/null
  export DATABASE_URL="${TEST_DATABASE_URL:-postgres://dane:dane@127.0.0.1:5433/dane_test}"
fi
export DATABASE_URL="${DATABASE_URL:?DATABASE_URL de pruebas}"
export SQLX_OFFLINE=true

names=(); codes=(); secs=()
step() {
  local name="$1"; shift
  local log="$LOG_DIR/$(echo "$name" | tr ' /' '__').log"
  printf "▶ %-28s" "$name"
  local t0=$SECONDS
  "$@" >"$log" 2>&1
  local code=$?
  local dt=$((SECONDS - t0))
  names+=("$name"); codes+=("$code"); secs+=("$dt")
  if [ $code -eq 0 ]; then echo " ✓ (${dt}s)"; else echo " ✗ código $code (${dt}s) → $log"; tail -n 25 "$log" | sed 's/^/    │ /'; fi
}

[ -d node_modules ] || pnpm install --frozen-lockfile >/dev/null 2>&1

step "rust fmt"            cargo fmt --all -- --check
step "rust clippy"         cargo clippy --workspace --all-targets -- -D warnings
step "rust tests"          cargo test --workspace --no-fail-fast
step "contrato OpenAPI"    bash scripts/gen-api.sh --check
if [ -f apps/web/package.json ]; then
  step "web biome"         pnpm -C apps/web lint
  step "web typecheck"     pnpm -C apps/web typecheck
  step "web unit tests"    pnpm -C apps/web test
  step "web build"         pnpm -C apps/web build
  if [ "${SKIP_E2E:-0}" != "1" ] && grep -q '"e2e"' apps/web/package.json; then
    step "web e2e (playwright)" bash -c "cargo build -q -p server && pnpm -C apps/web e2e"
  fi
fi
step "backup/restore"      bash scripts/test-backup-restore.sh

echo
echo "| Paso | Resultado | Código | Duración |"
echo "|---|---|---|---|"
fail=0
for i in "${!names[@]}"; do
  r="✓ aprobado"; [ "${codes[$i]}" != "0" ] && { r="✗ fallido"; fail=1; }
  echo "| ${names[$i]} | $r | ${codes[$i]} | ${secs[$i]}s |"
done
echo
[ $fail -eq 0 ] && echo "VERIFY: OK" || echo "VERIFY: FALLÓ (logs en $LOG_DIR)"
exit $fail
