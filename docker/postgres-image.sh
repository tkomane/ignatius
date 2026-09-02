#!/usr/bin/env bash
# Print the one reviewed image reference for a supported PostgreSQL major.
#
# The digest map is deliberately parsed as data instead of sourced, so changing
# a map value cannot inject shell syntax into a CI job.

set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: docker/postgres-image.sh <14|15|16|17|18>" >&2
  exit 2
fi

major="$1"
case "$major" in
  14|15|16|17|18) ;;
  *)
    echo "unsupported PostgreSQL fixture major: $major" >&2
    exit 2
    ;;
esac

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
map="$script_dir/postgres-images.env"
key="IGNATIUS_POSTGRES_IMAGE_${major}"
line="$(awk -F= -v key="$key" '$1 == key { matches += 1; value = $0 } END { if (matches != 1) exit 1; print value }' "$map")" || {
  echo "expected one fixture image entry for PostgreSQL $major" >&2
  exit 1
}
image="${line#*=}"
if [[ ! "$image" =~ ^docker\.io/library/postgres@sha256:[0-9a-f]{64}$ ]]; then
  echo "fixture image for PostgreSQL $major is not one immutable digest" >&2
  exit 1
fi

printf '%s\n' "$image"
