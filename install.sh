#!/bin/sh
# Per-user native utility installer. Release assets are data until verified.
set -eu

fail() { printf '%s\n' "AgentCiv: $*" >&2; exit 1; }
usage() {
    printf '%s\n' 'Usage: sh install.sh [--version vX.Y.Z] [--prefix PATH] [--with-host-tools] [--rollback | --uninstall]' \
        'Default: archive and reader utilities. No admin, models, servers, or background updates.'
}
version=latest
prefix=${HOME:?HOME must be set}/.local/share/agentciv
components='agentciv-archive agentciv-reader'
operation=install
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version) [ "$#" -ge 2 ] || fail 'missing version'; version=$2; shift 2 ;;
        --prefix) [ "$#" -ge 2 ] || fail 'missing prefix'; prefix=$2; shift 2 ;;
        --with-host-tools) components='agentciv-archive agentciv-reader agentciv-host agentciv-conformance'; shift ;;
        --rollback|--uninstall) [ "$operation" = install ] || fail 'conflicting operations'; operation=${1#--}; shift ;;
        --help|-h) usage; exit 0 ;;
        *) fail 'unknown option (use --help)' ;;
    esac
done
case "$prefix" in /*) ;; *) fail 'prefix must be absolute' ;; esac
case "$prefix" in */./*|*/../*|*/.|*/..) fail 'prefix cannot contain dot path segments' ;; esac
newline='
'
case "$prefix" in /|"$HOME"|*"$newline"*) fail 'unsafe prefix' ;; esac
# Do not traverse existing symbolic links, including intermediate directories.
check_path() {
    check=$1
    while [ "$check" != / ]; do
        [ ! -L "$check" ] || fail 'installation path contains a symlink'
        check=$(dirname "$check")
    done
}
check_path "$prefix"
valid_version() {
    case "$1" in *"$newline"*|*"$(printf '\r')"*) return 1 ;; esac
    printf '%s\n' "$1" | LC_ALL=C grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'
}
if [ "$operation" = install ] && [ "$version" != latest ]; then
    case "$version" in v*) ;; *) version=v$version ;; esac
    valid_version "$version" || fail 'version must be vX.Y.Z'
fi
hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d ' ' -f 1
    else fail 'sha256sum or shasum required'; fi
}
safe_owned() {
    printf '%s\n' "$1" | LC_ALL=C grep -Eq '^(bin/agentciv-(archive|reader|host|conformance)|versions/v[0-9]+\.[0-9]+\.[0-9]+/(agentciv-(archive|reader|host|conformance)|components|LICENSE|THIRD_PARTY_NOTICES\.txt|NOTICE_INVENTORY\.json))$'
}
verify_owned() {
    [ -f "$prefix/owned.sha256" ] && [ ! -L "$prefix/owned.sha256" ] || fail 'missing ownership manifest'
    awk 'seen[$2]++ { exit 1 }' "$prefix/owned.sha256" || fail 'duplicate ownership path'
    while IFS=' ' read -r expected relative extra || [ -n "$expected$relative$extra" ]; do
        if [ -n "$extra" ] || ! safe_owned "$relative"; then fail 'invalid ownership manifest'; fi
        printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || fail 'invalid ownership hash'
        check_path "$prefix/$relative"
        [ -f "$prefix/$relative" ] || fail 'owned file missing'
        [ "$(hash_file "$prefix/$relative")" = "$expected" ] || fail 'owned file changed; preserve it and resolve manually'
    done < "$prefix/owned.sha256"
}
current=none
previous=none
if [ -e "$prefix" ]; then
    [ -d "$prefix" ] || fail 'prefix is not a directory'
    [ -f "$prefix/state" ] && [ ! -L "$prefix/state" ] || fail 'prefix exists without installer ownership'
    [ "$(cat "$prefix/state")" = agentciv-install/1 ] || fail 'invalid install state'
    if [ -f "$prefix/current" ]; then current=$(cat "$prefix/current"); fi
    if [ -f "$prefix/previous" ]; then previous=$(cat "$prefix/previous"); fi
    [ "$current" = none ] || valid_version "$current" || fail 'invalid current version'
    [ "$previous" = none ] || valid_version "$previous" || fail 'invalid previous version'
    verify_owned
fi
for name in state current previous owned.sha256 bin versions; do check_path "$prefix/$name"; done
if [ "$operation" = install ] && [ ! -e "$prefix" ]; then
    mkdir -p "$(dirname "$prefix")"
    mkdir "$prefix" || fail 'prefix created concurrently; retry'
    mkdir "$prefix/bin" "$prefix/versions"
    printf '%s\n' agentciv-install/1 > "$prefix/state"
    : > "$prefix/owned.sha256"
fi
if [ -d "$prefix" ] && [ -f "$prefix/state" ]; then
    mkdir "$prefix/.install-lock" || fail 'another installer or stale lock exists'
else
    fail 'nothing installed at prefix'
fi
temp=
cleanup() {
    if [ -n "$temp" ]; then rm -rf -- "$temp"; fi
    rmdir "$prefix/.install-lock"
    if [ "$operation" = uninstall ]; then rmdir "$prefix" 2>/dev/null || :; fi
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
locked_current=none
if [ -f "$prefix/current" ]; then locked_current=$(cat "$prefix/current"); fi
[ "$locked_current" = "$current" ] || fail 'installation changed concurrently; retry'
for relative in owned.new previous.new current.new bin/.agentciv-archive.new bin/.agentciv-reader.new bin/.agentciv-host.new bin/.agentciv-conformance.new; do
    [ ! -e "$prefix/$relative" ] && [ ! -L "$prefix/$relative" ] || fail 'staging path already exists; preserve it and resolve manually'
done
if [ "$operation" = install ]; then
    for component in $components; do
        if [ -e "$prefix/bin/$component" ] && ! grep -Fq " bin/$component" "$prefix/owned.sha256"; then fail 'unowned command collision'; fi
    done
fi
if [ "$operation" = uninstall ]; then
    [ "$current" != none ] || fail 'nothing installed at prefix'
    while IFS=' ' read -r expected relative extra || [ -n "$expected$relative$extra" ]; do rm -- "$prefix/$relative"; done < "$prefix/owned.sha256"
    # Remove only empty owned directories; unknown files and user data remain.
    owned_versions=$(awk '$2 ~ /^versions\// { split($2, parts, "/"); print parts[2] }' "$prefix/owned.sha256" | sort -u)
    for installed_version in $owned_versions; do rmdir "$prefix/versions/$installed_version" 2>/dev/null || :; done
    rm -f -- "$prefix/state" "$prefix/owned.sha256" "$prefix/current" "$prefix/previous"
    rmdir "$prefix/bin" "$prefix/versions" "$prefix" 2>/dev/null || :
    printf '%s\n' 'AgentCiv removed. Any unowned files were retained; shell profiles were never modified.'
    exit 0
fi
if [ "$operation" = rollback ]; then
    [ "$previous" != none ] || fail 'no previous version'
    version=$previous
    components=$(cat "$prefix/versions/$version/components")
else
    # Explicit updates retain optional tools already installed.
    if [ "$current" != none ] && grep -Fq agentciv-host "$prefix/versions/$current/components"; then
        components='agentciv-archive agentciv-reader agentciv-host agentciv-conformance'
    fi
    case "$(uname -s):$(uname -m)" in
        Linux:x86_64) target=x86_64-unknown-linux-gnu ;;
        Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-gnu ;;
        Darwin:x86_64) target=x86_64-apple-darwin ;;
        Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
        *) fail 'unsupported OS/architecture; no source-build fallback' ;;
    esac
    command -v curl >/dev/null 2>&1 || fail 'curl required'
    command -v grep >/dev/null 2>&1 || fail 'grep required'
    curl_version=$(curl --disable --version | awk 'NR == 1 { print $2 }')
    printf '%s\n' "$curl_version" | awk -F. 'NR == 1 { exit !($1 > 8 || ($1 == 8 && $2 >= 4)) }' || fail 'curl 8.4 or newer required for bounded downloads'
    # GitHub may redirect assets to its HTTPS release storage. Never permit HTTP.
    download() { curl --disable --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --connect-timeout 15 --max-time 180 --max-filesize "${3:-134217728}" --output "$2" "$1"; }
    temp=$(mktemp -d "${TMPDIR:-/tmp}/agentciv-install.XXXXXXXX") || fail 'temporary directory unavailable'
    base=https://github.com/blisspixel/AgentCiv/releases
    if [ "$version" = latest ]; then
        download "$base/latest/download/VERSION" "$temp/VERSION" 64 || fail 'no published release available'
        [ "$(wc -c < "$temp/VERSION")" -le 64 ] || fail 'invalid release version'
        version=$(cat "$temp/VERSION")
    fi
    case "$version" in v*) ;; *) version=v$version ;; esac
    valid_version "$version" || fail 'version must be vX.Y.Z'
    download "$base/download/$version/SHA256SUMS" "$temp/SHA256SUMS" 16384 || fail 'checksums unavailable'
    [ "$(wc -c < "$temp/SHA256SUMS")" -le 16384 ] || fail 'checksum manifest too large'
    for resource in LICENSE THIRD_PARTY_NOTICES.txt NOTICE_INVENTORY.json; do
        expected=$(awk -v name="$resource" '$2 == name { count++; digest=$1 } END { if (count != 1) exit 1; print digest }' "$temp/SHA256SUMS") || fail 'missing or duplicate notice checksum'
        printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || fail 'invalid notice checksum'
        download "$base/download/$version/$resource" "$temp/$resource" || fail 'license notice download failed'
        [ "$(hash_file "$temp/$resource")" = "$expected" ] || fail 'license notice checksum mismatch'
    done
    for component in $components; do
        asset=$component-$version-$target
        expected=$(awk -v name="$asset" '$2 == name { count++; digest=$1 } END { if (count != 1) exit 1; print digest }' "$temp/SHA256SUMS") || fail 'missing or duplicate checksum'
        printf '%s\n' "$expected" | grep -Eq '^[0-9a-f]{64}$' || fail 'missing or duplicate checksum'
        download "$base/download/$version/$asset" "$temp/$component" || fail 'binary download failed'
        [ "$(wc -c < "$temp/$component")" -le 134217728 ] || fail 'binary too large'
        [ "$(hash_file "$temp/$component")" = "$expected" ] || fail 'binary checksum mismatch'
        chmod 755 "$temp/$component"
        [ "$("$temp/$component" --version)" = "$component ${version#v}" ] || fail 'binary version or platform mismatch'
    done
    # A private staging directory lives on the destination volume for rename.
    destination=$prefix/versions/$version
    if [ -e "$destination" ]; then
        [ "$(cat "$destination/components")" = "$components" ] || fail 'version already installed with different components'
        for component in $components; do
            cmp -s "$temp/$component" "$destination/$component" || fail 'existing version differs from release'
        done
        for resource in LICENSE THIRD_PARTY_NOTICES.txt NOTICE_INVENTORY.json; do cmp -s "$temp/$resource" "$destination/$resource" || fail 'existing notices differ from release'; done
    else
        stage=$(mktemp -d "$prefix/versions/.stage.XXXXXXXX") || fail 'staging failed'
        for component in $components; do cp "$temp/$component" "$stage/$component"; done
        for resource in LICENSE THIRD_PARTY_NOTICES.txt NOTICE_INVENTORY.json; do cp "$temp/$resource" "$stage/$resource"; done
        printf '%s\n' "$components" > "$stage/components"
        mv "$stage" "$destination"
    fi
fi
# No content from a peer or archive is ever executed. Shims contain fixed names.
for component in $components; do
    case "$component" in agentciv-archive|agentciv-reader|agentciv-host|agentciv-conformance) ;; *) fail 'invalid components' ;; esac
done
new_owned=$prefix/owned.new
# Regenerate owned shims and their hashes, including retained optional shims.
# One pointer activates the complete verified generation.
awk '$2 !~ /^bin\// { print }' "$prefix/owned.sha256" > "$new_owned"
for component in agentciv-archive agentciv-reader agentciv-host agentciv-conformance; do
    case " $components " in *" $component "*) selected=yes ;; *) selected=no ;; esac
    if [ "$selected" = no ] && ! grep -Fq " bin/$component" "$prefix/owned.sha256"; then continue; fi
    if [ -e "$prefix/bin/$component" ] && ! grep -Fq " bin/$component" "$prefix/owned.sha256"; then fail 'unowned command collision'; fi
    shim=$prefix/bin/.$component.new
    sed "s/@COMPONENT@/$component/g" > "$shim" <<'SHIM'
#!/bin/sh
root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd) || exit 1
IFS= read -r version < "$root/current" || exit 1
if [ ! -f "$root/versions/$version/@COMPONENT@" ]; then printf '%s\n' 'AgentCiv: component unavailable in selected version' >&2; exit 1; fi
exec "$root/versions/$version/@COMPONENT@" "$@"
SHIM
    chmod 755 "$shim"
    mv "$shim" "$prefix/bin/$component"
    relative=bin/$component
    printf '%s %s\n' "$(hash_file "$prefix/$relative")" "$relative" >> "$new_owned"
    if [ "$selected" = yes ]; then
        relative=versions/$version/$component
        if ! grep -Fq " $relative" "$new_owned"; then printf '%s %s\n' "$(hash_file "$prefix/$relative")" "$relative" >> "$new_owned"; fi
    fi
done
relative=versions/$version/components
if ! grep -Fq " $relative" "$new_owned"; then printf '%s %s\n' "$(hash_file "$prefix/$relative")" "$relative" >> "$new_owned"; fi
for resource in LICENSE THIRD_PARTY_NOTICES.txt NOTICE_INVENTORY.json; do
    relative=versions/$version/$resource
    if ! grep -Fq " $relative" "$new_owned"; then printf '%s %s\n' "$(hash_file "$prefix/$relative")" "$relative" >> "$new_owned"; fi
done
mv "$new_owned" "$prefix/owned.sha256"
if [ "$version" != "$current" ]; then previous=$current; fi
printf '%s\n' "$previous" > "$prefix/previous.new"
mv "$prefix/previous.new" "$prefix/previous"
printf '%s\n' "$version" > "$prefix/current.new"
mv "$prefix/current.new" "$prefix/current"
printf '%s\n' "Installed AgentCiv $version in $prefix" 'No shell profiles were modified. Add this directory to PATH:' "$prefix/bin" \
    'First run (offline):' "$prefix/bin/agentciv-archive demo"
