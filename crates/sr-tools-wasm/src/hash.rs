//! MD5 and CRC-32 (UTL-DEV-001). The SHA family goes through the browser's `SubtleCrypto`, which does not
//! offer MD5 (broken for security, still asked for as a checksum) or CRC-32; those two live here.
//! Neither is for security: the tool says so.

use wasm_bindgen::prelude::*;

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

fn k_table() -> [u32; 64] {
    // K[i] = floor(2^32 * abs(sin(i + 1))), RFC 1321.
    let mut k = [0u32; 64];
    for (i, v) in k.iter_mut().enumerate() {
        *v = ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32;
    }
    k
}

/// MD5 digest of `data`.
pub fn md5(data: &[u8]) -> [u8; 16] {
    let k = k_table();
    let (mut a0, mut b0, mut c0, mut d0) = (
        0x6745_2301u32,
        0xefcd_ab89u32,
        0x98ba_dcfeu32,
        0x1032_5476u32,
    );
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&(data.len() as u64).wrapping_mul(8).to_le_bytes());
    for chunk in msg.as_chunks::<64>().0 {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u32::from_le_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, v) in [a0, b0, c0, d0].iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// CRC-32 (IEEE 802.3, the one in zip and PNG).
pub fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *t = c;
    }
    !data.iter().fold(!0u32, |crc, &b| {
        table[((crc ^ u32::from(b)) & 0xff) as usize] ^ (crc >> 8)
    })
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// MD5 of `data` as lowercase hex.
#[wasm_bindgen(js_name = hashMd5)]
pub fn hash_md5(data: &[u8]) -> String {
    to_hex(&md5(data))
}

/// CRC-32 of `data` as eight lowercase hex digits.
#[wasm_bindgen(js_name = hashCrc32)]
pub fn hash_crc32(data: &[u8]) -> String {
    format!("{:08x}", crc32(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_known_vectors_rfc_1321() {
        let cases = [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
            (
                "The quick brown fox jumps over the lazy dog",
                "9e107d9d372bb6826bd81d3542a419d6",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(hash_md5(input.as_bytes()), want, "{input}");
        }
    }

    #[test]
    fn md5_padding_boundaries() {
        // Lengths around the 56 and 64 byte boundaries must not panic and must all differ.
        let mut seen = std::collections::HashSet::new();
        for n in 54..70 {
            assert!(seen.insert(hash_md5(&vec![b'x'; n])));
        }
    }

    #[test]
    fn crc32_known_vectors() {
        assert_eq!(hash_crc32(b""), "00000000");
        assert_eq!(hash_crc32(b"123456789"), "cbf43926");
        assert_eq!(hash_crc32(b"a"), "e8b7be43");
        assert_eq!(
            hash_crc32(b"The quick brown fox jumps over the lazy dog"),
            "414fa339"
        );
    }
}
