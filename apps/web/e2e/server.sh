#!/usr/bin/env bash
# Server real + web compilada en un solo origen (http://localhost:8099) con BD efímera para E2E.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
for d in /opt/homebrew/opt/postgresql@17/bin /usr/local/opt/postgresql@17/bin /usr/lib/postgresql/17/bin; do [ -x "$d/psql" ] && export PATH="$d:$PATH" && break; done
ADMIN="${E2E_ADMIN_DATABASE_URL:-postgres://postgres@127.0.0.1:5433/postgres}"
psql "$ADMIN" -qc "DROP DATABASE IF EXISTS vida_e2e WITH (FORCE)" -c "CREATE DATABASE vida_e2e OWNER vida" >/dev/null
export APP_ENV=development BIND_ADDR=127.0.0.1:8099 PUBLIC_BASE_URL=http://localhost:8099 COOKIE_SECURE=false
export DATABASE_URL="${E2E_DATABASE_URL:-postgres://vida:vida@127.0.0.1:5433/vida_e2e}"
export DEV_LOGIN_ENABLED=true DEMO_ENABLED=true ALLOWED_TELEGRAM_IDS=111,222 SCHEDULER_ENABLED=false
export TELEGRAM_BOT_TOKEN=123456:E2E-TOKEN TELEGRAM_BOT_USERNAME=vida_e2e_bot MINIAPP_MAX_AGE_SECS=86400
export INTERNAL_API_TOKEN=e2e-internal-token-0123456789abcdefghij ENCRYPTION_KEY=MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=
export WEB_DIST_DIR="$ROOT/apps/web/dist" RUST_LOG=warn
exec "$ROOT/target/debug/vida-server" serve
