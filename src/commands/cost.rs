//! `starforge cost` — AI-assisted deployment cost management: budgets,
//! forecasting, cross-network comparison, and aggregate reporting.
//!
//! Builds on top of `starforge gas estimate`'s cost-history store; run that
//! command first (or with `--save`, the default) to build up the history
//! this command reports, forecasts, and enforces budgets against.

use crate::utils::{config, cost_estimation as ce, cost_management as cm, print as p};
use anyhow::Result;
use clap::Subcommand;
use colored::*;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum CostCommands {
    /// Manage recurring spending budgets per network
    Budget {
        #[command(subcommand)]
        action: BudgetAction,
    },
    /// Estimate a wasm's deployment cost and check it against configured
    /// budgets for that network (budget enforcement)
    Check {
        /// Path to the compiled wasm
        wasm: PathBuf,
        /// Target network
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Exit with a non-zero status if any budget would be exceeded
        /// (suitable for gating CI/CD deploy pipelines)
        #[arg(long)]
        enforce: bool,
    },
    /// Project future deployment costs for a network from historical trend
    Forecast {
        /// Network to forecast
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Number of future deployments to project
        #[arg(long, default_value = "3")]
        periods: usize,
    },
    /// Compare estimated deployment cost for the same wasm across networks
    CompareNetworks {
        /// Path to the compiled wasm
        wasm: PathBuf,
        /// Comma-separated list of networks to compare
        #[arg(long, default_value = "testnet,mainnet,futurenet")]
        networks: String,
    },
    /// Aggregate cost report: totals, averages, cost-driver breakdown, and
    /// the most common optimization opportunities across deployment history
    Report {
        /// Filter to a single network (omit for all networks)
        #[arg(long)]
        network: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum BudgetAction {
    /// Set (or replace) the budget for a network
    Set {
        /// Network this budget applies to
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Spending cap in XLM per period
        #[arg(long)]
        amount: f64,
        /// Reset period: daily, weekly, or monthly
        #[arg(long, default_value = "monthly")]
        period: String,
        /// Optional human-readable label
        #[arg(long)]
        label: Option<String>,
    },
    /// List all configured budgets and their current status
    List,
    /// Show spend-to-date vs. limit for one or all budgets
    Status {
        /// Filter to a single network (omit for all)
        #[arg(long)]
        network: Option<String>,
    },
    /// Remove the budget configured for a network
    Remove {
        /// Network whose budget to remove
        #[arg(long)]
        network: String,
    },
}

pub async fn handle(cmd: CostCommands) -> Result<()> {
    match cmd {
        CostCommands::Budget { action } => budget(action),
        CostCommands::Check {
            wasm,
            network,
            enforce,
        } => check(wasm, network, enforce),
        CostCommands::Forecast { network, periods } => forecast(network, periods),
        CostCommands::CompareNetworks { wasm, networks } => compare_networks(wasm, networks),
        CostCommands::Report { network } => report(network),
    }
}

fn budget(action: BudgetAction) -> Result<()> {
    match action {
        BudgetAction::Set {
            network,
            amount,
            period,
            label,
        } => {
            config::validate_network(&network)?;
            let period = cm::BudgetPeriod::parse(&period)?;
            let budget = cm::set_budget(&network, period, amount, label)?;

            p::header("Cost Budget — Set");
            p::success(&format!(
                "Budget set for '{}': {:.7} XLM / {}",
                budget.network, budget.limit_xlm, budget.period
            ));
            if let Some(label) = &budget.label {
                p::kv("Label", label);
            }
            Ok(())
        }
        BudgetAction::List => {
            let budgets = cm::load_budgets()?;
            p::header("Cost Budgets");
            if budgets.is_empty() {
                p::info("No budgets configured. Set one with: starforge cost budget set --network <net> --amount <xlm>");
                return Ok(());
            }
            let headers = &["Network", "Period", "Limit (XLM)", "Label"];
            let rows: Vec<Vec<String>> = budgets
                .iter()
                .map(|b| {
                    vec![
                        b.network.clone(),
                        b.period.to_string(),
                        format!("{:.7}", b.limit_xlm),
                        b.label.clone().unwrap_or_else(|| "—".to_string()),
                    ]
                })
                .collect();
            p::table(headers, &rows);
            Ok(())
        }
        BudgetAction::Status { network } => {
            let statuses = cm::budget_status(network.as_deref())?;
            p::header("Cost Budget Status");
            if statuses.is_empty() {
                p::info("No matching budgets configured.");
                return Ok(());
            }
            for status in &statuses {
                println!();
                p::kv_accent("Network", &status.budget.network);
                p::kv("Period", &status.budget.period.to_string());
                p::kv("Limit", &format!("{:.7} XLM", status.budget.limit_xlm));
                p::kv("Spent", &format!("{:.7} XLM", status.spent_xlm));
                p::kv("Remaining", &format!("{:.7} XLM", status.remaining_xlm));
                p::kv("Used", &format!("{:.1}%", status.percent_used));
                p::kv(
                    "Deployments in period",
                    &status.deployments_in_period.to_string(),
                );
                if status.exceeded {
                    p::warn("Budget exceeded for the current period.");
                }
            }
            Ok(())
        }
        BudgetAction::Remove { network } => {
            let removed = cm::remove_budget(&network)?;
            p::header("Cost Budget — Remove");
            if removed {
                p::success(&format!("Budget removed for '{}'", network));
            } else {
                p::info(&format!("No budget was configured for '{}'", network));
            }
            Ok(())
        }
    }
}

fn check(wasm: PathBuf, network: String, enforce: bool) -> Result<()> {
    config::validate_file_path(&wasm, Some("wasm"))?;
    config::validate_network(&network)?;

    p::header("Cost Budget Check");
    p::kv("Wasm", &wasm.display().to_string());
    p::kv("Network", &network);

    let estimate = ce::estimate_deployment_cost(&wasm, &network)?;
    p::kv("Estimated fee", &estimate.fee_xlm_display());

    let results = cm::check_budget(&estimate)?;

    println!();
    if results.is_empty() {
        p::info(&format!(
            "No budget configured for '{}'. Set one with: starforge cost budget set --network {} --amount <xlm>",
            network, network
        ));
        return Ok(());
    }

    let mut any_exceeded = false;
    for result in &results {
        let label = result
            .status
            .budget
            .label
            .clone()
            .unwrap_or_else(|| result.status.budget.network.clone());
        if result.would_exceed {
            any_exceeded = true;
            println!(
                "{} Budget '{}' would be exceeded: {:.7} XLM projected vs {:.7} XLM limit",
                "✗".red().bold(),
                label,
                result.projected_spent_xlm,
                result.status.budget.limit_xlm
            );
        } else {
            println!(
                "{} Budget '{}' OK: {:.7} XLM projected vs {:.7} XLM limit ({:.1}% used)",
                "✓".green(),
                label,
                result.projected_spent_xlm,
                result.status.budget.limit_xlm,
                (result.projected_spent_xlm / result.status.budget.limit_xlm) * 100.0
            );
        }
    }

    if any_exceeded && enforce {
        anyhow::bail!("Deployment blocked: one or more budgets would be exceeded (--enforce)");
    }

    Ok(())
}

fn forecast(network: String, periods: usize) -> Result<()> {
    config::validate_network(&network)?;

    p::header("Cost Forecast");
    p::kv("Network", &network);

    let forecast = cm::forecast_costs(&network, periods)?;

    p::kv("Sample size", &forecast.sample_size.to_string());
    p::kv("Average fee", &format!("{:.7} XLM", forecast.avg_fee_xlm));
    p::kv(
        "Trend",
        &format!(
            "{:+.7} XLM per deployment",
            forecast.trend_xlm_per_deployment
        ),
    );
    p::kv(
        "Confidence",
        &format!("{:?}", forecast.confidence).to_lowercase(),
    );

    println!();
    p::info("Projected costs:");
    for p_cost in &forecast.projected {
        println!(
            "  +{} deployment(s) → {:.7} XLM",
            p_cost.deployment_offset, p_cost.projected_fee_xlm
        );
    }

    if forecast.confidence == cm::ForecastConfidence::Low {
        println!();
        p::warn("Low confidence: fewer than 3 historical deployments for this network.");
    }

    Ok(())
}

fn compare_networks(wasm: PathBuf, networks: String) -> Result<()> {
    config::validate_file_path(&wasm, Some("wasm"))?;

    let network_list: Vec<String> = networks
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    p::header("Network Cost Comparison");
    p::kv("Wasm", &wasm.display().to_string());
    p::kv("Networks", &network_list.join(", "));
    println!();

    let comparisons = cm::compare_networks(&wasm, &network_list)?;

    let headers = &[
        "Network",
        "Multiplier",
        "Adjusted Fee (stroops)",
        "Adjusted Fee (XLM)",
    ];
    let rows: Vec<Vec<String>> = comparisons
        .iter()
        .map(|c| {
            vec![
                c.network.clone(),
                format!("{:.2}x", c.multiplier),
                c.adjusted_total_stroops.to_string(),
                format!("{:.7}", c.adjusted_total_xlm),
            ]
        })
        .collect();
    p::table(headers, &rows);

    if let Some(cheapest) = comparisons.first() {
        println!();
        p::success(&format!(
            "Cheapest: {} ({:.7} XLM)",
            cheapest.network, cheapest.adjusted_total_xlm
        ));
    }

    Ok(())
}

fn report(network: Option<String>) -> Result<()> {
    p::header("Deployment Cost Report");
    if let Some(net) = &network {
        p::kv("Network", net);
    } else {
        p::kv("Network", "all");
    }

    let report = cm::generate_cost_report(network.as_deref())?;

    println!();
    if report.deployment_count == 0 {
        p::info("No cost history recorded yet. Run `starforge gas estimate <wasm> --network <net>` first.");
        return Ok(());
    }

    p::kv("Deployments", &report.deployment_count.to_string());
    p::kv("Total spent", &format!("{:.7} XLM", report.total_spent_xlm));
    p::kv("Average fee", &format!("{:.7} XLM", report.avg_fee_xlm));
    p::kv("Min fee", &format!("{:.7} XLM", report.min_fee_xlm));
    p::kv("Max fee", &format!("{:.7} XLM", report.max_fee_xlm));

    println!();
    p::info("Cost driver breakdown:");
    println!("  Gas:     {:.1}%", report.gas_share_percent);
    println!("  Storage: {:.1}%", report.storage_share_percent);
    println!("  Base:    {:.1}%", report.base_share_percent);

    if !report.top_suggestion_categories.is_empty() {
        println!();
        p::info("Most common optimization opportunities:");
        for (category, count) in report.top_suggestion_categories.iter().take(5) {
            println!("  {} — seen in {} deployment(s)", category, count);
        }
    }

    if let Some(entry) = &report.most_expensive {
        println!();
        p::info("Most expensive deployment:");
        p::kv("Wasm", &entry.estimate.wasm_path);
        p::kv("Network", &entry.estimate.network);
        p::kv("Fee", &entry.estimate.fee_xlm_display());
        p::kv("Date", &entry.estimate.estimated_at);
    }

    Ok(())
}
