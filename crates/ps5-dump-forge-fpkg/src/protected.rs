//! Protected container entries (`flags1` bit 31: `nptitle.dat`, `npbind.dat`), the inverse of the
//! builder's `crypto::encrypt_entry`. IV ‖ AES-128 key = SHA3-256(stored 32-byte row ‖ entry key),
//! CBC; the entry key is the passcode-derived key `flags2 >> 12` names.

use ps5upload_fpkg::cnt::ids;
use ps5upload_fpkg::crypto::{derive_pfs_key, sha3};

use crate::pfs::{Container, Entry, Pkg};

pub(crate) const FLAG: u32 = 0x8000_0000;

/// Protected entries are a few KiB.
const MAX_SIZE: u64 = 1 << 20;

const KEY_COUNT: u32 = 7;

/// `data` must be whole 16-byte blocks.
fn aes128_cbc_decrypt(key: &[u8; 16], iv: [u8; 16], data: &mut [u8]) {
    use aes::cipher::{BlockCipherDecrypt, KeyInit as _};
    debug_assert!(data.len().is_multiple_of(16));
    let cipher = aes::Aes128::new(&(*key).into());
    let mut prev = iv;
    for chunk in data.as_chunks_mut::<16>().0 {
        let cipher_block = *chunk;
        let mut blk = aes::Block::from(cipher_block);
        cipher.decrypt_block(&mut blk);
        chunk.copy_from_slice(&blk);
        for (b, p) in chunk.iter_mut().zip(prev) {
            *b ^= p;
        }
        prev = cipher_block;
    }
}

pub(crate) fn decrypt_entry(row: &[u8; 32], entry_key: &[u8; 32], data: &mut [u8]) {
    let mut pre = Vec::with_capacity(64);
    pre.extend_from_slice(row);
    pre.extend_from_slice(entry_key);
    let iv_key = sha3(&pre);
    aes128_cbc_decrypt(
        &iv_key[16..].try_into().unwrap(),
        iv_key[..16].try_into().unwrap(),
        data,
    );
}

/// Whether key `k` passes the entry-keys slot (a 32-byte seed digest, then `SHA3(k) ^ k` per key);
/// `None` when the slot has no digest for it.
pub(crate) fn key_check(
    pkg: &mut Pkg,
    cnt: &Container,
    key: &[u8; 32],
    k: u32,
) -> crate::Result<Option<bool>> {
    let digest_at = 32 + 32 * u64::from(k);
    let Some(keys) = cnt
        .entry(ids::ENTRY_KEYS)
        .filter(|ek| ek.size >= digest_at + 32)
    else {
        return Ok(None);
    };
    let mut stored = [0u8; 32];
    pkg.read_at(keys.at + digest_at, &mut stored)?;
    let mut want = sha3(key);
    for (w, b) in want.iter_mut().zip(key) {
        *w ^= b;
    }
    Ok(Some(stored == want))
}

/// The plaintext of protected entry `e`, or why not. The passcode is key-checked first: a wrong
/// one would otherwise write garbage as the file (a plaintext image never tests it).
pub(crate) fn plaintext(
    pkg: &mut Pkg,
    cnt: &Container,
    e: &Entry,
    passcode: &str,
) -> Result<Vec<u8>, String> {
    if e.size > MAX_SIZE {
        return Err(format!("{} bytes, over the {MAX_SIZE} it decrypts", e.size));
    }
    let k = (e.flags2 >> 12) & 0xF;
    if k >= KEY_COUNT {
        return Err(format!(
            "it names key index {k}, which the container has no key for"
        ));
    }
    let key = derive_pfs_key(&cnt.content_id, passcode, k);
    match key_check(pkg, cnt, &key, k).map_err(|e| e.to_string())? {
        None => return Err("the container has no key check for its key index".into()),
        Some(false) => return Err("the passcode fails the container's key check".into()),
        Some(true) => {}
    }
    let mut data = vec![0u8; e.size.next_multiple_of(16) as usize];
    pkg.read_at(e.at, &mut data).map_err(|e| e.to_string())?;
    decrypt_entry(&e.row, &key, &mut data);
    data.truncate(e.size as usize);
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ps5upload_fpkg::crypto::{DEFAULT_PASSCODE, encrypt_entry};

    const CONTENT_ID: &str = "UP0000-PPSA01234_00-TESTGAME00000000";
    const PLAIN_LEN: usize = 50;

    /// A container with the entry-keys slot and a protected `nptitle.dat` under `passcode`'s key 3.
    fn container(passcode: &str) -> (Pkg, Container, Vec<u8>) {
        let (keys_at, title_at) = (0x1100usize, 0x1200usize);
        let plain: Vec<u8> = (0..PLAIN_LEN).map(|i| (i * 7 + 1) as u8).collect();
        let padded = PLAIN_LEN.next_multiple_of(16);
        let mut c = vec![0u8; title_at + padded];
        c[0..4].copy_from_slice(&0x7F43_4E54u32.to_be_bytes());
        c[0x10..0x14].copy_from_slice(&2u32.to_be_bytes());
        c[0x18..0x1C].copy_from_slice(&0x1000u32.to_be_bytes());
        c[0x40..0x40 + CONTENT_ID.len()].copy_from_slice(CONTENT_ID.as_bytes());
        let row =
            |c: &mut Vec<u8>, i: usize, id: u32, flags: (u32, u32), at: usize, size: usize| {
                let o = 0x1000 + i * 0x20;
                c[o..o + 4].copy_from_slice(&id.to_be_bytes());
                c[o + 8..o + 12].copy_from_slice(&flags.0.to_be_bytes());
                c[o + 12..o + 16].copy_from_slice(&flags.1.to_be_bytes());
                c[o + 16..o + 20].copy_from_slice(&(at as u32).to_be_bytes());
                c[o + 20..o + 24].copy_from_slice(&(size as u32).to_be_bytes());
            };
        row(&mut c, 0, ids::ENTRY_KEYS, (0, 0), keys_at, 32 + 7 * 32);
        row(&mut c, 1, 0x0402, (FLAG, 3 << 12), title_at, PLAIN_LEN);
        for k in 0..KEY_COUNT {
            let key = derive_pfs_key(CONTENT_ID, passcode, k);
            let mut digest = sha3(&key);
            for (d, b) in digest.iter_mut().zip(key) {
                *d ^= b;
            }
            let at = keys_at + 32 + 32 * k as usize;
            c[at..at + 32].copy_from_slice(&digest);
        }
        let row_bytes: [u8; 32] = c[0x1020..0x1040].try_into().unwrap();
        let mut data = plain.clone();
        data.resize(padded, 0);
        encrypt_entry(
            &row_bytes,
            &derive_pfs_key(CONTENT_ID, passcode, 3),
            &mut data,
        );
        c[title_at..].copy_from_slice(&data);
        let len = c.len() as u64;
        let mut pkg = Pkg::new(Box::new(std::io::Cursor::new(c)), len);
        let cnt = crate::pfs::container(&mut pkg, 0).unwrap();
        (pkg, cnt, plain)
    }

    #[test]
    fn a_protected_entry_reads_back_as_its_file() {
        let (mut pkg, cnt, plain) = container(DEFAULT_PASSCODE);
        let e = cnt.entry(0x0402).unwrap();
        assert_eq!(
            plaintext(&mut pkg, &cnt, &e, DEFAULT_PASSCODE).unwrap(),
            plain
        );
    }

    #[test]
    fn a_wrong_passcode_yields_nothing() {
        let (mut pkg, cnt, _) = container(DEFAULT_PASSCODE);
        let e = cnt.entry(0x0402).unwrap();
        let other = "11111111111111111111111111111111";
        let why = plaintext(&mut pkg, &cnt, &e, other).unwrap_err();
        assert!(why.contains("key check"), "{why}");
    }
}
