//! Watch-only descriptors: parsing, BIP-32 public derivation, P2WPKH addresses and BIP-21 URIs.
//!
//! Only what donations need is implemented, on `secp256k1`, `bitcoin_hashes` and `bech32`
//! (see docs/specs/donations.md, "Why no bdk_wallet"):
//!
//! * `wpkh(KEY)` where `KEY` is `[origin]? xpub|tpub /child* /(index|<a;b>) /*`, children non-hardened.
//! * A trailing BIP-380 checksum (`#xxxxxxxx`) is verified when present.
//! * Any private key material is rejected, never stored, never echoed.

use bech32::{Hrp, segwit};
use bitcoin_hashes::{Hash, HashEngine, Hmac, HmacEngine, hash160, sha256, sha256d, sha512};
use secp256k1::{PublicKey, Scalar, Secp256k1};
use std::fmt;

/// Bitcoin network a wallet watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Network {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}

impl Network {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Signet => "signet",
            Self::Regtest => "regtest",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "mainnet" | "bitcoin" | "main" => Self::Mainnet,
            "testnet" => Self::Testnet,
            "signet" => Self::Signet,
            "regtest" => Self::Regtest,
            _ => return None,
        })
    }

    fn hrp(self) -> &'static str {
        match self {
            Self::Mainnet => "bc",
            Self::Testnet | Self::Signet => "tb",
            Self::Regtest => "bcrt",
        }
    }

    fn pub_version(self) -> [u8; 4] {
        match self {
            Self::Mainnet => XPUB,
            _ => TPUB,
        }
    }

    /// URI scheme prefix of BIP-21 (`bitcoin:` on every network).
    pub const fn uri_scheme(self) -> &'static str {
        "bitcoin"
    }
}

const XPUB: [u8; 4] = [0x04, 0x88, 0xB2, 0x1E];
const TPUB: [u8; 4] = [0x04, 0x35, 0x87, 0xCF];
const PRIVATE_VERSIONS: [[u8; 4]; 4] = [
    [0x04, 0x88, 0xAD, 0xE4], // xprv
    [0x04, 0x35, 0x83, 0x94], // tprv
    [0x04, 0xb2, 0x43, 0x0c], // zprv
    [0x04, 0x5f, 0x18, 0xbc], // vprv
];

/// Why a descriptor was refused. Messages never contain the input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DescriptorError {
    #[error(
        "the descriptor contains private key material; only watch-only (xpub/tpub) descriptors are accepted"
    )]
    PrivateKey,
    #[error("unsupported descriptor: use wpkh(xpub.../0/*)")]
    Unsupported,
    #[error("invalid descriptor: {0}")]
    Invalid(&'static str),
    #[error("the descriptor checksum does not match")]
    Checksum,
    #[error("the extended key belongs to a different network than {0}")]
    NetworkMismatch(&'static str),
}

/// A parsed watch-only `wpkh` descriptor with a wildcard index.
#[derive(Clone)]
pub struct WatchDescriptor {
    network: Network,
    /// Public key and chain code at the wildcard parent (already derived through the fixed path).
    key: PublicKey,
    chain: [u8; 32],
    normalized: String,
}

impl fmt::Debug for WatchDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WatchDescriptor")
            .field("network", &self.network)
            .field("fingerprint", &self.fingerprint())
            .finish()
    }
}

impl WatchDescriptor {
    /// Parse and validate `text` for `network`. The text may carry a `#checksum`.
    pub fn parse(text: &str, network: Network) -> Result<Self, DescriptorError> {
        let text = text.trim();
        if looks_private(text) {
            return Err(DescriptorError::PrivateKey);
        }
        let body = match text.split_once('#') {
            Some((body, sum)) => {
                if descriptor_checksum(body).as_deref() != Some(sum) {
                    return Err(DescriptorError::Checksum);
                }
                body
            }
            None => text,
        };
        let inner = body
            .strip_prefix("wpkh(")
            .and_then(|r| r.strip_suffix(')'))
            .ok_or(DescriptorError::Unsupported)?;
        if inner.contains('(') || inner.contains(')') {
            return Err(DescriptorError::Unsupported);
        }
        // Optional origin `[fingerprint/path]`.
        let rest = match inner.strip_prefix('[') {
            Some(r) => {
                let (origin, rest) = r
                    .split_once(']')
                    .ok_or(DescriptorError::Invalid("unterminated key origin"))?;
                let mut parts = origin.split('/');
                let fp = parts.next().unwrap_or("");
                if fp.len() != 8 || !fp.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(DescriptorError::Invalid("bad origin fingerprint"));
                }
                for p in parts {
                    let p = p.trim_end_matches(['h', 'H', '\'']);
                    if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                        return Err(DescriptorError::Invalid("bad origin path"));
                    }
                }
                rest
            }
            None => inner,
        };
        let mut segs = rest.split('/');
        let xpub = segs.next().unwrap_or("");
        let (key, chain) = decode_xpub(xpub, network)?;
        let mut path: Vec<u32> = Vec::new();
        let mut wildcard = false;
        for seg in segs {
            if wildcard {
                return Err(DescriptorError::Invalid("nothing may follow the wildcard"));
            }
            if seg == "*" {
                wildcard = true;
            } else if let Some(multi) = seg.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
                // `<0;1>`: the first branch is the receive chain.
                let first = multi
                    .split(';')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|n| *n < HARDENED)
                    .ok_or(DescriptorError::Invalid("bad multipath"))?;
                path.push(first);
            } else {
                let n = seg.parse::<u32>().ok().filter(|n| *n < HARDENED).ok_or(
                    DescriptorError::Invalid("only non-hardened derivation can be watched"),
                )?;
                path.push(n);
            }
        }
        if !wildcard {
            return Err(DescriptorError::Invalid(
                "a wildcard index (/*) is required",
            ));
        }
        let secp = Secp256k1::verification_only();
        let (mut key, mut chain) = (key, chain);
        for i in path {
            (key, chain) = ckd_pub(&secp, &key, &chain, i)?;
        }
        Ok(Self {
            network,
            key,
            chain,
            normalized: body.to_owned(),
        })
    }

    pub fn network(&self) -> Network {
        self.network
    }

    /// First 8 hex characters of SHA-256 of the descriptor body: safe to show, identifies the wallet.
    pub fn fingerprint(&self) -> String {
        let h = sha256::Hash::hash(self.normalized.as_bytes()).to_byte_array();
        hex(&h[..4])
    }

    /// The descriptor body without its checksum (what is stored).
    pub fn body(&self) -> &str {
        &self.normalized
    }

    /// The P2WPKH address at wildcard index `index`.
    pub fn address(&self, index: u32) -> Result<String, DescriptorError> {
        if index >= HARDENED {
            return Err(DescriptorError::Invalid("index out of range"));
        }
        let secp = Secp256k1::verification_only();
        let (child, _) = ckd_pub(&secp, &self.key, &self.chain, index)?;
        let program = hash160::Hash::hash(&child.serialize()).to_byte_array();
        let hrp = Hrp::parse(self.network.hrp()).map_err(|_| DescriptorError::Invalid("hrp"))?;
        segwit::encode_v0(hrp, &program).map_err(|_| DescriptorError::Invalid("address"))
    }
}

const HARDENED: u32 = 0x8000_0000;

fn looks_private(text: &str) -> bool {
    // Extended private keys, WIF-looking tokens and raw hex secrets inside key positions.
    let lower = text.to_ascii_lowercase();
    if ["xprv", "tprv", "zprv", "vprv", "yprv", "uprv"]
        .iter()
        .any(|p| lower.contains(p))
    {
        return true;
    }
    // A 12/24 word seed or a bare WIF key pasted instead of a descriptor.
    let t = text.trim();
    let wif = (t.len() == 51 || t.len() == 52)
        && t.bytes().all(|b| b.is_ascii_alphanumeric())
        && matches!(t.as_bytes()[0], b'5' | b'K' | b'L' | b'c' | b'9');
    wif || t.split_whitespace().count() >= 12
}

fn ckd_pub(
    secp: &Secp256k1<secp256k1::VerifyOnly>,
    key: &PublicKey,
    chain: &[u8; 32],
    index: u32,
) -> Result<(PublicKey, [u8; 32]), DescriptorError> {
    let mut eng = HmacEngine::<sha512::Hash>::new(chain);
    eng.input(&key.serialize());
    eng.input(&index.to_be_bytes());
    let i = Hmac::<sha512::Hash>::from_engine(eng).to_byte_array();
    let mut il = [0u8; 32];
    il.copy_from_slice(&i[..32]);
    let mut ir = [0u8; 32];
    ir.copy_from_slice(&i[32..]);
    let tweak =
        Scalar::from_be_bytes(il).map_err(|_| DescriptorError::Invalid("derivation failed"))?;
    let child = key
        .add_exp_tweak(secp, &tweak)
        .map_err(|_| DescriptorError::Invalid("derivation failed"))?;
    Ok((child, ir))
}

fn decode_xpub(s: &str, network: Network) -> Result<(PublicKey, [u8; 32]), DescriptorError> {
    let raw = base58check_decode(s).ok_or(DescriptorError::Invalid("bad extended key"))?;
    if raw.len() != 78 {
        return Err(DescriptorError::Invalid("bad extended key length"));
    }
    let version: [u8; 4] = [raw[0], raw[1], raw[2], raw[3]];
    if PRIVATE_VERSIONS.contains(&version) {
        return Err(DescriptorError::PrivateKey);
    }
    if version != network.pub_version() {
        if version == XPUB || version == TPUB {
            return Err(DescriptorError::NetworkMismatch(network.as_str()));
        }
        return Err(DescriptorError::Invalid(
            "unsupported extended key type (use xpub or tpub)",
        ));
    }
    let mut chain = [0u8; 32];
    chain.copy_from_slice(&raw[13..45]);
    let key = PublicKey::from_slice(&raw[45..78])
        .map_err(|_| DescriptorError::Invalid("bad public key"))?;
    Ok((key, chain))
}

// ---- base58check -------------------------------------------------------------------------------

const B58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Decodes and verifies the 4-byte double-SHA-256 checksum; returns the payload.
pub fn base58check_decode(s: &str) -> Option<Vec<u8>> {
    let mut bytes: Vec<u8> = Vec::new(); // big-endian base-256, built by repeated multiply-add
    for c in s.bytes() {
        let mut carry = B58.iter().position(|&b| b == c)? as u32;
        for b in bytes.iter_mut().rev() {
            let v = u32::from(*b) * 58 + carry;
            *b = (v & 0xff) as u8;
            carry = v >> 8;
        }
        while carry > 0 {
            bytes.insert(0, (carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let zeros = s.bytes().take_while(|&c| c == b'1').count();
    let mut out = vec![0u8; zeros];
    out.extend(bytes);
    if out.len() < 5 {
        return None;
    }
    let (payload, sum) = out.split_at(out.len() - 4);
    (sha256d::Hash::hash(payload).to_byte_array()[..4] == *sum).then(|| payload.to_vec())
}

/// Encodes `payload` with its checksum.
pub fn base58check_encode(payload: &[u8]) -> String {
    let mut data = payload.to_vec();
    data.extend_from_slice(&sha256d::Hash::hash(payload).to_byte_array()[..4]);
    let zeros = data.iter().take_while(|&&b| b == 0).count();
    let mut digits: Vec<u8> = Vec::new(); // base 58, little-endian
    for &byte in &data {
        let mut carry = u32::from(byte);
        for d in digits.iter_mut() {
            let v = u32::from(*d) * 256 + carry;
            *d = (v % 58) as u8;
            carry = v / 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut s = String::new();
    s.extend(std::iter::repeat_n('1', zeros));
    s.extend(digits.iter().rev().map(|&d| B58[usize::from(d)] as char));
    s
}

// ---- BIP-380 checksum --------------------------------------------------------------------------

const INPUT_CHARSET: &str = "0123456789()[],'/*abcdefgh@:$%{}IJKLMNOPQRSTUVWXYZ&+-.;<=>?!^_|~ijklmnopqrstuvwxyzABCDEFGH`#\"\\ ";
const CHECKSUM_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

#[allow(clippy::unusual_byte_groupings)]
fn polymod(mut c: u64, val: u64) -> u64 {
    let c0 = c >> 35;
    c = ((c & 0x7_ffff_ffff) << 5) ^ val;
    for (bit, gen_) in [
        (1u64, 0xf5de_e519_89u64),
        (2, 0xa9fd_ca33_12),
        (4, 0x1bab_10e3_2d),
        (8, 0x3706_b167_7a),
        (16, 0x644d_626f_fd),
    ] {
        if c0 & bit != 0 {
            c ^= gen_;
        }
    }
    c
}

/// The 8-character descriptor checksum of `desc` (without any `#`), or `None` for unknown characters.
pub fn descriptor_checksum(desc: &str) -> Option<String> {
    let (mut c, mut cls, mut clscount) = (1u64, 0u64, 0u32);
    for ch in desc.chars() {
        let pos = INPUT_CHARSET.find(ch)? as u64;
        c = polymod(c, pos & 31);
        cls = cls * 3 + (pos >> 5);
        clscount += 1;
        if clscount == 3 {
            c = polymod(c, cls);
            cls = 0;
            clscount = 0;
        }
    }
    if clscount > 0 {
        c = polymod(c, cls);
    }
    for _ in 0..8 {
        c = polymod(c, 0);
    }
    c ^= 1;
    Some(
        (0..8)
            .map(|j| CHECKSUM_CHARSET[((c >> (5 * (7 - j))) & 31) as usize] as char)
            .collect(),
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---- BIP-21 ------------------------------------------------------------------------------------

/// Formats satoshis as a BTC decimal string with 8 places and trailing zeros trimmed (`0.0001`).
pub fn btc_string(sats: u64) -> String {
    let whole = sats / 100_000_000;
    let frac = sats % 100_000_000;
    if frac == 0 {
        return whole.to_string();
    }
    let f = format!("{frac:08}");
    format!("{whole}.{}", f.trim_end_matches('0'))
}

/// `bitcoin:ADDRESS?amount=BTC&label=...` (BIP-21). The label is percent-encoded.
pub fn bip21(address: &str, sats: u64, label: Option<&str>) -> String {
    let mut uri = format!("bitcoin:{address}?amount={}", btc_string(sats));
    if let Some(l) = label {
        uri.push_str("&label=");
        for b in l.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                uri.push(b as char);
            } else {
                uri.push_str(&format!("%{b:02X}"));
            }
        }
    }
    uri
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// BIP-84 test vector, account 0 of the "abandon ... about" mnemonic (public test data).
    pub const BIP84_ZPUB: &str = "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs";

    /// The same extended key re-serialised with the `xpub` version bytes (what descriptors use).
    pub fn bip84_xpub() -> String {
        let mut raw = base58check_decode(BIP84_ZPUB).unwrap();
        raw[..4].copy_from_slice(&XPUB);
        base58check_encode(&raw)
    }

    /// The same key with `tpub` version bytes, for test networks (derives different HRP only).
    pub fn bip84_tpub() -> String {
        let mut raw = base58check_decode(BIP84_ZPUB).unwrap();
        raw[..4].copy_from_slice(&TPUB);
        base58check_encode(&raw)
    }

    #[test]
    fn base58_round_trips() {
        let p = [0u8, 0, 1, 2, 3, 250, 251];
        assert_eq!(base58check_decode(&base58check_encode(&p)).unwrap(), p);
        assert!(base58check_decode("0OIl").is_none());
        let mut s = base58check_encode(&p);
        s.push('2');
        assert!(base58check_decode(&s).is_none());
    }

    #[test]
    fn bip380_checksum_vector() {
        assert_eq!(descriptor_checksum("raw(deadbeef)").unwrap(), "89f8spxm");
    }

    #[test]
    fn bip84_addresses() {
        let d = WatchDescriptor::parse(&format!("wpkh({}/0/*)", bip84_xpub()), Network::Mainnet)
            .unwrap();
        assert_eq!(
            d.address(0).unwrap(),
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
        );
        assert_eq!(
            d.address(1).unwrap(),
            "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g"
        );
        let change =
            WatchDescriptor::parse(&format!("wpkh({}/1/*)", bip84_xpub()), Network::Mainnet)
                .unwrap();
        assert_eq!(
            change.address(0).unwrap(),
            "bc1q8c6fshw2dlwun7ekn9qwf37cu2rn755upcp6el"
        );
    }

    #[test]
    fn multipath_uses_the_receive_branch_and_origin_is_accepted() {
        let plain =
            WatchDescriptor::parse(&format!("wpkh({}/0/*)", bip84_xpub()), Network::Mainnet)
                .unwrap();
        let multi = WatchDescriptor::parse(
            &format!("wpkh([73c5da0a/84h/0h/0h]{}/<0;1>/*)", bip84_xpub()),
            Network::Mainnet,
        )
        .unwrap();
        assert_eq!(plain.address(7).unwrap(), multi.address(7).unwrap());
    }

    #[test]
    fn test_networks_use_tpub_and_their_hrp() {
        let d = WatchDescriptor::parse(&format!("wpkh({}/0/*)", bip84_tpub()), Network::Signet)
            .unwrap();
        assert!(d.address(0).unwrap().starts_with("tb1q"));
        let r = WatchDescriptor::parse(&format!("wpkh({}/0/*)", bip84_tpub()), Network::Regtest)
            .unwrap();
        assert!(r.address(0).unwrap().starts_with("bcrt1q"));
        // A tpub on mainnet and an xpub on signet are refused.
        assert_eq!(
            WatchDescriptor::parse(&format!("wpkh({}/0/*)", bip84_tpub()), Network::Mainnet)
                .unwrap_err(),
            DescriptorError::NetworkMismatch("mainnet")
        );
        assert!(
            WatchDescriptor::parse(&format!("wpkh({}/0/*)", bip84_xpub()), Network::Signet)
                .is_err()
        );
    }

    #[test]
    fn checksum_is_enforced_when_present() {
        let body = format!("wpkh({}/0/*)", bip84_xpub());
        let sum = descriptor_checksum(&body).unwrap();
        assert!(WatchDescriptor::parse(&format!("{body}#{sum}"), Network::Mainnet).is_ok());
        assert_eq!(
            WatchDescriptor::parse(&format!("{body}#qqqqqqqq"), Network::Mainnet).unwrap_err(),
            DescriptorError::Checksum
        );
    }

    #[test]
    fn private_material_and_unsupported_shapes_are_refused() {
        // Built from parts so no key-looking literal sits in the source.
        let xprv = format!("{}{}", "xprv", "JUNKJUNKJUNKJUNK");
        assert_eq!(
            WatchDescriptor::parse(&format!("wpkh({xprv}/0/*)"), Network::Mainnet).unwrap_err(),
            DescriptorError::PrivateKey
        );
        assert_eq!(
            WatchDescriptor::parse("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about", Network::Mainnet)
                .unwrap_err(),
            DescriptorError::PrivateKey
        );
        let x = bip84_xpub();
        for bad in [
            format!("pkh({x}/0/*)"),
            format!("wpkh({x}/0)"),
            format!("wpkh({x}/0h/*)"),
            format!("wpkh({x}/*/0)"),
            "wpkh(nonsense/0/*)".to_owned(),
            format!("wpkh([zz/84h]{x}/0/*)"),
        ] {
            assert!(
                WatchDescriptor::parse(&bad, Network::Mainnet).is_err(),
                "{bad}"
            );
        }
        // The error text never echoes the input.
        let e =
            WatchDescriptor::parse("wpkh(secret-looking-input/0/*)", Network::Mainnet).unwrap_err();
        assert!(!e.to_string().contains("secret-looking"));
    }

    #[test]
    fn fingerprint_is_stable_and_short() {
        let d = WatchDescriptor::parse(&format!("wpkh({}/0/*)", bip84_xpub()), Network::Mainnet)
            .unwrap();
        assert_eq!(d.fingerprint().len(), 8);
        assert_eq!(d.fingerprint(), d.fingerprint());
        assert!(!format!("{d:?}").contains("xpub"));
    }

    #[test]
    fn bip21_uri_and_btc_strings() {
        assert_eq!(btc_string(100_000_000), "1");
        assert_eq!(btc_string(7_693), "0.00007693");
        assert_eq!(btc_string(150_000_000), "1.5");
        assert_eq!(btc_string(1), "0.00000001");
        assert_eq!(
            bip21(
                "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu",
                50_000,
                Some("SIDE ROOM #1")
            ),
            "bitcoin:bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu?amount=0.0005&label=SIDE%20ROOM%20%231"
        );
    }
}
