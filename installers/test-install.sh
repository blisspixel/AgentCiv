#!/bin/sh
# Offline behavior tests. Production URLs are never configurable.
set -eu
repository=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/agentciv-installer-tests.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT HUP INT TERM
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
case "$url" in https://github.com/blisspixel/AgentCiv/releases/*) ;; *) exit 90 ;; esac
case "$url" in
    */latest/download/VERSION) cp "$AGENTCIV_FIXTURE/VERSION" "$output" ;;
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
printf '%s\n' v0.1.0 > "$scratch/releases/VERSION"
case_number=0
if [ -n "${AGENTCIV_COVERAGE_DIR:-}" ]; then mkdir -p "$AGENTCIV_COVERAGE_DIR"; fi
run() {
    case_number=$((case_number + 1))
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
printf '%s\n' 'Shell installer offline behavior tests passed.'
