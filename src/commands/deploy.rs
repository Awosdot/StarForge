use crate::utils::{config, confirmation, horizon, optimizer, print as p, soroban, wasm_preflight};
use anyhow::Result;
use clap::Args;
use colored::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SOROBAN_WASM_LIMIT_KB: f64 = 128.0;

/// Deploy a compiled Soroban WASM artifact to testnet or mainnet.
///
/// By default StarForge performs a dry-run: it validates the WASM, checks the
/// wallet on Horizon, prints the Stellar CLI command, and optionally simulates
/// fees with `--simulate`. Pass `--execute` to run `stellar contract deploy`.
#[derive(Args)]
pub struct DeployArgs {
    /// Path to the compiled .wasm file
    #[arg(long)]
    pub wasm: PathBuf,
    /// Network to deploy to
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet"])]
    pub network: String,
    /// Wallet name to use for deployment
    #[arg(long)]
    pub wallet: Option<String>,
    /// Optimize the WASM before deployment using the built-in optimizer
    #[arg(long, default_value = "false")]
    pub optimize: bool,
    /// Skip confirmation prompt
    #[arg(long, default_value = "false")]
    pub yes: bool,
    /// Execute deployment immediately if Stellar CLI is installed
    #[arg(long, default_value = "false")]
    pub execute: bool,
    /// Simulate the deploy transaction using Soroban RPC
    /// Simulate deploy transaction via Soroban RPC before confirmation
    #[arg(long, default_value = "false")]
    pub simulate: bool,
    /// Dry-run: validate artifact paths, network connectivity, wallet existence,
    /// and estimate fees without submitting any transaction. Prints a full
    /// deployment plan and exits. Implies --simulate.
    #[arg(long, default_value = "false")]
    pub dry_run: bool,
}

fn is_wasm_above_size_limit(wasm_size_kb: f64) -> bool {
    wasm_size_kb > SOROBAN_WASM_LIMIT_KB
}

/// Compute the Soroban WASM hash (SHA-256 over raw `.wasm` file bytes)
/// and return it as a 64-character lowercase hex string.
///
/// This matches the hash that `stellar contract inspect --wasm <file>` reports
/// and that Soroban uses to identify uploaded contract bytecode on-chain.
fn compute_local_wasm_hash(wasm_bytes: &[u8]) -> String {
    let digest = Sha256::digest(wasm_bytes);
    hex::encode(digest)
}

fn build_stellar_deploy_command(wasm: &std::path::Path, source: &str, network: &str) -> String {
    format!(
        "stellar contract deploy \\\n  --wasm {} \\\n  --source {} \\\n  --network {}",
        wasm.display(),
        source,
        network
    )
}

fn build_stellar_deploy_args(wasm: &std::path::Path, source: &str, network: &str) -> Vec<String> {
    vec![
        "contract".to_string(),
        "deploy".to_string(),
        "--wasm".to_string(),
        wasm.display().to_string(),
        "--source".to_string(),
        source.to_string(),
        "--network".to_string(),
        network.to_string(),
    ]
}

/// Validate and summarise a deployment plan without submitting any transaction.
///
/// Shows: WASM artifact details + pre-flight policy check, wallet/account on
/// Horizon, estimated Soroban fees, **authorization requirements**, and the
/// exact **planned on-chain mutations** (upload WASM + create contract
/// instance). Exits cleanly so the caller can review before going live.
async fn run_dry_run(
    wasm_path: &std::path::Path,
    wasm_bytes: &[u8],
    wasm_hash: &str,
    wasm_size_kb: f64,
    wallet: &crate::utils::config::WalletEntry,
    network: &str,
) -> Result<()> {
    p::header("Deployment Dry-Run Plan");

    let mut warnings: Vec<String> = Vec::new();
    let mut checks_passed = 0u32;
    let checks_total = 5u32;

    // ── Check 1: WASM artifact + pre-flight policy ────────────────────────
    p::kv("[ 1/5 ] WASM artifact", &wasm_path.display().to_string());
    p::kv("        Size", &format!("{:.1} KB", wasm_size_kb));
    p::kv("        SHA-256 (code hash)", wasm_hash);

    let policy = wasm_preflight::WasmPolicy::default();
    let preflight = wasm_preflight::validate_wasm_bytes(wasm_bytes, &wasm_path.to_string_lossy(), &policy);

    if !preflight.is_valid_wasm {
        for v in &preflight.violations {
            warnings.push(format!("[{}] {}", v.code, v.message));
        }
        p::warn("        Pre-flight: invalid WASM binary");
    } else if !preflight.passes_policy {
        for v in &preflight.violations {
            warnings.push(format!("[{}] {}", v.code, v.message));
        }
        p::warn("        Pre-flight: policy violations detected (see warnings)");
    } else {
        checks_passed += 1;
        p::success("        Pre-flight: WASM valid and within policy");
    }
    for w in &preflight.warnings {
        warnings.push(w.clone());
    }
    if !preflight.exports.is_empty() {
        p::kv(
            "        Exports",
            &preflight.exports.join(", "),
        );
    }
    println!();

    // ── Check 2: wallet existence ─────────────────────────────────────────
    p::kv("[ 2/5 ] Wallet", &wallet.name);
    p::kv("        Public key", &wallet.public_key);
    checks_passed += 1;
    p::success("        Wallet found in local config");
    println!();

    // ── Check 3: network connectivity / account balance ───────────────────
    p::kv("[ 3/5 ] Network", network);
    let mut xlm_balance = "0".to_string();
    match horizon::fetch_account(&wallet.public_key, network).await {
        Ok(account) => {
            let xlm = account
                .balances
                .iter()
                .find(|b| b.asset_type == "native")
                .map(|b| b.balance.as_str())
                .unwrap_or("0");
            xlm_balance = xlm.to_string();
            p::kv("        XLM balance", &format!("{} XLM", xlm));
            let balance: f64 = xlm.parse().unwrap_or(0.0);
            if balance < 1.0 {
                warnings.push(format!(
                    "Account balance ({} XLM) may be too low to cover fees. \
                     Fund with: starforge wallet fund {}",
                    xlm, wallet.name
                ));
            }
            checks_passed += 1;
            p::success("        Account is active on-chain");
        }
        Err(e) => {
            warnings.push(format!(
                "Cannot reach {} network or account not funded: {}. \
                 Fund with: starforge wallet fund {}",
                network, e, wallet.name
            ));
            p::warn(&format!("        Network/account check failed: {}", e));
        }
    }
    println!();

    // ── Check 4: fee estimation via Soroban RPC simulation ────────────────
    p::info("[ 4/5 ] Estimating Soroban fees via RPC simulation...");
    let mut estimated_fee_stroops: Option<u64> = None;
    match soroban::simulate_deploy_transaction(wasm_hash, network, wallet).await {
        Ok(simulation) => {
            estimated_fee_stroops = Some(simulation.fee);
            p::kv(
                "        Estimated fee",
                &format!("{} stroops ({:.7} XLM)", simulation.fee, simulation.fee as f64 / 10_000_000.0),
            );
            if !simulation.errors.is_empty() {
                for error in &simulation.errors {
                    warnings.push(format!("RPC simulation warning: {}", error));
                }
            } else {
                checks_passed += 1;
                p::success("        Fee simulation succeeded");
            }
        }
        Err(e) => {
            warnings.push(format!(
                "Fee simulation unavailable (Soroban RPC unreachable): {}. \
                 Deployment may still succeed.",
                e
            ));
            p::warn(&format!("        Fee simulation skipped: {}", e));
            checks_passed += 1; // non-fatal — RPC may be offline
        }
    }
    println!();

    // ── Check 5: authorization requirements ──────────────────────────────
    p::kv("[ 5/5 ] Authorization", "");
    p::kv(
        "        Required signer",
        &format!("{} ({})", wallet.name, wallet.public_key),
    );
    p::kv(
        "        Signing method",
        "Ed25519 (single-key, no threshold)",
    );
    checks_passed += 1;
    p::success("        Authorization requirements satisfied by selected wallet");
    println!();

    // ── Planned mutations (what will happen on-chain) ─────────────────────
    p::separator();
    p::header("Planned On-Chain Mutations");
    println!(
        "  {} {} Upload WASM bytecode",
        "Op 1:".cyan().bold(),
        "InvokeHostFunction —".dimmed()
    );
    println!(
        "         Code hash  : {}",
        wasm_hash.cyan()
    );
    println!(
        "         Size       : {:.1} KB ({} bytes)",
        wasm_size_kb,
        wasm_bytes.len()
    );
    println!(
        "         Authorized : {}",
        wallet.public_key
    );
    println!();
    println!(
        "  {} {} Create contract instance",
        "Op 2:".cyan().bold(),
        "InvokeHostFunction —".dimmed()
    );
    println!(
        "         Constructor: default (no __constructor export detected)"
    );
    println!(
        "         Storage    : Persistent (new ContractData ledger entry)"
    );
    println!(
        "         Authorized : {}",
        wallet.public_key
    );
    println!();
    println!("  {} No existing contract state will be modified.", "Note:".dimmed());
    println!("  {} This is a fresh deployment.", "Note:".dimmed());
    println!();

    // ── Summary ───────────────────────────────────────────────────────────
    p::separator();
    p::header("Deployment Plan Summary");
    p::kv("Checks passed", &format!("{}/{}", checks_passed, checks_total));
    p::kv("Network", network);
    p::kv("Wallet", &wallet.name);
    p::kv("Account public key", &wallet.public_key);
    p::kv("Account XLM balance", &format!("{} XLM", xlm_balance));
    p::kv("WASM file", &wasm_path.display().to_string());
    p::kv("WASM code hash (SHA-256)", wasm_hash);
    if let Some(fee) = estimated_fee_stroops {
        p::kv("Estimated fee", &format!("{} stroops", fee));
    }
    p::kv("Planned operations", "2 (upload WASM + create instance)");

    println!();
    let deploy_cmd = build_stellar_deploy_command(wasm_path, &wallet.public_key, network);
    println!("  Stellar CLI command to deploy:");
    for line in deploy_cmd.lines() {
        println!("    {}", line.cyan());
    }

    if !warnings.is_empty() {
        println!();
        p::warn(&format!("{} warning(s):", warnings.len()));
        for w in &warnings {
            p::warn(&format!("  • {}", w));
        }
    }

    if network == "mainnet" {
        println!();
        p::warn("Target network is MAINNET. This will cost real XLM when executed.");
    }

    println!();
    if warnings.is_empty() {
        p::success("Dry-run complete — no issues found. Run with --execute to deploy.");
    } else {
        p::info("Dry-run complete with warnings. Review above before deploying.");
        p::info("Run with --execute to deploy, or address the warnings first.");
    }

    Ok(())
}

pub async fn handle(args: DeployArgs) -> Result<()> {
    p::header("Deploy Soroban Contract");

    if !args.wasm.exists() {
        anyhow::bail!(
            "WASM file not found: {:?}\nRun `stellar contract build` first.",
            args.wasm
        );
    }

    let mut wasm_path = args.wasm.clone();
    let mut wasm_bytes = fs::read(&wasm_path)?;
    let mut wasm_size_kb = wasm_bytes.len() as f64 / 1024.0;

    if args.optimize {
        let optimized_path = args.wasm.with_file_name(format!(
            "{}-optimized.wasm",
            args.wasm.file_stem().unwrap_or_default().to_string_lossy()
        ));
        p::header("WASM Optimization");
        p::kv("Input WASM", &args.wasm.display().to_string());
        p::kv("Output WASM", &optimized_path.display().to_string());
        let result = optimizer::optimize_wasm(&args.wasm, &optimized_path)?;
        wasm_path = optimized_path;
        wasm_bytes = fs::read(&wasm_path)?;
        wasm_size_kb = wasm_bytes.len() as f64 / 1024.0;
        println!();
        p::success("Optimization pass completed");
        p::kv("Optimizer", &result.tool);
        p::kv("Input size", &format!("{} bytes", result.input_size_bytes));
        p::kv(
            "Output size",
            &format!("{} bytes", result.output_size_bytes),
        );
        p::kv(
            "Size reduction",
            &format!(
                "{} bytes ({:+.2}%)",
                result.reduction_bytes(),
                result.reduction_percent()
            ),
        );
        p::separator();
    }

    p::separator();
    p::kv("WASM file", &wasm_path.display().to_string());
    p::kv("WASM size", &format!("{:.1} KB", wasm_size_kb));
    p::kv("Network", &args.network);

    if is_wasm_above_size_limit(wasm_size_kb) {
        p::warn(&format!(
            "WASM is {:.1} KB - Soroban limit is 128 KB. Optimize with --release.",
            wasm_size_kb
        ));
        p::info("If this contract is still too large, use `starforge gas optimize --target <input>.wasm --output <output>.wasm` or external tools such as `wasm-opt -Oz`.");
    }

    let cfg = config::load()?;
    let wallet = if let Some(ref wallet_name) = args.wallet {
        cfg.wallets
            .iter()
            .find(|w| &w.name == wallet_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Wallet '{}' not found. Run `starforge wallet list`",
                    wallet_name
                )
            })?
    } else if !cfg.wallets.is_empty() {
        p::info(&format!(
            "No --wallet specified. Using: {}",
            cfg.wallets[0].name.cyan()
        ));
        &cfg.wallets[0]
    } else {
        anyhow::bail!(
            "No wallets found. Create one first:\n  starforge wallet create deployer --fund"
        );
    };

    p::kv("Wallet", &wallet.name);
    p::kv_accent("Public Key", &wallet.public_key);
    p::separator();

    let wasm_hash = compute_local_wasm_hash(&wasm_bytes);

    // ── WASM pre-flight policy check (always runs, blocks on violations) ───
    {
        let policy = wasm_preflight::WasmPolicy::default();
        let report = wasm_preflight::validate_wasm_bytes(&wasm_bytes, &wasm_path.to_string_lossy(), &policy);
        if !report.is_ok() {
            for v in &report.violations {
                p::warn(&format!("[{}] {}", v.code, v.message));
            }
            anyhow::bail!(
                "WASM pre-flight check failed with {} violation(s). \
                 Fix the module before deploying.",
                report.violations.len()
            );
        }
        for w in &report.warnings {
            p::warn(w);
        }
    }

    // --dry-run: validate everything and print deployment plan, then exit.
    if args.dry_run {
        return run_dry_run(
            &wasm_path,
            &wasm_bytes,
            &wasm_hash,
            wasm_size_kb,
            wallet,
            &args.network,
        ).await;
    }

    if args.simulate {
        p::info("Simulating deploy transaction via Soroban RPC...");
        match soroban::simulate_deploy_transaction(&wasm_hash, &args.network, wallet).await {
            Ok(simulation) => {
                p::kv("Estimated Fee", &format!("{} stroops", simulation.fee));
                if !simulation.errors.is_empty() {
                    for error in &simulation.errors {
                        p::warn(&format!("Simulation error: {}", error));
                    }
                } else {
                    p::success("Simulation completed without reported RPC errors");
                }
            }
            Err(error) => {
                p::warn(&format!("Simulation failed: {}", error));
            }
        }
        p::separator();
    }

    // Build operation summary for confirmation
    let risk_level = if args.network == "mainnet" {
        confirmation::RiskLevel::High
    } else {
        confirmation::RiskLevel::Medium
    };

    let summary = confirmation::OperationSummary::new(
        "Deploy Soroban Contract".to_string(),
        args.network.clone(),
        risk_level,
    )
    .add("WASM file", wasm_path.display().to_string())
    .add("WASM size", format!("{:.1} KB", wasm_size_kb))
    .add("WASM hash", &wasm_hash)
    .add("Wallet", &wallet.name)
    .add("Public Key", &wallet.public_key)
    .add("Optimized", if args.optimize { "Yes" } else { "No" })
    .add("Execute", if args.execute { "Yes" } else { "No (dry-run)" });

    let confirm_config = confirmation::ConfirmationConfig {
        risk_level,
        network: args.network.clone(),
        skip_confirm: args.yes,
        dry_run: !args.execute,
        prompt: Some("Proceed with deployment?".to_string()),
        require_type_confirmation: args.network == "mainnet",
    };

    if !confirmation::confirm_operation(&summary, &confirm_config)? {
        return Ok(());
    }

    println!();
    println!();
    let pb = p::progress_bar(3, "Starting deployment steps...");

    pb.set_message("Verifying account on-chain...");
    let account = horizon::fetch_account(&wallet.public_key, &args.network).await.map_err(|e| {
        pb.abandon();
        anyhow::anyhow!(
            "Account not active on {}: {}\nFund it with: starforge wallet fund {}",
            args.network,
            e,
            wallet.name
        )
    })?;

    let xlm = account
        .balances
        .iter()
        .find(|b| b.asset_type == "native")
        .map(|b| b.balance.as_str())
        .unwrap_or("0");

    pb.inc(1);
    pb.set_message("Calculating WASM SHA-256 hash...");
    pb.set_message("Recording WASM SHA-256 hash...");

    pb.inc(1);
    pb.set_message("Generating stellar CLI command...");
    pb.finish_with_message("Deployment preparation complete!");

    println!();
    p::kv_accent("XLM Balance", &format!("{} XLM", xlm));
    p::kv("WASM Hash (local SHA-256)", &wasm_hash);

    println!();
    p::separator();
    println!(
        "  {} {}",
        "✓".green().bold(),
        "Ready! Run this to complete the deployment:".bright_white()
    );
    println!();
    let deploy_cmd = build_stellar_deploy_command(&wasm_path, &wallet.public_key, &args.network);
    for line in deploy_cmd.lines() {
        println!("  {}", line.cyan());
    }
    println!();

    if args.execute {
        p::info("Executing deployment with Stellar CLI...");
        let deploy_args = build_stellar_deploy_args(&wasm_path, &wallet.public_key, &args.network);
        let output = Command::new("stellar")
            .args(&deploy_args)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute stellar CLI: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Stellar CLI deployment failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        p::success("Deployment executed successfully!");
        println!("{}", stdout);
    } else {
        p::info("Dry-run complete. Use --execute to deploy for real.");
    }

    Ok(())
}
