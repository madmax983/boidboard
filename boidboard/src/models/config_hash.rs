//! Canonical hashing of a scenario configuration.
//!
//! A run's provenance record (AC-38) pins the exact parameter set it executed
//! by hash. For that hash to be meaningful it must be **canonical**: two JSON
//! documents that mean the same thing must hash the same, no matter what order
//! their keys arrived in (`serde_json` preserves insertion order under the
//! `preserve_order` feature, and HTTP form/JSON round-trips reorder freely).
//!
//! So we serialise to a canonical form — object keys sorted, no insignificant
//! whitespace — and take a SHA-256 of the bytes.
//!
//! The SHA-256 is implemented here by hand, on purpose: adding a crypto crate
//! to a workspace for one 32-byte digest is not a trade worth making, and a
//! hand-rolled digest keeps the provenance format under our own control. It is
//! pinned against the FIPS 180-4 test vectors in the unit tests below.

use serde_json::Value;

/// Hash a configuration document canonically.
///
/// Stable regardless of JSON key order at any nesting depth; sensitive to every
/// value, to array order, and to the presence or absence of any key. Returns a
/// lowercase 64-character hex SHA-256 digest.
#[must_use]
pub fn canonical_config_hash(config: &Value) -> String {
    let mut canonical = String::new();
    write_canonical(config, &mut canonical);
    hex(&sha256(canonical.as_bytes()))
}

/// Render `value` into `out` in canonical JSON form: object keys in sorted
/// order, arrays in their given order, no insignificant whitespace.
fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_json_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
    }
}

/// Write a JSON string literal, escaping exactly what RFC 8259 requires
/// (matching `serde_json`'s default, which does not escape `/` or non-ASCII).
fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ── SHA-256 (FIPS 180-4) ─────────────────────────────────────────

/// Round constants: the first 32 bits of the fractional parts of the cube roots
/// of the first 64 primes.
#[rustfmt::skip]
const K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
    0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
    0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
    0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
    0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
    0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
    0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
];

/// SHA-256 digest of `data`.
fn sha256(data: &[u8]) -> [u8; 32] {
    // Initial hash values: the first 32 bits of the fractional parts of the
    // square roots of the first 8 primes.
    #[rustfmt::skip]
    let mut h: [u32; 8] = [
        0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a,
        0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19,
    ];

    // Pad: 0x80, zeroes, then the message length in bits as a big-endian u64.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (word, bytes) in w.iter_mut().zip(chunk.chunks_exact(4)) {
            *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for i in 16..64 {
            let x = w[i - 15];
            let y = w[i - 2];
            let s0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
            let s1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut v = h;
        for (k, wi) in K.iter().zip(w.iter()) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(*wi);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);

            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(t1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = t1.wrapping_add(t2);
        }

        for (acc, add) in h.iter_mut().zip(v.iter()) {
            *acc = acc.wrapping_add(*add);
        }
    }

    let mut out = [0u8; 32];
    for (slot, word) in out.chunks_exact_mut(4).zip(h.iter()) {
        slot.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Lowercase hex encoding.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[usize::from(b >> 4)] as char);
        s.push(DIGITS[usize::from(b & 0x0f)] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{canonical_config_hash, hex, sha256, write_canonical};

    #[test]
    fn sha256_matches_the_fips_180_4_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // Multi-block input, exercising the length-padding edge (1 000 000 'a').
        let million_a = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&million_a)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn canonical_form_sorts_object_keys_at_every_depth() {
        let mut out = String::new();
        write_canonical(
            &serde_json::json!({ "b": { "z": 1, "a": 2 }, "a": [3, 1] }),
            &mut out,
        );
        assert_eq!(out, r#"{"a":[3,1],"b":{"a":2,"z":1}}"#);
    }

    #[test]
    fn canonical_form_escapes_strings_like_serde_json() {
        let mut out = String::new();
        write_canonical(&serde_json::json!("a\"b\\c\nd\te\u{1}"), &mut out);
        assert_eq!(out, serde_json::to_string("a\"b\\c\nd\te\u{1}").unwrap());
    }

    #[test]
    fn empty_object_and_empty_array_are_distinguishable() {
        assert_ne!(
            canonical_config_hash(&serde_json::json!({})),
            canonical_config_hash(&serde_json::json!([]))
        );
    }
}
