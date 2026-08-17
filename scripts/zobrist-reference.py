#!/usr/bin/env python3
"""An independent derivation of boidboard's zobrist key table.

This script exists to be a SECOND implementation, not a copy of the first. It was written
from the specification in docs/DECISIONS.md D-0019 and D-0025 -- splitmix64 with Vigna's
published constants, seeded from the big-endian bytes of b"boidbord", 781 keys in the order
piece-square / side-to-move / castling / en-passant-file, serialised big-endian -- and not
from crates/boid-board/src/zobrist.rs.

The order of operations is what makes it worth anything: this script's digest was computed
BEFORE the constant was pinned in the Rust test source, and the Rust was then made to agree.
Pasting the Rust's own output in here would invert the check into a tautology.

Usage:
    scripts/zobrist-reference.py            # print the digest
    scripts/zobrist-reference.py --keys 3   # print the first N keys, for spot checks
"""

import argparse
import hashlib

MASK = (1 << 64) - 1

# Vigna's published splitmix64 constants.
GAMMA = 0x9E3779B97F4A7C15
MUL1 = 0xBF58476D1CE4E5B9
MUL2 = 0x94D049BB133111EB

SEED = int.from_bytes(b"boidbord", "big")

# 768 piece-square + 1 side-to-move + 4 castling + 8 en-passant file.
KEY_COUNT = 768 + 1 + 4 + 8


def splitmix64(state):
    """Advance `state` by gamma and mix it. Returns (next_state, output)."""
    state = (state + GAMMA) & MASK
    z = state
    z = ((z ^ (z >> 30)) * MUL1) & MASK
    z = ((z ^ (z >> 27)) * MUL2) & MASK
    return state, (z ^ (z >> 31)) & MASK


def table():
    """The 781 keys, in table-index order."""
    keys = []
    state = SEED
    for _ in range(KEY_COUNT):
        state, out = splitmix64(state)
        keys.append(out)
    return keys


def digest(keys):
    """SHA-256 over each key's big-endian bytes, concatenated in table-index order.

    Big-endian is deliberate: every runner this project has is x86-64, so a native-endian
    digest would make an endianness bug permanently invisible.
    """
    body = b"".join(k.to_bytes(8, "big") for k in keys)
    return hashlib.sha256(body).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--keys",
        type=int,
        default=0,
        metavar="N",
        help="also print the first N keys as hex, for spot-checking single values",
    )
    args = parser.parse_args()

    keys = table()
    assert len(keys) == KEY_COUNT, f"expected {KEY_COUNT} keys, built {len(keys)}"

    for index in range(min(args.keys, len(keys))):
        print(f"key[{index}] = {keys[index]:#018x}")

    print(digest(keys))


if __name__ == "__main__":
    main()
