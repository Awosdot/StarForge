//! WebAssembly (WASM) API surface for StarForge.
//!
//! Compiling StarForge's core wallet functionality to WebAssembly lets developers
//! drive common Stellar operations directly from the browser — web-based IDEs,
//! playgrounds, and other web development environments — without installing the
//! native CLI. This crate is intentionally self-contained: it depends only on
//! pure-Rust crypto primitives (no filesystem, networking, or USB/hardware
//! access), so it compiles cleanly to the `wasm32-unknown-unknown` target while
//! the native CLI keeps its full, non-WASM feature set.
//!
//! Build the browser bundle with:
//!
//! ```text
//! wasm-pack build crates/starforge-wasm --target web
//! ```
//!
//! Every exported item is bridged through `wasm-bindgen`; fallible operations
//! return `Result<_, JsValue>` so errors surface as ordinary JavaScript
//! exceptions.

use bip39::{Language, Mnemonic};
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha512;
use stellar_strkey::ed25519::{PrivateKey as StellarPrivateKey, PublicKey as StellarPublicKey};
use wasm_bindgen::prelude::*;

type HmacSha512 = Hmac<Sha512>;

/// A Stellar ed25519 keypair, exposed to JavaScript with `publicKey` /
/// `secretKey` accessors.
#[wasm_bindgen]
pub struct Keypair {
    public_key: String,
    secret_key: String,
}

#[wasm_bindgen]
impl Keypair {
    /// Stellar public key (strkey, begins with `G`).
    #[wasm_bindgen(getter, js_name = publicKey)]
    pub fn public_key(&self) -> String {
        self.public_key.clone()
    }

    /// Stellar secret key (strkey, begins with `S`). Treat as sensitive.
    #[wasm_bindgen(getter, js_name = secretKey)]
    pub fn secret_key(&self) -> String {
        self.secret_key.clone()
    }
}

/// Crate/library version, useful for feature detection from the web client.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Generate a fresh, random Stellar ed25519 keypair.
///
/// Entropy comes from the browser's `crypto.getRandomValues` via `getrandom`.
#[wasm_bindgen(js_name = generateKeypair)]
pub fn generate_keypair() -> Keypair {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);

    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();

    Keypair {
        public_key: StellarPublicKey(verifying_key.to_bytes()).to_string(),
        secret_key: StellarPrivateKey(seed).to_string(),
    }
}

/// Generate a new BIP39 English recovery phrase of the requested length.
///
/// `word_count` must be `12` or `24`.
#[wasm_bindgen(js_name = generateMnemonic)]
pub fn generate_mnemonic(word_count: u32) -> Result<String, JsValue> {
    let count = match word_count {
        12 | 24 => word_count as usize,
        other => return Err(js_err(format!("word count must be 12 or 24 (got {other})"))),
    };

    Mnemonic::generate_in(Language::English, count)
        .map(|m| m.to_string())
        .map_err(|e| js_err(format!("failed to generate mnemonic: {e}")))
}

/// Derive a Stellar keypair from a BIP39 phrase using the SEP-0005 path
/// `m/44'/148'/account'`.
///
/// `passphrase` is the optional BIP39 passphrase (pass `""` for none).
#[wasm_bindgen(js_name = keypairFromMnemonic)]
pub fn keypair_from_mnemonic(
    phrase: &str,
    passphrase: &str,
    account_index: u32,
) -> Result<Keypair, JsValue> {
    let normalized = phrase.split_whitespace().collect::<Vec<_>>().join(" ");
    let mnemonic = Mnemonic::parse_in(Language::English, &normalized)
        .map_err(|e| js_err(format!("invalid recovery phrase: {e}")))?;

    let word_count = mnemonic.word_count();
    if word_count != 12 && word_count != 24 {
        return Err(js_err(format!(
            "recovery phrase must be 12 or 24 words (got {word_count})"
        )));
    }

    let seed = mnemonic.to_seed(passphrase);
    let private_key =
        derive_stellar_private_key(&seed, account_index).map_err(|e| js_err(e.to_string()))?;
    let signing_key = SigningKey::from_bytes(&private_key);
    let verifying_key = signing_key.verifying_key();

    Ok(Keypair {
        public_key: StellarPublicKey(verifying_key.to_bytes()).to_string(),
        secret_key: StellarPrivateKey(private_key).to_string(),
    })
}

/// Return `true` if `address` is a valid Stellar public key (strkey `G...`).
#[wasm_bindgen(js_name = validateAddress)]
pub fn validate_address(address: &str) -> bool {
    StellarPublicKey::from_string(address).is_ok()
}

// ── Browser configuration storage (localStorage) ────────────────────────────
//
// Mirrors the native CLI's on-disk config with a browser-native backing store
// so web sessions can persist preferences (e.g. selected network) across loads.

const CONFIG_PREFIX: &str = "starforge:";

fn local_storage() -> Result<web_sys::Storage, JsValue> {
    web_sys::window()
        .ok_or_else(|| js_err("no browser window available".to_string()))?
        .local_storage()?
        .ok_or_else(|| js_err("localStorage is not available".to_string()))
}

/// Persist a configuration value in browser storage.
#[wasm_bindgen(js_name = configSet)]
pub fn config_set(key: &str, value: &str) -> Result<(), JsValue> {
    local_storage()?.set_item(&format!("{CONFIG_PREFIX}{key}"), value)
}

/// Read a configuration value from browser storage, or `null` if unset.
#[wasm_bindgen(js_name = configGet)]
pub fn config_get(key: &str) -> Result<Option<String>, JsValue> {
    local_storage()?.get_item(&format!("{CONFIG_PREFIX}{key}"))
}

/// Remove a configuration value from browser storage.
#[wasm_bindgen(js_name = configRemove)]
pub fn config_remove(key: &str) -> Result<(), JsValue> {
    local_storage()?.remove_item(&format!("{CONFIG_PREFIX}{key}"))
}

// ── Internal SLIP-0010 ed25519 derivation (SEP-0005) ────────────────────────

fn js_err(message: String) -> JsValue {
    JsValue::from_str(&message)
}

fn derive_stellar_private_key(seed: &[u8], account_index: u32) -> Result<[u8; 32], String> {
    let (mut key, mut chain) = slip10_master(seed)?;
    (key, chain) = slip10_child(key, chain, hardened(44))?;
    (key, chain) = slip10_child(key, chain, hardened(148))?;
    (key, _) = slip10_child(key, chain, hardened(account_index))?;
    Ok(key)
}

fn hardened(index: u32) -> u32 {
    index | 0x8000_0000
}

fn slip10_master(seed: &[u8]) -> Result<([u8; 32], [u8; 32]), String> {
    let mut mac =
        HmacSha512::new_from_slice(b"ed25519 seed").map_err(|_| "HMAC init failed".to_string())?;
    mac.update(seed);
    split_512(&mac.finalize().into_bytes())
}

fn slip10_child(
    parent_key: [u8; 32],
    parent_chain: [u8; 32],
    index: u32,
) -> Result<([u8; 32], [u8; 32]), String> {
    if index < 0x8000_0000 {
        return Err("Stellar derivation requires hardened path segments".to_string());
    }

    let mut mac =
        HmacSha512::new_from_slice(&parent_chain).map_err(|_| "HMAC init failed".to_string())?;
    mac.update(&[0x00]);
    mac.update(&parent_key);
    mac.update(&index.to_be_bytes());
    split_512(&mac.finalize().into_bytes())
}

fn split_512(bytes: &[u8]) -> Result<([u8; 32], [u8; 32]), String> {
    let mut left = [0u8; 32];
    let mut right = [0u8; 32];
    left.copy_from_slice(&bytes[..32]);
    right.copy_from_slice(&bytes[32..]);
    Ok((left, right))
}

// Native unit tests for the pure (non-`wasm-bindgen`) derivation logic. These
// run on the host with `cargo test` and don't require a browser.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic_and_well_formed() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let seed = Mnemonic::parse_in(Language::English, phrase)
            .unwrap()
            .to_seed("");

        let key_a = derive_stellar_private_key(&seed, 0).unwrap();
        let key_b = derive_stellar_private_key(&seed, 0).unwrap();
        assert_eq!(key_a, key_b);

        let public = StellarPublicKey(SigningKey::from_bytes(&key_a).verifying_key().to_bytes())
            .to_string();
        assert!(public.starts_with('G'));
        assert_eq!(public.len(), 56);

        // Different account indices must diverge.
        let key_c = derive_stellar_private_key(&seed, 1).unwrap();
        assert_ne!(key_a, key_c);
    }
}
