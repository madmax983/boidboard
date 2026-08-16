#!/usr/bin/env bash
# Materialise the toolchain pinned in rust-toolchain.toml, with retries.
#
# The pin is an exact patch version (D-0009), which is what makes `clippy -D warnings` mean
# the same thing locally and in CI. The cost is that a runner without that exact toolchain
# must download it from static.rust-lang.org on the first cargo invocation, and that
# download is occasionally reset mid-flight:
#
#   error: component download failed for rust-std-x86_64-unknown-linux-gnu:
#   error sending request ... Connection reset by peer (os error 104)
#
# Observed on GitHub Actions. Without a retry, a network hiccup reads as a build failure,
# which is exactly the kind of noise that teaches people to ignore red CI.
set -euo pipefail

for attempt in 1 2 3 4; do
  if cargo --version >/dev/null 2>&1; then
    break
  fi
  echo "toolchain not ready (attempt ${attempt}/4); retrying"
  sleep $((attempt * 5))
done

# Final invocation is unguarded: if the toolchain still is not usable, fail loudly here
# rather than inside a cargo command, where it would look like a compile error.
rustup show active-toolchain
cargo --version
rustc --version
