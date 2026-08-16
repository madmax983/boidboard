#!/usr/bin/env bash
# Install Stockfish and make it reachable as bare `stockfish`, idempotently.
#
# Two environment traps this exists to handle (docs/DECISIONS.md D-0015):
#
#   1. Ubuntu's `stockfish` package installs ONLY to /usr/games/stockfish, and /usr/games
#      is not on the PATH of every environment. Without a symlink, issue #3's AC5 command
#      fails with "command not found" even though the package is installed.
#
#   2. `echo -e` is a bashism. Under dash -- which is /bin/sh on Debian and Ubuntu -- it
#      prints a literal "-e" and does not interpret the escape. This script uses printf.
set -euo pipefail

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  SUDO="sudo"
fi

if ! command -v stockfish >/dev/null 2>&1; then
  if [ -x /usr/games/stockfish ]; then
    echo "stockfish present at /usr/games/stockfish but not on PATH; linking"
  else
    echo "installing stockfish"
    $SUDO apt-get update -qq
    $SUDO apt-get install -y stockfish
  fi
  if [ -x /usr/games/stockfish ] && ! [ -x /usr/local/bin/stockfish ]; then
    $SUDO ln -sf /usr/games/stockfish /usr/local/bin/stockfish
  fi
fi

command -v stockfish

# Verify, rather than assume. This is issue #3's AC5 check, written portably.
out=$(printf 'position startpos\ngo perft 3\nquit\n' | stockfish)
if ! printf '%s\n' "$out" | grep -Eq 'Nodes searched[[:space:]]*:[[:space:]]*8902'; then
  echo "stockfish did not report the expected perft(3) count for the initial position."
  echo "expected: Nodes searched: 8902"
  echo "got:"
  printf '%s\n' "$out" | tail -5
  exit 1
fi

echo "stockfish OK: $(printf 'uci\nquit\n' | stockfish | head -1)"
echo "perft(3) startpos = 8902 confirmed"
