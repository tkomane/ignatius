#!/bin/sh
# Validate the source-controlled release catalogue and its changelog links.
# This command is read-only: it does not build, publish, sign or upload.

set -eu

usage() {
    cat <<'EOF'
Usage: release-notes/validate.sh [--catalog PATH] [--changelog PATH]

Validate one release catalogue with the repository semantic validator, then
verify that every notes_reference.entry appears as exactly one `## <entry>`
heading in the changelog and that catalogue entries do not reuse one exact
heading.
The default paths are release-notes/catalog.json and CHANGELOG.md in the
repository containing this script. No files are written.

Examples:
  release-notes/validate.sh
  release-notes/validate.sh --catalog release-notes/catalog.json \
    --changelog CHANGELOG.md
EOF
}

fail() {
    echo "release notes: $*" >&2
    exit 1
}

absolute_path() {
    case "$1" in
        /*)
            printf '%s\n' "$1"
            ;;
        *)
            path_directory=$(dirname -- "$1")
            path_name=$(basename -- "$1")
            (
                cd -- "$path_directory"
                printf '%s/%s\n' "$(pwd -P)" "$path_name"
            )
            ;;
    esac
}

reject_symlinked_parent() {
    input_path=$1
    label=$2
    case "$input_path" in
        /*)
            input_current=/
            input_remaining=${input_path#/}
            ;;
        *)
            input_current=$(pwd -P) || fail "could not resolve current directory"
            input_remaining=$input_path
            ;;
    esac

    while [ -n "$input_remaining" ]; do
        case "$input_remaining" in
            */*)
                input_component=${input_remaining%%/*}
                input_remaining=${input_remaining#*/}
                input_has_child=1
                ;;
            *)
                input_component=$input_remaining
                input_remaining=
                input_has_child=0
                ;;
        esac
        case "$input_component" in
            ""|.)
                continue
                ;;
            ..)
                input_current=$(dirname "$input_current")
                ;;
            *)
                if [ "$input_current" = "/" ]; then
                    input_current=/$input_component
                else
                    input_current=$input_current/$input_component
                fi
                if [ "$input_has_child" -eq 1 ]; then
                    [ ! -L "$input_current" ] || fail "$label parent must not be a symlink: $input_current"
                fi
                ;;
        esac
    done
}

catalog_path=
changelog_path=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --catalog)
            [ "$#" -ge 2 ] || fail "--catalog needs a path"
            catalog_path=$2
            shift 2
            ;;
        --changelog)
            [ "$#" -ge 2 ] || fail "--changelog needs a path"
            changelog_path=$2
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage >&2
            fail "unknown argument: $1"
            ;;
    esac
done

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
catalog_path=${catalog_path:-"$repo_root/release-notes/catalog.json"}
changelog_path=${changelog_path:-"$repo_root/CHANGELOG.md"}
reject_symlinked_parent "$catalog_path" "catalogue"
reject_symlinked_parent "$changelog_path" "changelog"
catalog_path=$(absolute_path "$catalog_path") || fail "could not resolve catalogue path"
changelog_path=$(absolute_path "$changelog_path") || fail "could not resolve changelog path"

[ -f "$catalog_path" ] || fail "catalogue is not a regular file: $catalog_path"
[ -f "$changelog_path" ] || fail "changelog is not a regular file: $changelog_path"
[ ! -L "$catalog_path" ] || fail "catalogue must not be a symlink: $catalog_path"
[ ! -L "$changelog_path" ] || fail "changelog must not be a symlink: $changelog_path"
command -v python3 >/dev/null 2>&1 || fail "python3 is required for changelog validation"

if [ -n "${IGNATIUS_XTASK_BIN:-}" ]; then
    case "$IGNATIUS_XTASK_BIN" in
        /*) ;;
        *) fail "IGNATIUS_XTASK_BIN must be an absolute path" ;;
    esac
    [ -x "$IGNATIUS_XTASK_BIN" ] || fail "xtask helper is not executable: $IGNATIUS_XTASK_BIN"
    "$IGNATIUS_XTASK_BIN" release validate "$catalog_path"
else
    command -v cargo >/dev/null 2>&1 || fail "cargo is required for semantic validation"
    (
        cd "$repo_root"
        cargo --locked xtask release validate "$catalog_path"
    )
fi

python3 - "$catalog_path" "$changelog_path" <<'PY'
import json
import pathlib
import sys

catalog_path = pathlib.Path(sys.argv[1])
changelog_path = pathlib.Path(sys.argv[2])

try:
    catalogue = json.loads(catalog_path.read_text(encoding="utf-8"))
    changelog = changelog_path.read_text(encoding="utf-8")
except (OSError, UnicodeError, json.JSONDecodeError) as error:
    print(f"release notes: could not read validation input: {error}", file=sys.stderr)
    raise SystemExit(1)

records = catalogue.get("records")
if not isinstance(records, list) or not records:
    print("release notes: catalogue records must be a non-empty array", file=sys.stderr)
    raise SystemExit(1)

errors = []
entry_indexes = {}

def is_version_token_character(character):
    return character.isascii() and (character.isalnum() or character in ".-+")


def contains_exact_version_token(value, version):
    if not version:
        return False
    search_from = 0
    while True:
        start = value.find(version, search_from)
        if start < 0:
            return False
        end = start + len(version)
        left_is_boundary = start == 0 or not is_version_token_character(value[start - 1])
        right_is_boundary = end == len(value) or not is_version_token_character(value[end])
        if left_is_boundary and right_is_boundary:
            return True
        search_from = end


for index, record in enumerate(records):
    location = f"records[{index}]"
    notes = record.get("notes_reference") if isinstance(record, dict) else None
    version = record.get("product_version") if isinstance(record, dict) else None
    if not isinstance(notes, dict):
        errors.append(f"{location}.notes_reference must be an object")
        continue
    if notes.get("path") != "CHANGELOG.md":
        errors.append(f"{location}.notes_reference.path must be CHANGELOG.md")
    entry = notes.get("entry")
    if not isinstance(entry, str) or not entry:
        errors.append(f"{location}.notes_reference.entry must be non-empty text")
        continue
    if "\n" in entry or "\r" in entry:
        errors.append(f"{location}.notes_reference.entry must be one line")
        continue
    previous_index = entry_indexes.setdefault(entry, index)
    if previous_index != index:
        errors.append(
            f"{location}.notes_reference.entry duplicates records[{previous_index}]"
        )
    if not isinstance(version, str) or not contains_exact_version_token(entry, version):
        errors.append(f"{location}.notes_reference.entry must name its product version")
    heading = f"## {entry}"
    occurrences = sum(line == heading for line in changelog.splitlines())
    if occurrences != 1:
        errors.append(
            f"{location}.notes_reference.entry heading occurs {occurrences} times in "
            f"{changelog_path}, expected exactly once"
        )

if errors:
    for error in errors:
        print(f"release notes: {error}", file=sys.stderr)
    raise SystemExit(1)

print(f"Release notes: verified {len(records)} record(s) against {changelog_path}")
PY
