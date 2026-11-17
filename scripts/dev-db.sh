#!/usr/bin/env bash
# PostgreSQL local para desarrollo SIN Docker (D-004). Cluster en .data/pg, puerto 5433.
# Uso: scripts/dev-db.sh start|stop|status|reset|psql
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$ROOT/.data/pg"
PORT="${DEV_PG_PORT:-5433}"
for d in /opt/homebrew/opt/postgresql@17/bin /usr/local/opt/postgresql@17/bin /usr/lib/postgresql/17/bin; do
  [ -x "$d/pg_ctl" ] && PGBIN="$d" && break
done
: "${PGBIN:?No encontré PostgreSQL 17 (brew install postgresql@17 o apt install postgresql-17)}"
export PATH="$PGBIN:$PATH" LC_ALL="${LC_ALL:-en_US.UTF-8}"

init() {
  mkdir -p "$ROOT/.data"
  initdb -D "$DATA" -U postgres --auth=trust --encoding=UTF8 >/dev/null
  cat >> "$DATA/postgresql.conf" <<CONF
port = $PORT
listen_addresses = '127.0.0.1'
unix_socket_directories = '$DATA'
max_connections = 200
timezone = 'UTC'
CONF
}
start() {
  [ -d "$DATA" ] || init
  if ! pg_ctl -D "$DATA" status >/dev/null 2>&1; then
    pg_ctl -D "$DATA" -l "$ROOT/.data/pg.log" -w start >/dev/null
  fi
  psql -h 127.0.0.1 -p "$PORT" -U postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='dane'" | grep -q 1 \
    || psql -h 127.0.0.1 -p "$PORT" -U postgres -qc "CREATE ROLE dane LOGIN CREATEDB PASSWORD 'dane'"
  for db in dane dane_test; do
    psql -h 127.0.0.1 -p "$PORT" -U postgres -tAc "SELECT 1 FROM pg_database WHERE datname='$db'" | grep -q 1 \
      || psql -h 127.0.0.1 -p "$PORT" -U postgres -qc "CREATE DATABASE $db OWNER dane"
    # Extensiones contrib requieren superusuario: se crean aquí para que las migraciones solo verifiquen.
    psql -h 127.0.0.1 -p "$PORT" -U postgres -d "$db" -qc "CREATE EXTENSION IF NOT EXISTS unaccent; CREATE EXTENSION IF NOT EXISTS pg_trgm;"
  done
  # template1 con extensiones para que las BD de #[sqlx::test] las hereden.
  psql -h 127.0.0.1 -p "$PORT" -U postgres -d template1 -qc "CREATE EXTENSION IF NOT EXISTS unaccent; CREATE EXTENSION IF NOT EXISTS pg_trgm;"
  echo "PostgreSQL listo: postgres://dane:dane@127.0.0.1:$PORT/dane"
}
case "${1:-start}" in
  start) start ;;
  stop) pg_ctl -D "$DATA" -m fast stop ;;
  status) pg_ctl -D "$DATA" status ;;
  reset) pg_ctl -D "$DATA" -m fast stop 2>/dev/null || true; rm -rf "$DATA"; start ;;
  psql) shift; psql -h 127.0.0.1 -p "$PORT" -U dane -d dane "$@" ;;
  *) echo "uso: $0 start|stop|status|reset|psql" >&2; exit 2 ;;
esac
