#!/bin/sh
# Offline behavior tests. Production URLs are never configurable.
set -eu
repository=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/agentciv-installer-tests.XXXXXXXX")
scratch=$(CDPATH='' cd "$scratch" && pwd -P)
cleanup() {
    fixture_status=$?
    trap - EXIT HUP INT TERM
    if [ "$fixture_status" -ne 0 ]; then
        printf 'Shell installer fixture failed (exit %s, case %s): %s\n' "$fixture_status" "${case_number:-0}" "${last_arguments:-setup}" >&2
        for stream in stdout stderr; do
            if [ -f "$scratch/$stream" ]; then
                printf '%s\n' "Last installer $stream:" >&2
                cat "$scratch/$stream" >&2 || :
            fi
        done
    fi
    rm -rf -- "$scratch"
    exit "$fixture_status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$scratch/commands" "$scratch/releases"
export AGENTCIV_FIXTURE="$scratch/releases"
export AGENTCIV_TEST_LOG="$scratch/downloads"
export AGENTCIV_TEST_OS=Linux
export AGENTCIV_TEST_CPU=x86_64
cat > "$scratch/commands/uname" <<'SCRIPT'
#!/bin/sh
case "$1" in -s) printf '%s\n' "$AGENTCIV_TEST_OS" ;; -m) printf '%s\n' "$AGENTCIV_TEST_CPU" ;; *) exit 1 ;; esac
SCRIPT
cat > "$scratch/commands/curl" <<'SCRIPT'
#!/bin/sh
set -eu
output=
url=
if [ "${2:-}" = --version ]; then printf '%s\n' 'curl 8.4.0'; exit 0; fi
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) output=$2; shift 2 ;;
        --proto|--proto-redir|--connect-timeout|--max-time|--max-filesize) shift 2 ;;
        --*) shift ;;
        *) url=$1; shift ;;
    esac
done
printf '%s\n' "$url" >> "$AGENTCIV_TEST_LOG"
case "$url" in https://github.com/blisspixel/AgentCiv/releases/*|https://api.github.com/repos/blisspixel/AgentCiv/releases\?per_page=100) ;; *) exit 90 ;; esac
case "$url" in
    */latest/download/VERSION) cp "$AGENTCIV_FIXTURE/VERSION" "$output" ;;
    https://api.github.com/*) printf '%s\n' '[{"tag_name":"v0.3.0-preview.1","draft":false,"prerelease":true}]' > "$output" ;;
    */download/*) relative=${url#https://github.com/blisspixel/AgentCiv/releases/download/}; cp "$AGENTCIV_FIXTURE/$relative" "$output" ;;
    *) exit 91 ;;
esac
SCRIPT
chmod 755 "$scratch/commands/uname" "$scratch/commands/curl"
PATH=$scratch/commands:$PATH
export PATH
digest() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d ' ' -f 1
    else shasum -a 256 "$1" | cut -d ' ' -f 1; fi
}
make_release() {
    release=$1
    mkdir "$scratch/releases/$release"
    : > "$scratch/releases/$release/SHA256SUMS"
    for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin; do
        for tool in agentciv-archive agentciv-reader agentciv-host agentciv-conformance; do
            asset=$tool-$release-$target
            sed -e "s/@COMPONENT@/$tool/g" -e "s/@VERSION@/${release#v}/g" > "$scratch/releases/$release/$asset" <<'BINARY'
#!/bin/sh
[ "$1" = --version ] || exit 1
printf '%s\n' '@COMPONENT@ @VERSION@'
BINARY
            printf '%s  %s\n' "$(digest "$scratch/releases/$release/$asset")" "$asset" >> "$scratch/releases/$release/SHA256SUMS"
        done
    done
    for resource in LICENSE THIRD_PARTY_NOTICES.txt NOTICE_INVENTORY.json; do
        printf '%s fixture\n' "$resource" > "$scratch/releases/$release/$resource"
        printf '%s  %s\n' "$(digest "$scratch/releases/$release/$resource")" "$resource" >> "$scratch/releases/$release/SHA256SUMS"
    done
}
make_release v0.1.0
make_release v0.2.0
make_release v0.3.0-preview.1
printf '%s\n' v0.1.0 > "$scratch/releases/VERSION"
case_number=0
if [ -n "${AGENTCIV_COVERAGE_DIR:-}" ]; then mkdir -p "$AGENTCIV_COVERAGE_DIR"; fi
run() {
    case_number=$((case_number + 1))
    last_arguments=$*
    if [ -n "${AGENTCIV_COVERAGE_DIR:-}" ]; then
        kcov --bash-parser="$(command -v bash)" --bash-dont-parse-binary-dir --include-path="$repository/install.sh" "$AGENTCIV_COVERAGE_DIR/case-$case_number" "$repository/install.sh" "$@" > "$scratch/stdout" 2> "$scratch/stderr"
    else
        sh "$repository/install.sh" "$@" > "$scratch/stdout" 2> "$scratch/stderr"
    fi
}
reject() { if run "$@"; then printf '%s\n' 'Expected installer failure' >&2; exit 1; fi; }
run --help
reject --unknown
reject --version
reject --prefix relative
mkdir "$scratch/traversal-parent"
printf 'unowned\n' > "$scratch/traversal-parent/keep"
reject --prefix "$scratch/traversal-parent/absent/.."
[ ! -e "$scratch/traversal-parent/absent" ] && [ ! -e "$scratch/traversal-parent/state" ]
newline='
'
reject --prefix "$scratch/new${newline}line"
reject --prefix "$scratch/multiline-version" --version "v0.1.0${newline}../../outside"
prefix=$scratch/installation\ with\ spaces
run --prefix "$prefix"
[ "$("$prefix/bin/agentciv-archive" --version)" = 'agentciv-archive 0.1.0' ]
[ "$(cat "$prefix/current")" = v0.1.0 ]
run --prefix "$prefix" --version 0.1.0
run --prefix "$prefix" --version v0.2.0 --with-host-tools
[ "$("$prefix/bin/agentciv-host" --version)" = 'agentciv-host 0.2.0' ]
run --prefix "$prefix" --rollback
[ "$("$prefix/bin/agentciv-archive" --version)" = 'agentciv-archive 0.1.0' ]
if "$prefix/bin/agentciv-host" --version > "$scratch/unavailable" 2>&1; then exit 1; fi
grep -q 'component unavailable' "$scratch/unavailable"
run --prefix "$prefix" --rollback
[ "$(cat "$prefix/current")" = v0.2.0 ]
run --prefix "$prefix" --version v0.2.0
[ "$("$prefix/bin/agentciv-host" --version)" = 'agentciv-host 0.2.0' ]
# Changed files abort uninstall before any deletion.
cp "$prefix/bin/agentciv-reader" "$scratch/shim"
printf '\n# changed\n' >> "$prefix/bin/agentciv-reader"
reject --prefix "$prefix" --uninstall
[ -f "$prefix/bin/agentciv-archive" ]
cp "$scratch/shim" "$prefix/bin/agentciv-reader"
printf 'private user data\n' > "$prefix/keep.txt"
printf 'other file\n' > "$prefix/versions/v0.1.0/keep.txt"
mkdir "$prefix/versions/unowned-empty"
run --prefix "$prefix" --uninstall
[ -d "$prefix/versions/unowned-empty" ]
[ -f "$prefix/keep.txt" ] && [ -f "$prefix/versions/v0.1.0/keep.txt" ]
[ ! -e "$prefix/bin/agentciv-archive" ]
reject --prefix "$prefix" --uninstall
mkdir "$scratch/foreign"
reject --prefix "$scratch/foreign"
ln -s "$scratch/foreign" "$scratch/link"
reject --prefix "$scratch/link"
AGENTCIV_TEST_CPU=unsupported
export AGENTCIV_TEST_CPU
reject --prefix "$scratch/unsupported"
AGENTCIV_TEST_CPU=x86_64
export AGENTCIV_TEST_CPU
# Broken releases cannot change an already working current pointer.
healthy=$scratch/healthy
run --prefix "$healthy" --version v0.1.0
# Release-equivalent bytes do not grant the installer ownership of existing files.
unowned=$scratch/unowned-generation
donor=$scratch/release-compatible-donor
run --prefix "$unowned" --version v0.1.0
run --prefix "$donor" --version v0.2.0
cp -R "$donor/versions/v0.2.0" "$unowned/versions/v0.2.0"
printf 'unowned user data\n' > "$unowned/versions/v0.2.0/sentinel.txt"
cp "$unowned/owned.sha256" "$scratch/unowned-original-manifest"
for path in "$unowned/versions/v0.2.0/"* "$unowned/current" "$unowned/previous" "$unowned/state" "$unowned/owned.sha256"; do
    printf '%s %s\n' "$(digest "$path")" "$path"
done > "$scratch/unowned-original-hashes"
reject --prefix "$unowned" --version v0.2.0
grep -q 'existing version contains unowned files' "$scratch/stderr"
while IFS=' ' read -r expected path; do [ "$(digest "$path")" = "$expected" ]; done < "$scratch/unowned-original-hashes"
[ "$(cat "$unowned/current")" = v0.1.0 ] && [ ! -e "$unowned/owned.new" ]
cp "$unowned/previous" "$scratch/unowned-original-previous"
printf '%s\n' v0.2.0 > "$unowned/previous"
rollback_previous_hash=$(digest "$unowned/previous")
reject --prefix "$unowned" --rollback
grep -q 'existing version contains unowned files' "$scratch/stderr"
[ "$(digest "$unowned/previous")" = "$rollback_previous_hash" ]
while IFS=' ' read -r expected path; do
    if [ "$path" != "$unowned/previous" ]; then [ "$(digest "$path")" = "$expected" ]; fi
done < "$scratch/unowned-original-hashes"
cp "$scratch/unowned-original-previous" "$unowned/previous"
for missing in components agentciv-archive NOTICE_INVENTORY.json; do
    cat "$scratch/unowned-original-manifest" > "$unowned/owned.sha256"
    awk -v missing="versions/v0.2.0/$missing" '$2 ~ /^versions\/v0\.2\.0\// && $2 != missing' "$donor/owned.sha256" >> "$unowned/owned.sha256"
    partial_hash=$(digest "$unowned/owned.sha256")
    reject --prefix "$unowned" --version v0.2.0
    grep -q 'existing version contains unowned files' "$scratch/stderr"
    [ "$(digest "$unowned/owned.sha256")" = "$partial_hash" ]
    while IFS=' ' read -r expected path; do
        if [ "$path" != "$unowned/owned.sha256" ]; then [ "$(digest "$path")" = "$expected" ]; fi
    done < "$scratch/unowned-original-hashes"
    printf '%s\n' v0.2.0 > "$unowned/previous"
    reject --prefix "$unowned" --rollback
    grep -q 'existing version contains unowned files' "$scratch/stderr"
    [ "$(digest "$unowned/owned.sha256")" = "$partial_hash" ] && [ "$(digest "$unowned/previous")" = "$rollback_previous_hash" ]
    while IFS=' ' read -r expected path; do
        if [ "$path" != "$unowned/owned.sha256" ] && [ "$path" != "$unowned/previous" ]; then [ "$(digest "$path")" = "$expected" ]; fi
    done < "$scratch/unowned-original-hashes"
    cp "$scratch/unowned-original-previous" "$unowned/previous"
done
cp "$scratch/unowned-original-manifest" "$unowned/owned.sha256"
run --prefix "$unowned" --uninstall
while IFS=' ' read -r expected path; do
    case "$path" in "$unowned/versions/v0.2.0/"*) [ "$(digest "$path")" = "$expected" ] ;; esac
done < "$scratch/unowned-original-hashes"
cp "$healthy/owned.sha256" "$scratch/original-ownership"
cat "$scratch/original-ownership" >> "$healthy/owned.sha256"
reject --prefix "$healthy" --uninstall
[ -f "$healthy/bin/agentciv-archive" ] && [ -f "$healthy/versions/v0.1.0/agentciv-reader" ]
[ "$(cat "$healthy/current")" = v0.1.0 ]
cp "$scratch/original-ownership" "$healthy/owned.sha256"
# A last ownership row without a newline must still be checked before mutation.
printf '%s' "$(cat "$scratch/original-ownership")" > "$healthy/owned.sha256"
last_owned=$(awk 'END { print $2 }' "$scratch/original-ownership")
cp "$healthy/$last_owned" "$scratch/last-owned"
printf 'changed owned content\n' >> "$healthy/$last_owned"
reject --prefix "$healthy" --uninstall
[ -f "$healthy/bin/agentciv-archive" ] && [ "$(cat "$healthy/current")" = v0.1.0 ]
cp "$scratch/last-owned" "$healthy/$last_owned"
run --prefix "$healthy" --version v0.1.0
for relative in owned.new previous.new current.new bin/.agentciv-archive.new; do
    printf 'unowned sentinel\n' > "$healthy/$relative"
    reject --prefix "$healthy" --version v0.2.0
    [ "$(cat "$healthy/$relative")" = 'unowned sentinel' ]
    [ "$(cat "$healthy/current")" = v0.1.0 ]
    rm -- "$healthy/$relative"
done
printf 'private sentinel\n' > "$scratch/symlink-sentinel"
ln -s "$scratch/symlink-sentinel" "$healthy/current.new"
reject --prefix "$healthy" --version v0.2.0
[ "$(cat "$scratch/symlink-sentinel")" = 'private sentinel' ]
rm -- "$healthy/current.new"
# Simulate a legitimately recorded older shim representation; rewriting it
# must update the ownership hash rather than leave a stale manifest.
printf '\n# prior installer representation\n' >> "$healthy/bin/agentciv-reader"
updated_digest=$(digest "$healthy/bin/agentciv-reader")
awk -v digest="$updated_digest" '$2 == "bin/agentciv-reader" { $1=digest } { print }' "$healthy/owned.sha256" > "$scratch/updated-ownership"
cp "$scratch/updated-ownership" "$healthy/owned.sha256"
run --prefix "$healthy" --version v0.1.0
run --prefix "$healthy" --version v0.1.0
mkdir "$healthy/.install-lock"
reject --prefix "$healthy" --version v0.2.0
[ "$(cat "$healthy/current")" = v0.1.0 ]
rmdir "$healthy/.install-lock"
asset=agentciv-reader-v0.2.0-x86_64-unknown-linux-gnu
cp "$scratch/releases/v0.2.0/$asset" "$scratch/original"
printf 'corruption\n' >> "$scratch/releases/v0.2.0/$asset"
reject --prefix "$healthy" --version v0.2.0
[ "$(cat "$healthy/current")" = v0.1.0 ]
cp "$scratch/original" "$scratch/releases/v0.2.0/$asset"
cp "$scratch/releases/v0.2.0/SHA256SUMS" "$scratch/sums"
grep " $asset$" "$scratch/sums" >> "$scratch/releases/v0.2.0/SHA256SUMS"
reject --prefix "$healthy" --version v0.2.0
cp "$scratch/sums" "$scratch/releases/v0.2.0/SHA256SUMS"
mv "$scratch/releases/v0.2.0/$asset" "$scratch/missing"
reject --prefix "$healthy" --version v0.2.0
mv "$scratch/missing" "$scratch/releases/v0.2.0/$asset"
reject --prefix "$healthy" --version '../../escape'
reject --prefix "$healthy" --version "v0.2.0${newline}../../outside"
[ "$(cat "$healthy/current")" = v0.1.0 ]
reject --prefix "$healthy" --rollback
run --prefix "$healthy" --uninstall
# Architecture detection downloads its exact target, never a fallback.
for pair in Linux:arm64 Darwin:x86_64 Darwin:arm64; do
    AGENTCIV_TEST_OS=${pair%:*}; AGENTCIV_TEST_CPU=${pair#*:}
    export AGENTCIV_TEST_OS AGENTCIV_TEST_CPU
    run --prefix "$scratch/architecture-$AGENTCIV_TEST_OS-$AGENTCIV_TEST_CPU" --version v0.1.0
done
# Published previews are selectable, and a source-only preview is diagnosed.
run --prefix "$scratch/preview" --version v0.3.0-preview.1
[ "$(cat "$scratch/preview/current")" = v0.3.0-preview.1 ]
run --prefix "$scratch/preview" --uninstall
[ ! -e "$scratch/preview" ]
reject --prefix "$scratch/source-only" --version v0.1.0-preview.3
grep -q 'native assets unavailable' "$scratch/stderr"
[ ! -e "$scratch/source-only" ]
# Explicit preview discovery never silently changes stable latest behavior.
cat > "$scratch/commands/jq" <<'SCRIPT'
#!/bin/sh
printf '%s\n' v0.3.0-preview.1
SCRIPT
chmod 755 "$scratch/commands/jq"
run --prefix "$scratch/preview-api" --pre
[ "$(cat "$scratch/preview-api/current")" = v0.3.0-preview.1 ]
grep -Fq 'https://api.github.com/repos/blisspixel/AgentCiv/releases?per_page=100' "$AGENTCIV_TEST_LOG"
run --prefix "$scratch/preview-api" --uninstall
reject --prefix "$scratch/conflict" --pre --from-source
reject --prefix "$scratch/conflict" --pre --version v0.1.0
reject --prefix "$scratch/conflict" --from-source --rollback
[ ! -e "$scratch/conflict" ]
# A legacy failed-install skeleton can be removed, retaining unknown files.
mkdir -p "$scratch/legacy/bin" "$scratch/legacy/versions"
printf '%s\n' agentciv-install/1 > "$scratch/legacy/state"
: > "$scratch/legacy/owned.sha256"
printf '%s\n' 'user data' > "$scratch/legacy/keep"
run --prefix "$scratch/legacy" --uninstall
[ "$(cat "$scratch/legacy/keep")" = 'user data' ]
[ ! -e "$scratch/legacy/state" ]
# Source builds are explicit, locked, commit-labelled and use the same ownership.
export AGENTCIV_SOURCE_COMMIT=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
export AGENTCIV_SOURCE_DIRTY=no
export AGENTCIV_SOURCE_FAIL=no
cat > "$scratch/commands/git" <<'SCRIPT'
#!/bin/sh
case "$3" in rev-parse) printf '%s\n' "$AGENTCIV_SOURCE_COMMIT" ;; diff) [ "$AGENTCIV_SOURCE_DIRTY" = no ] ;; *) exit 1 ;; esac
SCRIPT
cat > "$scratch/commands/cargo" <<'SCRIPT'
#!/bin/sh
set -eu
[ "$AGENTCIV_SOURCE_FAIL" = no ] || exit 1
[ "$1" = install ] && [ "$2" = --locked ] && [ "$3" = --path ] && [ "$5" = --root ]
case "$4" in */reference/host) tool=agentciv-host ;; */conformance/runner) tool=agentciv-conformance ;; *) tool=${4##*/} ;; esac
mkdir -p "$6/bin"
cp "$AGENTCIV_FIXTURE/v0.1.0/$tool-v0.1.0-x86_64-unknown-linux-gnu" "$6/bin/$tool"
chmod 755 "$6/bin/$tool"
SCRIPT
chmod 755 "$scratch/commands/git" "$scratch/commands/cargo"
run --prefix "$scratch/source" --from-source --with-host-tools
[ "$(cat "$scratch/source/current")" = source-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ]
[ "$("$scratch/source/bin/agentciv-conformance" --version)" = 'agentciv-conformance 0.1.0' ]
run --prefix "$scratch/source" --from-source
AGENTCIV_SOURCE_COMMIT=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
export AGENTCIV_SOURCE_COMMIT
run --prefix "$scratch/source" --from-source
run --prefix "$scratch/source" --rollback
[ "$(cat "$scratch/source/current")" = source-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ]
AGENTCIV_SOURCE_FAIL=yes
export AGENTCIV_SOURCE_FAIL
reject --prefix "$scratch/source" --from-source
[ "$(cat "$scratch/source/current")" = source-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ]
reject --prefix "$scratch/failed-source" --from-source
[ ! -e "$scratch/failed-source" ]
AGENTCIV_SOURCE_DIRTY=yes
export AGENTCIV_SOURCE_DIRTY
reject --prefix "$scratch/dirty-source" --from-source
grep -q 'commit tracked changes' "$scratch/stderr"
[ ! -e "$scratch/dirty-source" ]
run --prefix "$scratch/source" --uninstall
[ ! -e "$scratch/source" ]
printf '%s\n' 'Shell installer offline behavior tests passed.'
