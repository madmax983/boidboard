#!/usr/bin/env bash
# Fail if the repository contains a construct that makes a test or a CI job incapable of
# failing. Each pattern below is a way to be green without being correct.
#
# Scans tracked files only, and excludes this script -- which necessarily contains every
# forbidden string as data.
set -euo pipefail

cd "$(dirname "$0")/.."
self="scripts/anti-theatre.sh"
status=0

report() { # <label> <files...>
  echo "FAIL: $1"
  shift
  printf '  %s\n' "$@"
  status=1
}

# --- Rust sources -----------------------------------------------------------------
# assert!(true) and assert_eq!(x, x) are assertions that cannot fail.
# #[ignore] silently removes a test from the suite while leaving it visible in the source.
# A crate-level #![allow(...)] disables a lint everywhere at once, which is how a
# -D warnings gate gets defeated without deleting it.
rust_files=$(git ls-files '*.rs' | grep -v "^${self}$" || true)
if [ -n "$rust_files" ]; then
  if hits=$(printf '%s\n' "$rust_files" | xargs grep -nE 'assert!\(\s*true\s*\)' 2>/dev/null); then
    report "assertion that cannot fail" "$hits"
  fi
  if hits=$(printf '%s\n' "$rust_files" | xargs grep -nE '#\[ignore' 2>/dev/null); then
    report "ignored test (this project runs every test it ships)" "$hits"
  fi
  if hits=$(printf '%s\n' "$rust_files" | xargs grep -nE '#!\[allow\(' 2>/dev/null); then
    report "crate-level lint suppression" "$hits"
  fi
fi

# --- Workflows --------------------------------------------------------------------
# continue-on-error turns a failing step green.
wf_files=$(git ls-files '.github/workflows/*.yml' '.github/workflows/*.yaml' || true)
if [ -n "$wf_files" ]; then
  if hits=$(printf '%s\n' "$wf_files" | xargs grep -n 'continue-on-error' 2>/dev/null); then
    report "step allowed to fail without failing the job" "$hits"
  fi
  # `cargo fmt` without --check rewrites files and exits 0 unconditionally.
  if hits=$(printf '%s\n' "$wf_files" | xargs grep -nE 'cargo fmt( --all)?\s*$' 2>/dev/null); then
    report "cargo fmt without --check can never fail" "$hits"
  fi
  # Dropping --all-targets makes clippy skip test code entirely.
  if printf '%s\n' "$wf_files" | xargs grep -q 'cargo clippy' 2>/dev/null; then
    if ! printf '%s\n' "$wf_files" | xargs grep -q 'cargo clippy --workspace --all-targets -- -D warnings' 2>/dev/null; then
      report "clippy invocation is not the full AC1 command" "expected: cargo clippy --workspace --all-targets -- -D warnings"
    fi
  fi
fi

if [ "$status" -eq 0 ]; then
  echo "anti-theatre: clean"
fi
exit "$status"
