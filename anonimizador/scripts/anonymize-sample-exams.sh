#!/usr/bin/env bash
# Runs `anonimizador batch` over the owner's sample exams (local only, never in CI).
# The outputs and the index go to tests/sample-exams/anonymized/, which git ignores.
#
#   scripts/anonymize-sample-exams.sh --terms ~/private/terms.txt [batch options...]
set -euo pipefail
cd "$(dirname "$0")/.."
exams=../embeddings-fundamentals/tests/sample-exams
exec uv run anonimizador batch --out "$exams/anonymized" "$@" "$exams"
