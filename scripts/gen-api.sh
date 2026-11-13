#!/usr/bin/env bash
# Genera packages/api-client desde el contrato del servidor. Con --check falla si el contrato versionado está desactualizado.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=packages/api-client
python3 scripts/check-schema-names.py
