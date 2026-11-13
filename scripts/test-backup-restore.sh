#!/usr/bin/env bash
# Prueba T63: migra una BD temporal, inserta datos representativos, respalda, restaura en otra BD temporal y compara.
set -euo pipefail
cd "$(dirname "$0")/.."
for d in /opt/homebrew/opt/postgresql@17/bin /usr/local/opt/postgresql@17/bin /usr/lib/postgresql/17/bin; do [ -x "$d/pg_dump" ] && export PATH="$d:$PATH" && break; done
ADMIN="${ADMIN_DATABASE_URL:-postgres://postgres@127.0.0.1:5433/postgres}"
SRC=vida_bk_src_$$; DST=vida_bk_dst_$$
BASE="${ADMIN%/*}"; BASE_VIDA="postgres://vida:vida@${BASE#*@}"
cleanup() { psql "$ADMIN" -qc "DROP DATABASE IF EXISTS $SRC" -c "DROP DATABASE IF EXISTS $DST" >/dev/null 2>&1 || true; rm -f "/tmp/$SRC.dump"; }
trap cleanup EXIT
psql "$ADMIN" -qc "CREATE DATABASE $SRC OWNER vida" -c "CREATE DATABASE $DST OWNER vida"
DATABASE_URL="$BASE_VIDA/$SRC" APP_ENV=test INTERNAL_API_TOKEN=x-backup-test-token-0123456789abcdefgh \
  ENCRYPTION_KEY=MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY= COOKIE_SECURE=false ./target/debug/vida-server migrate >/dev/null
psql "$BASE_VIDA/$SRC" -q <<'SQL'
INSERT INTO users (id, display_name) VALUES ('00000000-0000-0000-0000-000000000001', 'Respaldo');
INSERT INTO user_settings (user_id) VALUES ('00000000-0000-0000-0000-000000000001');
INSERT INTO areas (user_id, name, slug) VALUES ('00000000-0000-0000-0000-000000000001', 'Escuela', 'escuela');
INSERT INTO notes (user_id, title, body_md) VALUES ('00000000-0000-0000-0000-000000000001', 'Canción', 'Ñandú y acentos');
INSERT INTO daily_checkins (user_id, on_date, weight_kg, energy) VALUES ('00000000-0000-0000-0000-000000000001', '2026-11-12', 71.40, 4);
SQL
DATABASE_URL="$BASE_VIDA/$SRC" bash scripts/backup.sh "/tmp/$SRC.dump" >/dev/null
TARGET_DATABASE_URL="$BASE_VIDA/$DST" bash scripts/restore.sh "/tmp/$SRC.dump" >/dev/null
q="SELECT (SELECT count(*) FROM users)||'/'||(SELECT count(*) FROM areas)||'/'||(SELECT body_md FROM notes)||'/'||(SELECT weight_kg FROM daily_checkins)||'/'||(SELECT count(*) FROM _sqlx_migrations)||'/'||(SELECT count(*) FROM search_index)"
a=$(psql "$BASE_VIDA/$SRC" -tAc "$q"); b=$(psql "$BASE_VIDA/$DST" -tAc "$q")
echo "origen:   $a"; echo "restaurada: $b"
[ "$a" = "$b" ] || { echo "✗ La restauración no coincide" >&2; exit 1; }
# Restaurar sobre una BD con datos sin --force debe negarse.
if TARGET_DATABASE_URL="$BASE_VIDA/$DST" bash scripts/restore.sh "/tmp/$SRC.dump" >/dev/null 2>&1; then echo "✗ restore debió negarse" >&2; exit 1; fi
echo "✓ backup/restore verificado en BD temporal"
