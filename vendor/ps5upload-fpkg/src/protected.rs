//! Protected container entries (`flags1` bit 31): `nptitle.dat` and the two `npbind.dat` files a
//! package can keep only in its container, encrypted. [`crate::crypto::encrypt_entry`] is the
//! writer; this is its inverse.
//!
//! Ported from PS5 Dump Forge (`crates/ps5-dump-forge-fpkg/src/protected.rs`, its PR #47
//! "decrypt protected container entries instead of dropping them").
//!
//! key/iv = SHA3-256(row ‖ entry key): the first 16 bytes the IV, the last 16 the AES-128 key,
//! CBC. `row` is the entry's 32-byte table row as stored, and the entry key is the
//! passcode-derived key ([`derive_pfs_key`]) the row's `flags2 >> 12` names. Before a byte is
//! decrypted the key is tested against the container's entry-keys slot, so a wrong passcode
//! never passes garbage off as the file.

use crate::be32;
use crate::cnt::{ids, Cnt, Entry};
use crate::crypto::{aes128_cbc_decrypt, derive_pfs_key, sha3};

/// `flags1` bit 31: the entry is stored encrypted.
pub const FLAG: u32 = 0x8000_0000;

/// The most a protected entry may be and still be decrypted: they are a few KiB.
const MAX_SIZE: u32 = 1 << 20;

/// The entry-keys slot holds keys 0..=6.
const KEY_COUNT: u32 = 7;

const ROW_LEN: usize = 0x20;

/// `e` is stored encrypted.
pub fn is_protected(e: &Entry) -> bool {
    e.flags1 & FLAG != 0
}

/// Decrypt a protected entry in place: the inverse of [`crate::crypto::encrypt_entry`]. `data`
/// is whole 16-byte blocks.
pub fn decrypt_entry(row: &[u8; 32], entry_key: &[u8; 32], data: &mut [u8]) {
    let mut pre = Vec::with_capacity(64);
    pre.extend_from_slice(row);
    pre.extend_from_slice(entry_key);
    let iv_key = sha3(&pre);
    // The digest's first half is the IV, its second half the key.
    let key: [u8; 16] = std::array::from_fn(|i| iv_key[16 + i]);
    let iv: [u8; 16] = std::array::from_fn(|i| iv_key[i]);
    aes128_cbc_decrypt(&key, iv, data);
}

/// Whether `key`, the passcode-derived key `k`, passes the container's entry-keys slot (the
/// 32-byte seed digest, then `SHA3(k) ^ k` for keys 0..=6); `None` when the slot has no digest
/// for it.
pub fn key_check(cnt: &Cnt, key: &[u8; 32], k: u32) -> Option<bool> {
    let at = 32 + 32 * usize::try_from(k).ok()?;
    let keys = cnt.entry(ids::ENTRY_KEYS)?;
    let stored = cnt.payload(keys).get(at..at.checked_add(32)?)?;
    let mut want = sha3(key);
    for (w, b) in want.iter_mut().zip(key) {
        *w ^= b;
    }
    Some(stored == want)
}

/// The 32-byte table row of the container's entry `id`, as stored.
fn row_of(cnt: &Cnt, id: u32) -> Option<[u8; 32]> {
    let i = cnt.entries.iter().position(|e| e.id == id)?;
    let table = usize::try_from(be32(cnt.bytes.get(0x18..0x1C)?, 0)).ok()?;
    let at = table.checked_add(i.checked_mul(ROW_LEN)?)?;
    cnt.bytes.get(at..at.checked_add(ROW_LEN)?)?.try_into().ok()
}

/// The plaintext of the protected entry `e`, or why it can't be had (a reason in plain words,
/// for a note that says the file was left out).
pub fn plaintext(cnt: &Cnt, e: &Entry, passcode: &str) -> std::result::Result<Vec<u8>, String> {
    if e.size > MAX_SIZE {
        return Err(format!(
            "it is {} bytes, more than the {MAX_SIZE} this app decrypts",
            e.size
        ));
    }
    let k = (e.flags2 >> 12) & 0xF;
    if k >= KEY_COUNT {
        return Err(format!(
            "it names key {k}, which the package has no key for"
        ));
    }
    let key = derive_pfs_key(&cnt.content_id, passcode, k);
    match key_check(cnt, &key, k) {
        None => return Err("the package has no key check for its key".into()),
        Some(false) => {
            return Err("it is encrypted with a passcode other than the one used to open it".into())
        }
        Some(true) => {}
    }
    let row = row_of(cnt, e.id).ok_or("its row in the container's table is missing")?;
    // Stored padded to the cipher's block; `size` is the file's own length.
    let size = e.size as usize;
    let start = e.offset as usize;
    let mut data = start
        .checked_add(size.next_multiple_of(16))
        .and_then(|end| cnt.bytes.get(start..end))
        .ok_or("the package is cut short inside it")?
        .to_vec();
    decrypt_entry(&row, &key, &mut data);
    data.truncate(size);
    Ok(data)
}
