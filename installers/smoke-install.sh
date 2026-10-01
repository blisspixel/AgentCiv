#!/bin/sh
set -eu
repository=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
[ "$#" -eq 2 ] || { printf '%s\n' 'Usage: smoke-install.sh RELEASE_DIRECTORY vX.Y.Z' >&2; exit 1; }
AGENTCIV_NATIVE_RELEASE=$(CDPATH='' cd "$1" && pwd)
AGENTCIV_NATIVE_VERSION=$2
export AGENTCIV_NATIVE_RELEASE AGENTCIV_NATIVE_VERSION
AGENTCIV_NATIVE_REPOSITORY=$repository
export AGENTCIV_NATIVE_REPOSITORY
scratch=$(mktemp -d "${TMPDIR:-/tmp}/agentciv-native-smoke.XXXXXXXX")
scratch=$(CDPATH='' cd "$scratch" && pwd -P)
smoke_stage=setup
cleanup() {
    smoke_status=$?
    trap - EXIT HUP INT TERM
    if [ "$smoke_status" -ne 0 ]; then printf 'Native POSIX installer smoke failed (exit %s): %s\n' "$smoke_status" "$smoke_stage" >&2; fi
    rm -rf -- "$scratch"
    exit "$smoke_status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$scratch/commands"
cat > "$scratch/commands/curl" <<'SCRIPT'
#!/bin/sh
set -eu
if [ "${2:-}" = --version ]; then printf '%s\n' 'curl 8.4.0'; exit 0; fi
output=
url=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) output=$2; shift 2 ;;
        --proto|--proto-redir|--connect-timeout|--max-time|--max-filesize) shift 2 ;;
        --*) shift ;;
        *) url=$1; shift ;;
    esac
done
case "$url" in "https://github.com/blisspixel/AgentCiv/releases/download/$AGENTCIV_NATIVE_VERSION/"*) ;; *) exit 91 ;; esac
name=${url##*/}
if [ "$name" = SHA256SUMS ]; then
    cat "$AGENTCIV_NATIVE_RELEASE"/SHA256SUMS-* > "$output"
    for resource in LICENSE THIRD_PARTY_NOTICES.txt NOTICE_INVENTORY.json; do
        if [ "$resource" = LICENSE ]; then source_path=$AGENTCIV_NATIVE_REPOSITORY/$resource
        else source_path=$AGENTCIV_NATIVE_REPOSITORY/release/$resource; fi
        if command -v sha256sum >/dev/null 2>&1; then digest=$(sha256sum "$source_path" | cut -d ' ' -f 1)
        else digest=$(shasum -a 256 "$source_path" | cut -d ' ' -f 1); fi
        printf '%s  %s\n' "$digest" "$resource" >> "$output"
    done
elif [ "$name" = LICENSE ]; then
    cp "$AGENTCIV_NATIVE_REPOSITORY/LICENSE" "$output"
elif [ "$name" = THIRD_PARTY_NOTICES.txt ] || [ "$name" = NOTICE_INVENTORY.json ]; then
    cp "$AGENTCIV_NATIVE_REPOSITORY/release/$name" "$output"
else
    cp "$AGENTCIV_NATIVE_RELEASE/$name" "$output"
fi
SCRIPT
chmod 755 "$scratch/commands/curl"
PATH=$scratch/commands:$PATH
export PATH
prefix=$scratch/installed\ with\ spaces
smoke_stage='initial install'
sh "$repository/install.sh" --prefix "$prefix" --version "$AGENTCIV_NATIVE_VERSION" --with-host-tools
smoke_stage='archive demo'
"$prefix/bin/agentciv-archive" demo > "$scratch/demo.json"
smoke_stage='archive demo validation'
grep -q source_authenticity "$scratch/demo.json"
for tool in agentciv-archive agentciv-reader agentciv-host agentciv-conformance; do
    smoke_stage="version check: $tool"
    [ "$("$prefix/bin/$tool" --version)" = "$tool ${AGENTCIV_NATIVE_VERSION#v}" ]
done
smoke_stage=reinstall
sh "$repository/install.sh" --prefix "$prefix" --version "$AGENTCIV_NATIVE_VERSION"
smoke_stage=uninstall
sh "$repository/install.sh" --prefix "$prefix" --uninstall
smoke_stage='uninstall directory check'
[ ! -e "$prefix" ]
printf '%s\n' 'Native POSIX installer fixture smoke passed.'
