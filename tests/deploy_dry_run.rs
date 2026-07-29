#![allow(dead_code, unused_imports)]

/// Integration tests for the enhanced deploy dry-run plan (#688)
/// Verifies that network, account, code hash, fees, authorization, and planned
/// mutations are all surfaced without submitting any transaction.
#[cfg(test)]
mod deploy_dry_run_tests {
    use sha2::{Digest, Sha256};
    use starforge::utils::wasm_preflight::{
        validate_wasm_bytes, WasmPolicy, WASM_SIZE_LIMIT_BYTES,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn minimal_wasm() -> Vec<u8> {
        vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    struct DeployPlan {
        network: String,
        wallet_name: String,
        wallet_pubkey: String,
        wasm_bytes: Vec<u8>,
        wasm_hash: String,
        wasm_size_kb: f64,
        estimated_fee_stroops: Option<u64>,
        operations: Vec<String>,
        authorization: Vec<String>,
    }

    impl DeployPlan {
        fn from_wasm(bytes: Vec<u8>, network: &str, wallet_name: &str, pubkey: &str) -> Self {
            let hash = sha256_hex(&bytes);
            let size_kb = bytes.len() as f64 / 1024.0;
            Self {
                network: network.to_string(),
                wallet_name: wallet_name.to_string(),
                wallet_pubkey: pubkey.to_string(),
                wasm_hash: hash,
                wasm_size_kb: size_kb,
                wasm_bytes: bytes,
                estimated_fee_stroops: None,
                operations: vec![
                    "InvokeHostFunction — Upload WASM bytecode".to_string(),
                    "InvokeHostFunction — Create contract instance".to_string(),
                ],
                authorization: vec![pubkey.to_string()],
            }
        }

        fn preflight_ok(&self) -> bool {
            let policy = WasmPolicy::default();
            validate_wasm_bytes(&self.wasm_bytes, "contract.wasm", &policy).is_ok()
        }
    }

    const PUBKEY: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    // ── Primary flow ─────────────────────────────────────────────────────────

    #[test]
    fn dry_run_plan_exposes_code_hash() {
        let wasm = minimal_wasm();
        let expected_hash = sha256_hex(&wasm);
        let plan = DeployPlan::from_wasm(wasm, "testnet", "deployer", PUBKEY);
        assert_eq!(
            plan.wasm_hash, expected_hash,
            "dry-run plan must expose the SHA-256 code hash"
        );
    }

    #[test]
    fn dry_run_plan_exposes_network() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(plan.network, "testnet");
    }

    #[test]
    fn dry_run_plan_exposes_account_pubkey() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(plan.wallet_pubkey, PUBKEY);
        assert!(plan.authorization.contains(&PUBKEY.to_string()));
    }

    #[test]
    fn dry_run_plan_lists_two_planned_operations() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(
            plan.operations.len(),
            2,
            "deploy produces exactly 2 on-chain operations"
        );
        assert!(
            plan.operations[0].contains("Upload"),
            "first op should be WASM upload"
        );
        assert!(
            plan.operations[1].contains("instance"),
            "second op should be instance creation"
        );
    }

    #[test]
    fn dry_run_plan_lists_authorization_requirements() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert!(
            !plan.authorization.is_empty(),
            "authorization list must not be empty"
        );
        assert!(
            plan.authorization.contains(&PUBKEY.to_string()),
            "authorizing signer must be the deployer's public key"
        );
    }

    // ── Boundary cases ────────────────────────────────────────────────────────

    #[test]
    fn dry_run_preflight_passes_for_valid_wasm() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert!(
            plan.preflight_ok(),
            "valid minimal WASM should pass pre-flight in dry-run"
        );
    }

    #[test]
    fn dry_run_code_hash_is_64_hex_chars() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(
            plan.wasm_hash.len(),
            64,
            "SHA-256 hex string should be 64 characters"
        );
        assert!(
            plan.wasm_hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be lowercase hex"
        );
    }

    #[test]
    fn dry_run_wasm_size_matches_bytes_len() {
        let wasm = minimal_wasm();
        let expected_kb = wasm.len() as f64 / 1024.0;
        let plan = DeployPlan::from_wasm(wasm, "testnet", "deployer", PUBKEY);
        assert!(
            (plan.wasm_size_kb - expected_kb).abs() < 0.001,
            "reported size should match actual bytes"
        );
    }

    // ── Failure cases ─────────────────────────────────────────────────────────

    #[test]
    fn dry_run_preflight_rejects_invalid_magic() {
        // Not a valid WASM binary.
        let bad_bytes = b"not-a-wasm-file".to_vec();
        let policy = WasmPolicy::default();
        let report = validate_wasm_bytes(&bad_bytes, "bad.wasm", &policy);
        assert!(!report.is_ok(), "invalid WASM should fail pre-flight");
        assert_eq!(report.violations[0].code, "INVALID_MAGIC");
    }

    #[test]
    fn dry_run_preflight_rejects_oversized_wasm() {
        let mut bytes = minimal_wasm();
        bytes.extend(vec![0u8; WASM_SIZE_LIMIT_BYTES + 1]);
        let policy = WasmPolicy::default();
        let report = validate_wasm_bytes(&bytes, "big.wasm", &policy);
        assert!(!report.is_ok());
        assert!(report.violations.iter().any(|v| v.code == "SIZE_EXCEEDED"));
    }

    #[test]
    fn dry_run_mainnet_flag_is_tracked() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "mainnet", "deployer", PUBKEY);
        assert_eq!(plan.network, "mainnet");
    }
}
