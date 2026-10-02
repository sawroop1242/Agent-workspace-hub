#!/usr/bin/env bash
# End-to-end verification of scripts/install.sh against the local mock
# GitHub release server (serve.py). Runs the whole scenario matrix and
# asserts, per scenario: the exit code, the installed/leftover files,
# and the key transcript line — plus that the release API is queried
# EXACTLY once per install (the single-fetch contract).
#
# Usage: bash tests/installer-mock/run.sh
# Requires: bash, curl, python3, sha256sum (or shasum).
# Not run by cargo test (needs network-free localhost + python); use it
# directly, e.g. before tagging a release.
set -Euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${HERE}/../.." && pwd)"
INSTALL="${REPO_ROOT}/scripts/install.sh"
PORT="${AWH_INSTALLER_MOCK_PORT:-9419}"
API="http://127.0.0.1:${PORT}"
TMP=""
SRV_PID=""

cleanup() {
    if [ -n "$SRV_PID" ]; then kill "$SRV_PID" 2>/dev/null || true; fi
    if [ -n "$TMP" ]; then rm -rf "$TMP" || true; fi
}
trap cleanup EXIT

fail_case() {
    echo "FAIL: $1" >&2
    exit 1
}

relcount() {
    curl -fsS "${API}/__count" | sed -n 's/.*"release": \([0-9]*\).*/\1/p'
}

python3 "${HERE}/serve.py" "${PORT}" &
SRV_PID=$!
for _ in $(seq 1 50); do
    curl -fsS "${API}/__reset" >/dev/null 2>&1 && break
    sleep 0.1
done
curl -fsS "${API}/__reset" >/dev/null || fail_case "mock server did not start on ${API}"

TMP="$(mktemp -d)"

# The mock publishes only the linux/x86_64 asset. Pin the platform the
# installer detects on any runner by shadowing uname with a fake that
# answers exactly the two probes install.sh makes.
FAKEBIN="${TMP}/bin"
mkdir -p "$FAKEBIN"
cat >"${FAKEBIN}/uname" <<'EOF'
#!/bin/sh
case "$1" in
    -s) printf 'Linux\n' ;;
    -m) printf 'x86_64\n' ;;
    *) /usr/bin/uname "$@" ;;
esac
EOF
chmod +x "${FAKEBIN}/uname"

run_install() { # $1=version $2=prefix-dir -> sets RUN_OUT and RUN_RC
    # The if-form (rather than plain assignment + $?) is deliberate: it
    # keeps RUN_RC meaningful even if someone later adds `set -e` to this
    # script — a failing command in an if-condition never triggers errexit,
    # whereas a bare `RUN_OUT="$(...)"` would abort the driver before the
    # failure scenarios could assert on the exit code.
    if RUN_OUT="$(env -i PATH="${FAKEBIN}:/usr/bin:/bin" HOME="${HOME:-/tmp}" \
        AWH_GITHUB_API="$API" AWH_REPO="mock/awh" AWH_PREFIX="$2" AWH_VERSION="$1" \
        bash "$INSTALL" 2>&1)"; then
        RUN_RC=0
    else
        RUN_RC=$?
    fi
}

check() { # $1=label $2=zero|fail $3=expected_files $4=expected_grep(re|EMPTY) $5=version $6=expected_release_hits
    local label="$1" want_rc="$2" want_files="$3" want_grep="$4" version="$5" want_hits="$6"
    local d files before after
    d="${TMP}/${label}"
    mkdir -p "$d"
    before="$(relcount)"
    run_install "$version" "$d"
    files="$(ls "$d" 2>/dev/null | tr '\n' ' ')"
    after="$(relcount)"
    if [ "$want_rc" = "zero" ]; then
        [ "$RUN_RC" = "0" ] || fail_case "${label}: expected rc=0 got rc=${RUN_RC}; output: ${RUN_OUT}"
    else
        [ "$RUN_RC" != "0" ] || fail_case "${label}: expected nonzero rc, got 0; output: ${RUN_OUT}"
        # The security property on failure: unverified bytes may remain as a
        # `.part` file, but NOTHING installable is left under the real asset
        # name — no `awh` link a user could otherwise execute believing it
        # was verified. -e alone follows symlinks and misses a DANGLING one
        # (install.sh creates `awh` via ln -sf, so a failure between link
        # creation and cleanup would leave exactly that), hence the extra
        # -L. The other two names are regular files, so -e suffices.
        if [ -e "${d}/awh" ] || [ -L "${d}/awh" ]; then
            fail_case "${label}: failed install must not leave an installable 'awh' behind"
        fi
        [ ! -e "${d}/awh-linux-x86_64" ] || fail_case "${label}: failed install must not leave unverified bytes under the real asset name"
        [ ! -e "${d}/awh.exe" ] || fail_case "${label}: failed install must not leave 'awh.exe' behind"
    fi
    [ "$files" = "$want_files" ] || fail_case "${label}: files=[$files] want=[$want_files]"
    if [ "$want_grep" != "EMPTY" ]; then
        printf '%s\n' "$RUN_OUT" | grep -qE "$want_grep" ||
            fail_case "${label}: transcript lacks /${want_grep}/; got: ${RUN_OUT}"
    fi
    [ $((after - before)) = "$want_hits" ] ||
        fail_case "${label}: release-API hits delta=$((after - before)) want=${want_hits}"
    echo "ok: ${label}"
}

# label               rc    files                       transcript grep                                     version           hits
check happy-pinned     zero "awh awh-linux-x86_64 "     "Checksum verified for awh-linux-x86_64"          "v9.9.9"          1
check happy-latest     zero "awh awh-linux-x86_64 "     "Checksum verified for awh-linux-x86_64"          "latest"          1
check bsd-star-entry   zero "awh awh-linux-x86_64 "     "Checksum verified for awh-linux-x86_64"          "v9.9.9-star"     1
check checksum-mismatch fail "awh-linux-x86_64.part "   "Checksum mismatch for awh-linux-x86_64"           "v9.9.9-mismatch" 1
check entry-missing    fail "awh-linux-x86_64.part "    "does not contain an entry for awh-linux-x86_64"  "v9.9.9-missing"  1
check no-checksum-file fail "awh-linux-x86_64.part "   "does not publish sha256sums.txt"                "v9.9.9-nosums"    1
check asset-url-fallback zero "awh awh-linux-x86_64 "   "Checksum verified for awh-linux-x86_64"          "v9.9.9-noasset"  1
check pinned-tag-gone  fail ""                          "Release v9.9.9-badtag was not found"             "v9.9.9-badtag"   1

echo "ALL INSTALLER MOCK CHECKS PASSED"
