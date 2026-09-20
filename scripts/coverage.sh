#!/bin/sh

set -eu

# rustup toolchains normally provide these tools next to rustc. Homebrew's
# rustc uses a separate LLVM installation, so discover that common layout too.
if [ -z "${LLVM_COV:-}" ] || [ -z "${LLVM_PROFDATA:-}" ]; then
    rustc_sysroot="$(rustc --print sysroot 2>/dev/null || true)"
    rustc_host="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
    rustup_llvm_dir="$rustc_sysroot/lib/rustlib/$rustc_host/bin"
    if [ -x "$rustup_llvm_dir/llvm-cov" ] && [ -x "$rustup_llvm_dir/llvm-profdata" ]; then
        LLVM_COV="${LLVM_COV:-$rustup_llvm_dir/llvm-cov}"
        LLVM_PROFDATA="${LLVM_PROFDATA:-$rustup_llvm_dir/llvm-profdata}"
    elif command -v brew >/dev/null 2>&1; then
        brew_llvm_prefix="$(brew --prefix llvm@22 2>/dev/null || true)"
        if [ -x "$brew_llvm_prefix/bin/llvm-cov" ] && [ -x "$brew_llvm_prefix/bin/llvm-profdata" ]; then
            LLVM_COV="${LLVM_COV:-$brew_llvm_prefix/bin/llvm-cov}"
            LLVM_PROFDATA="${LLVM_PROFDATA:-$brew_llvm_prefix/bin/llvm-profdata}"
        fi
    fi
fi

export LLVM_COV LLVM_PROFDATA
exec cargo llvm-cov --all-features --workspace --fail-under-lines 90 "$@"
