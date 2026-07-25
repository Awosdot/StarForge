use crate::utils::{config, print as p};
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current global configuration
    Show,
    /// Set a configuration key/value pair
    Set {
        /// Configuration key to update (e.g. telemetry.enabled)
        key: String,
        /// New value for the configuration key
        value: String,
    },
    /// Set global wallet encryption parameters (Argon2id)
    SetEncryption {
        /// Argon2 memory cost in KiB (e.g. 65536)
        #[arg(long)]
        mem: Option<u32>,
        /// Argon2 iteration count (e.g. 3)
        #[arg(long)]
        iterations: Option<u32>,
        /// Argon2 parallelism factor (e.g. 4)
        #[arg(long)]
        parallelism: Option<u32>,
        /// Reset to library defaults
        #[arg(long, default_value = "false")]
        reset: bool,
    },
}

pub fn handle(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Show => show(),
        ConfigCommands::Set { key, value } => set(&key, &value),
        ConfigCommands::SetEncryption {
            mem,
            iterations,
            parallelism,
            reset,
        } => set_encryption(mem, iterations, parallelism, reset),
    }
}

fn show() -> Result<()> {
    let cfg = config::load()?;
    p::header("StarForge Configuration");
    p::separator();

    p::kv("Config file", &config::config_path().display().to_string());
    p::kv("Active network", &cfg.network);
    p::kv(
        "Telemetry",
        if cfg.telemetry_enabled.unwrap_or(true) {
            "enabled"
        } else {
            "disabled"
        },
    );
    p::kv(
        "telemetry.enabled",
        &cfg.telemetry_enabled.unwrap_or(true).to_string(),
    );

    println!();
    p::header("Wallet Encryption (Argon2id)");
    if let Some(kdf) = &cfg.wallet_encryption {
        p::kv("Memory cost", &format!("{} KiB", kdf.mem.unwrap_or(32768)));
        p::kv("Iterations", &kdf.iterations.unwrap_or(3).to_string());
        p::kv("Parallelism", &kdf.parallelism.unwrap_or(1).to_string());
    } else {
        p::info("Using default Argon2id parameters:");
        p::kv("Memory cost", "32768 KiB (default)");
        p::kv("Iterations", "3 (default)");
        p::kv("Parallelism", "1 (default)");
    }

    p::separator();
    Ok(())
}

fn set_encryption(
    mem: Option<u32>,
    iterations: Option<u32>,
    parallelism: Option<u32>,
    reset: bool,
) -> Result<()> {
    let mut cfg = config::load()?;

    if reset {
        cfg.wallet_encryption = None;
        config::save(&cfg)?;
        p::success("Wallet encryption parameters reset to defaults.");
        return Ok(());
    }

    if mem.is_none() && iterations.is_none() && parallelism.is_none() {
        anyhow::bail!("Provide at least one parameter to set (e.g. --mem 65536) or use --reset");
    }

    let mut kdf = cfg.wallet_encryption.unwrap_or_default();
    if let Some(m) = mem {
        kdf.mem = Some(m);
    }
    if let Some(i) = iterations {
        kdf.iterations = Some(i);
    }
    if let Some(p) = parallelism {
        kdf.parallelism = Some(p);
    }

    cfg.wallet_encryption = Some(kdf);
    config::save(&cfg)?;

    p::success("Global wallet encryption parameters updated.");
    show()?;
    Ok(())
}

fn set(key: &str, value: &str) -> Result<()> {
    let mut cfg = config::load()?;

    let normalized = key.to_lowercase();
    let enabled = match value.to_lowercase().as_str() {
        "true" | "1" | "on" | "yes" => true,
        "false" | "0" | "off" | "no" => false,
        _ => anyhow::bail!("Invalid boolean value '{}'. Use true or false.", value),
    };

    match normalized.as_str() {
        "telemetry" | "telemetry.enabled" => {
            cfg.telemetry_enabled = Some(enabled);
            config::save(&cfg)?;
            p::success(&format!("'{}' set to '{}'.", key, enabled));
            Ok(())
        }
        _ => anyhow::bail!(
            "Unsupported config key '{}'. Supported keys: telemetry.enabled",
            key
        ),
    }
}
