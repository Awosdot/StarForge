use crate::utils::{config, invocation_script, print as p, soroban, wallet_signer};
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct InvocationScriptArgs {
    /// YAML or JSON invocation script
    pub file: PathBuf,
    /// Print planned calls without reading wallets or contacting the network
    #[arg(long)]
    pub dry_run: bool,
    /// Network used when a step does not specify one
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet"])]
    pub network: String,
}

pub async fn handle(args: InvocationScriptArgs) -> Result<()> {
    let script = invocation_script::load(&args.file)?;
    let planned = invocation_script::plan(&script, &args.network)?;

    if args.dry_run {
        p::header("Invocation Script Dry Run");
        for call in &planned {
            println!(
                "step {}: {}::{}({}) on {}{}",
                call.step,
                call.contract_id,
                call.function,
                call.args
                    .iter()
                    .map(|arg| format!("{}: {}", arg.arg_type, arg.value))
                    .collect::<Vec<_>>()
                    .join(", "),
                call.network,
                if call.submit {
                    " [would submit]"
                } else {
                    " [simulate]"
                }
            );
        }
        return Ok(());
    }

    let cfg = config::load()?;
    for call in planned {
        config::validate_contract_id(&call.contract_id)?;
        config::validate_network(&call.network)?;
        let args = call
            .args
            .iter()
            .map(|arg| arg.value.clone())
            .collect::<Vec<_>>();
        let arg_types = call
            .args
            .iter()
            .map(|arg| arg.arg_type.clone())
            .collect::<Vec<_>>();
        let wallet = if call.submit {
            let name = call
                .wallet
                .as_ref()
                .context("A submitting step requires a wallet.")?;
            let wallet = cfg
                .wallets
                .iter()
                .find(|wallet| &wallet.name == name)
                .with_context(|| format!("Wallet '{}' not found.", name))?;
            if wallet.secret_key.is_none() {
                anyhow::bail!("Wallet '{}' has no local secret key.", name);
            }
            Some(wallet.clone())
        } else {
            None
        };
        let signing = wallet
            .as_ref()
            .map(|wallet| {
                wallet_signer::SigningRequest::from_options(
                    Some(wallet),
                    None,
                    None,
                    &call.network,
                    false,
                    "invocation script",
                )
            })
            .transpose()?;
        let outcome = soroban::invoke_contract(
            &call.contract_id,
            &call.function,
            &args,
            &arg_types,
            &call.network,
            wallet.as_ref(),
            signing.as_ref(),
        )
        .await
        .with_context(|| format!("Invocation script step {} failed.", call.step))?;
        let result = invocation_script::InvocationResult {
            return_value: outcome.simulation.return_value,
            errors: outcome.simulation.errors,
            events: outcome.simulation.events,
            fee: outcome.simulation.fee,
        };
        for assertion in &call.assertions {
            invocation_script::assert_result(assertion, &result).with_context(|| {
                format!("Invocation script step {} assertion failed.", call.step)
            })?;
        }
        p::success(&format!("Invocation script step {} complete.", call.step));
    }
    Ok(())
}
