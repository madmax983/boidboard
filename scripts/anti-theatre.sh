#!/usr/bin/env bash
# Fail if the repository contains a construct that makes a test or a CI job incapable of
# failing. Each pattern below is a way to be green without being correct.
#
# Scans tracked files only, and excludes this script -- which necessarily contains every
# forbidden string as data.
#
# Matching is WHOLE-FILE, not line-based (`grep -Pz`). That is not a refinement: rustfmt
# breaks a long `assert_eq!` across lines automatically, so a line-based scanner does not
# see the shape it was written to catch. Run `scripts/anti-theatre.sh --selftest` to see
# every rule proved against a probe.
set -euo pipefail

cd "$(dirname "$0")/.."
self="scripts/anti-theatre.sh"
status=0

report() { # <label> <detail...>
  echo "FAIL: $1"
  shift
  printf '  %s\n' "$@"
  status=1
}

# Print "<file>: <first matching line>" for every file matching a whole-file PCRE.
# Whole-file so that a construct split across lines by rustfmt is still seen.
scan() { # <pcre> <files...>
  local pcre="$1"
  shift
  local f hit
  for f in "$@"; do
    if hit=$(grep -Pzo "$pcre" "$f" 2>/dev/null | tr '\0\n' '  ') && [ -n "$hit" ]; then
      printf '%s: %s\n' "$f" "$(echo "$hit" | tr -s ' ')"
    fi
  done
}

# --- the patterns, named once so the self-test proves the same strings the scan uses ----

# assert!(true) is an assertion that cannot fail.
PAT_ASSERT_TRUE='assert!\(\s*true\s*[,)]'
# assert_eq!(x, x) compares a value with itself. The trailing (?:\(\))? matters: the
# tempting form in this repository is assert_eq!(board.key(), board.key()), and a pattern
# without it reads the parentheses as a mismatch and waves the assertion through.
PAT_SELF_COMPARE='assert_(?:eq|ne)!\(\s*([A-Za-z_][A-Za-z0-9_.]*(?:\(\))?)\s*,\s*\1\s*[,)]'
# #[ignore] silently removes a test from the suite while leaving it visible in the source.
# cfg_attr(..., ignore) does the same without the literal attribute. The second alternative
# is `.*` and not `[^)]*` deliberately: the realistic evasion is #[cfg_attr(all(), ignore)],
# whose first ')' belongs to all() rather than to cfg_attr, and a negated-class pattern
# stops there and waves it through. The self-test below caught exactly that.
PAT_IGNORED='#\[ignore|cfg_attr\(.*ignore'
# A crate-level #![allow(...)] disables a lint everywhere at once, which is how a
# -D warnings gate gets defeated without deleting it.
PAT_CRATE_ALLOW='#!\[allow\('

# --- self-test ---------------------------------------------------------------------
# A scanner nobody has seen fail is indistinguishable from a scanner that matches nothing.
# Each probe below is a construct the corresponding rule exists to catch; the mode fails
# unless EVERY rule fires on its own probe and NO rule fires on the clean control.
if [ "${1:-}" = "--selftest" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  fails=0

  printf 'fn t() { assert!(true); }\n'                                  > "$tmp/assert_true.rs"
  printf 'fn t() { assert_eq!(a, a); }\n'                               > "$tmp/self_eq.rs"
  printf 'fn t() { assert_eq!(b.key(), b.key()); }\n'                   > "$tmp/self_eq_call.rs"
  printf 'fn t() {\n    assert_eq!(\n        b.key(),\n        b.key()\n    );\n}\n' \
                                                                        > "$tmp/self_eq_multiline.rs"
  printf 'fn t() { assert_ne!(x, x); }\n'                               > "$tmp/self_ne.rs"
  printf '#[ignore]\nfn t() {}\n'                                       > "$tmp/ignore.rs"
  printf '#[cfg_attr(all(), ignore)]\nfn t() {}\n'                      > "$tmp/cfg_ignore.rs"
  printf '#![allow(dead_code)]\n'                                       > "$tmp/crate_allow.rs"
  printf 'fn t() { assert_eq!(got, want); assert!(ok); }\n'             > "$tmp/clean.rs"

  check() { # <label> <pattern> <probe> <expect: fire|silent>
    local got
    got=$(scan "$2" "$tmp/$3")
    if [ "$4" = fire ] && [ -z "$got" ]; then
      echo "SELFTEST FAIL: rule '$1' did not fire on $3"
      fails=1
    elif [ "$4" = silent ] && [ -n "$got" ]; then
      echo "SELFTEST FAIL: rule '$1' fired on the clean control: $got"
      fails=1
    else
      echo "ok: $1 vs $3 ($4)"
    fi
  }

  check "assertion that cannot fail"   "$PAT_ASSERT_TRUE"  assert_true.rs        fire
  check "self-comparison"              "$PAT_SELF_COMPARE" self_eq.rs            fire
  check "self-comparison (method)"     "$PAT_SELF_COMPARE" self_eq_call.rs       fire
  check "self-comparison (multiline)"  "$PAT_SELF_COMPARE" self_eq_multiline.rs  fire
  check "self-comparison (assert_ne)"  "$PAT_SELF_COMPARE" self_ne.rs            fire
  check "ignored test"                 "$PAT_IGNORED"      ignore.rs             fire
  check "ignored test (cfg_attr)"      "$PAT_IGNORED"      cfg_ignore.rs         fire
  check "crate-level lint suppression" "$PAT_CRATE_ALLOW"  crate_allow.rs        fire
  check "assertion that cannot fail"   "$PAT_ASSERT_TRUE"  clean.rs              silent
  check "self-comparison"              "$PAT_SELF_COMPARE" clean.rs              silent
  check "ignored test"                 "$PAT_IGNORED"      clean.rs              silent
  check "crate-level lint suppression" "$PAT_CRATE_ALLOW"  clean.rs              silent

  [ "$fails" -eq 0 ] && echo "anti-theatre selftest: every rule fires on its probe and none on the control"
  exit "$fails"
fi

# --- Rust sources -----------------------------------------------------------------
mapfile -t rust_files < <(git ls-files '*.rs' | grep -v "^${self}$" || true)
if [ "${#rust_files[@]}" -gt 0 ]; then
  if hits=$(scan "$PAT_ASSERT_TRUE" "${rust_files[@]}") && [ -n "$hits" ]; then
    report "assertion that cannot fail" "$hits"
  fi
  if hits=$(scan "$PAT_SELF_COMPARE" "${rust_files[@]}") && [ -n "$hits" ]; then
    report "assertion comparing a value with itself" "$hits"
  fi
  if hits=$(scan "$PAT_IGNORED" "${rust_files[@]}") && [ -n "$hits" ]; then
    report "ignored test (this project runs every test it ships)" "$hits"
  fi
  if hits=$(scan "$PAT_CRATE_ALLOW" "${rust_files[@]}") && [ -n "$hits" ]; then
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
