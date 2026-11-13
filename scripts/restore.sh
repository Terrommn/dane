#!/usr/bin/env bash
# Restaura un respaldo en la BD indicada. Se niega si la BD destino ya tiene tablas (usa --force para reemplazar).
# Uso: TARGET_DATABASE_URL=... scripts/restore.sh backup.dump [--force]
set -euo pipefail
: "${TARGET_DATABASE_URL:?define TARGET_DATABASE_URL}"
FILE="${1:?archivo de respaldo}"
FORCE="${2:-}"
tables=$(psql "$TARGET_DATABASE_URL" -tAc "SELECT count(*) FROM pg_tables WHERE schemaname='public'")
if [ "$tables" != "0" ] && [ "$FORCE" != "--force" ]; then
  echo "✗ La BD destino tiene $tables tablas. Usa --force para reemplazar (destructivo)." >&2
  exit 1
fi
# Las extensiones (unaccent, pg_trgm) pertenecen al superusuario y ya existen en el destino (postgres-init.sql):
# se excluyen del listado para no intentar borrarlas/recrearlas con el rol de la app.
LIST="$(mktemp)"; trap 'rm -f "$LIST"' EXIT
pg_restore --list "$FILE" | grep -v ' EXTENSION ' > "$LIST"
CLEAN=(); [ "$tables" != "0" ] && CLEAN=(--clean --if-exists)
pg_restore "${CLEAN[@]}" --use-list="$LIST" --no-owner --no-privileges --exit-on-error --dbname="$TARGET_DATABASE_URL" "$FILE"
echo "✓ Restaurado en la BD destino"
