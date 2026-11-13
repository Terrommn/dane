#!/usr/bin/env bash
# Arranque local completo: PostgreSQL (nativo o Docker), server, bot (si hay token) y web con recarga.
# Uso: pnpm dev        (Ctrl+C detiene todo)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[ -f .env ] || { cp .env.example .env; echo "Creado .env desde .env.example (revísalo)"; }
set -a; source .env; set +a

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1 && [ "${USE_DOCKER_DB:-0}" = "1" ]; then
  docker compose -f infra/compose.dev.yml up -d db
else
  bash scripts/dev-db.sh start
fi

[ -d node_modules ] || pnpm install
pids=()
cleanup() { echo; echo "Deteniendo…"; for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; }
trap cleanup EXIT INT TERM

cargo build -p server -p telegram-bot
./target/debug/vida-server serve & pids+=($!)
for _ in $(seq 1 60); do curl -fsS "http://${BIND_ADDR}/readyz" >/dev/null 2>&1 && break; sleep 1; done
echo "✓ API en http://${BIND_ADDR} (OpenAPI: /api/v1/openapi.json)"

if [ -n "${TELEGRAM_BOT_TOKEN:-}" ]; then
  env -u DATABASE_URL -u TEST_DATABASE_URL ./target/debug/vida-bot & pids+=($!)
  echo "✓ Bot en modo ${TELEGRAM_MODE:-polling}"
else
  echo "• Bot omitido: define TELEGRAM_BOT_TOKEN en .env para probarlo"
fi

pnpm -C apps/web dev --host 127.0.0.1 & pids+=($!)
echo "✓ Web en ${PUBLIC_BASE_URL}"
wait
