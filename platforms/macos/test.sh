#!/bin/sh
set -eu

ROOT=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/tacit-macos-tests.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

swiftc \
    "$ROOT/platforms/macos/InputTextContext.swift" \
    "$ROOT/platforms/macos/tests/InputTextContextTests.swift" \
    -o "$WORK/input-text-context-tests"
"$WORK/input-text-context-tests"
