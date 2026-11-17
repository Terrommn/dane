#!/usr/bin/env bash
# Respaldo lógico de PostgreSQL (formato custom, comprimido). Uso: DATABASE_URL=... scripts/backup.sh [archivo]
# En producción con Compose: docker compose -f infra/compose.prod.yml exec -T db pg_dump -U dane -Fc dane > backup.dump
set -euo pipefail
: "${DATABASE_URL:?define DATABASE_URL}"
OUT="${1:-backups/dane-$(date -u +%Y%m%dT%H%M%SZ).dump}"
mkdir -p "$(dirname "$OUT")"
pg_dump --format=custom --no-owner --no-privileges --dbname="$DATABASE_URL" --file="$OUT"
echo "✓ Respaldo: $OUT ($(du -h "$OUT" | cut -f1))"
