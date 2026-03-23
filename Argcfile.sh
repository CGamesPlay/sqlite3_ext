#!/usr/bin/env bash
# @describe sqlite3_ext development tasks
set -eu

# @cmd Regenerate the sqlite3ext bindings
bindgen() {
    if ! (command -v bindgen &>/dev/null); then
        cargo install bindgen
    fi

    command bindgen src/ffi/sqlite3ext.h \
        --allowlist-file src/ffi/sqlite3.h \
        --allowlist-file src/ffi/sqlite3ext.h \
        --generate types,vars \
        --default-macro-constant-type signed \
        --raw-line "#![allow(non_snake_case)]" \
        --raw-line "#![allow(dead_code)]" \
        --raw-line "#![allow(non_camel_case_types)]" \
        --raw-line "#![allow(clippy::type_complexity)]" \
        -o src/ffi/sqlite3types.rs
    echo "Generated src/ffi/sqlite3types.rs"

    command bindgen src/ffi/sqlite3ext.h \
        --allowlist-file src/ffi/sqlite3.h \
        --allowlist-file src/ffi/sqlite3ext.h \
        --generate functions,methods,constructors,destructors \
        --default-macro-constant-type signed \
        --raw-line "#![allow(non_snake_case)]" \
        --raw-line "#![allow(dead_code)]" \
        --raw-line "#![allow(non_camel_case_types)]" \
        --raw-line "#![allow(clippy::type_complexity)]" \
        --raw-line "use super::sqlite3types::*;" \
        -o src/ffi/sqlite3funcs.rs
    echo "Generated src/ffi/sqlite3funcs.rs"
}

# @cmd Test all supported configurations
test() {
    cargo test --workspace --all-features
    cargo test --workspace --features=static
}

if ! command -v argc >/dev/null; then
	echo "This command requires argc. Install from https://github.com/sigoden/argc" >&2
	exit 100
fi
eval "$(argc --argc-eval "$0" "$@")"
