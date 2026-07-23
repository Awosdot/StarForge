//! `starforge ai-debug` — AI Contract Debugging Assistant.
//!
//! Analyses Soroban contract errors, stack traces, and variable state to
//! provide clear explanations, root-cause analysis, bug identification,
//! fix suggestions, and guided reproduction steps.
//!
//! ## Sub-commands
//! - `analyse`   — Analyse an error message and/or stack trace
//! - `explain`   — Explain a specific error code or category
//! - `inspect`   — Inspect variable state for suspicious values
//! - `test`      — Analyse test failure output and suggest fixes

use crate::utils::ai_debugger::{self, Severity};
use crate::utils::print as p;
use anyhow::Result;
use clap::{Args, Subcommand};
use colored::*;
use std::fs;
use std::path::PathBuf;

// ── Sub-command enum ──────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum AiDebugCommands {
    /// Analyse a contract error message, with optional stack trace and variables
    Analyse(AnalyseArgs),
    /// Explain a known error category (auth, arithmetic, storage, token, wasm, ttl, type)
    Explain(ExplainArgs),
    /// Inspect variable state for potential bugs (pass name=value pairs)
    Inspect(InspectArgs),
    /// Analyse test failure output and suggest fixes
    Test(TestArgs),
}

// ── Analyse sub-command ───────────────────────────────────────────────────────

#[derive(Args)]
pub struct AnalyseArgs {
    /// The error message to analyse (quote the full message)
    pub error: String,

    /// Raw stack trace string (optional; use quotes or --stack-trace-file)
    #[arg(long)]
    pub stack_trace: Option<String>,

    /// Path to a file containing the stack trace
    #[arg(long)]
    pub stack_trace_file: Option<PathBuf>,

    /// Variable name=value pairs for state inspection (e.g. amount=0 caller=None)
    #[arg(long = "var", value_name = "NAME=VALUE", num_args = 1..)]
    pub variables: Vec<String>,

    /// Output format: text | json
    #[arg(long, default_value = "text", value_parser = ["text", "json"])]
    pub format: String,
}

// ── Explain sub-command ───────────────────────────────────────────────────────

#[derive(Args)]
pub struct ExplainArgs {
    /// Category to explain: auth | arithmetic | storage | token | panic | wasm | network | ttl | test | type
    pub category: String,
}

// ── Inspect sub-command ───────────────────────────────────────────────────────

#[derive(Args)]
pub struct InspectArgs {
    /// Variable name=value pairs to inspect (e.g. amount=0 balance=9999)
    #[arg(required = true, value_name = "NAME=VALUE", num_args = 1..)]
    pub variables: Vec<String>,
}

// ── Test sub-command ──────────────────────────────────────────────────────────

#[derive(Args)]
pub struct TestArgs {
    /// The test failure output to analyse (quote the full output)
    pub output: Option<String>,

    /// Path to a file containing test output (alternative to inline output)
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Also provide the originating error message for combined analysis
    #[arg(long)]
    pub error: Option<String>,
}

// ── Top-level handler ─────────────────────────────────────────────────────────

pub async fn handle(cmd: AiDebugCommands) -> Result<()> {
    match cmd {
        AiDebugCommands::Analyse(args) => handle_analyse(args).await,
        AiDebugCommands::Explain(args) => handle_explain(args).await,
        AiDebugCommands::Inspect(args) => handle_inspect(args).await,
        AiDebugCommands::Test(args) => handle_test(args).await,
    }
}

// ── analyse handler ───────────────────────────────────────────────────────────

async fn handle_analyse(args: AnalyseArgs) -> Result<()> {
    // Resolve stack trace from inline string or file
    let stack_trace_owned: Option<String> = if let Some(file) = args.stack_trace_file {
        Some(fs::read_to_string(&file).map_err(|e| {
            anyhow::anyhow!("Could not read stack trace file {}: {}", file.display(), e)
        })?)
    } else {
        args.stack_trace
    };

    // Parse name=value variable pairs
    let variables = parse_variables(&args.variables)?;
    let vars_ref: Vec<(String, String)> = variables;

    let report = ai_debugger::analyse(
        &args.error,
        stack_trace_owned.as_deref(),
        if vars_ref.is_empty() { None } else { Some(&vars_ref) },
        None,
    );

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    print_report(&report);
    Ok(())
}

// ── explain handler ───────────────────────────────────────────────────────────

async fn handle_explain(args: ExplainArgs) -> Result<()> {
    // Map category to a synthetic error message that will trigger the right pattern
    let synthetic_error = match args.category.to_lowercase().as_str() {
        "auth" | "authorization" => "require_auth failed",
        "arithmetic" | "overflow" | "underflow" => "attempt to add with overflow",
        "storage" | "store" => "storage key not found",
        "token" | "balance" => "insufficient balance for transfer",
        "panic" => "called `option::unwrap` on a `none` value",
        "wasm" | "binary" => "invalid wasm binary",
        "network" | "contract" => "contract not found on network",
        "ttl" | "archival" => "entry expired ttl elapsed",
        "test" | "assert" => "assertion failed left right",
        "type" | "abi" | "xdr" => "xdr type conversion mismatch",
        other => anyhow::bail!(
            "Unknown category '{}'. Valid categories: auth, arithmetic, storage, token, panic, wasm, network, ttl, test, type",
            other
        ),
    };

    let report = ai_debugger::analyse(synthetic_error, None, None, None);

    p::header("AI Debugger — Category Explanation");
    p::kv("Category", &args.category);
    p::separator();

    if report.findings.is_empty() {
        p::warn("No detailed explanation available for this category.");
        return Ok(());
    }

    for finding in &report.findings {
        print_finding(finding, true);
    }
    Ok(())
}

// ── inspect handler ───────────────────────────────────────────────────────────

async fn handle_inspect(args: InspectArgs) -> Result<()> {
    let variables = parse_variables(&args.variables)?;

    p::header("AI Debugger — Variable State Inspection");
    p::separator();

    let insights = ai_debugger::inspect_variable_state(&variables);
    for insight in &insights {
        println!("  {}", insight.bright_white());
    }
    println!();
    Ok(())
}

// ── test handler ─────────────────────────────────────────────────────────────

async fn handle_test(args: TestArgs) -> Result<()> {
    // Resolve test output from inline or file
    let output: String = if let Some(file) = args.file {
        fs::read_to_string(&file)
            .map_err(|e| anyhow::anyhow!("Could not read file {}: {}", file.display(), e))?
    } else if let Some(out) = args.output {
        out
    } else {
        anyhow::bail!("Provide test output inline or via --file <path>");
    };

    let error_msg = args.error.as_deref().unwrap_or("test failure");

    let report = ai_debugger::analyse(error_msg, None, None, Some(&output));

    p::header("AI Debugger — Test Failure Analysis");
    p::separator();

    if let Some(ref analysis) = report.test_failure_analysis {
        println!("\n  {}", "Test Analysis:".yellow().bold());
        println!("  {}\n", analysis.bright_white());
    }

    if !report.findings.is_empty() {
        println!("  {}", "Related Findings:".yellow().bold());
        for finding in &report.findings {
            print_finding(finding, false);
        }
    }

    println!("  {}", "Guidance:".yellow().bold());
    println!("  {}\n", report.overall_guidance.bright_white());
    Ok(())
}

// ── Display helpers ───────────────────────────────────────────────────────────

fn print_report(report: &ai_debugger::DebugReport) {
    p::header("AI Contract Debugging Assistant");
    p::kv("Input", &report.input_summary);
    p::separator();

    if report.findings.is_empty() {
        p::warn("No specific issue pattern matched. See general guidance below.");
    } else {
        println!(
            "\n  {} {}\n",
            "Findings:".yellow().bold(),
            format!("({})", report.findings.len()).dimmed()
        );
        for finding in &report.findings {
            print_finding(finding, true);
        }
    }

    // Variable insights
    if !report.variable_insights.is_empty() {
        println!("  {}", "Variable State Insights:".yellow().bold());
        for insight in &report.variable_insights {
            println!("    {}", insight.bright_white());
        }
        println!();
    }

    // Suggested breakpoints
    if !report.suggested_breakpoints.is_empty() {
        println!("  {}", "Suggested Breakpoints:".cyan().bold());
        for bp in &report.suggested_breakpoints {
            println!("    {} {}", "→".cyan(), bp);
        }
        println!();
    }

    // Overall guidance
    println!("  {}", "Guidance:".yellow().bold());
    println!("  {}\n", report.overall_guidance.bright_white());
}

fn print_finding(finding: &ai_debugger::DebugFinding, verbose: bool) {
    let sev_color = match finding.severity {
        Severity::Critical => finding.severity.label().red().bold(),
        Severity::High => finding.severity.label().yellow().bold(),
        Severity::Medium => finding.severity.label().bright_yellow().bold(),
        Severity::Low => finding.severity.label().cyan().bold(),
        Severity::Info => finding.severity.label().white().bold(),
    };

    println!(
        "  [{}] {} — {}",
        sev_color,
        finding.id.bright_white().bold(),
        finding.title.bright_white()
    );
    println!("    {} {}", "Category:".dimmed(), finding.category);
    println!();

    println!("    {}", "Explanation:".bright_white().underline());
    wrap_print(&finding.explanation, 80, "    ");
    println!();

    println!("    {}", "Root Cause:".bright_white().underline());
    wrap_print(&finding.root_cause, 80, "    ");
    println!();

    println!("    {}", "Fix Suggestion:".green().underline());
    wrap_print(&finding.fix_suggestion, 80, "    ");
    println!();

    if verbose {
        if !finding.reproduction_steps.is_empty() {
            println!("    {}", "Reproduction Steps:".bright_white().underline());
            for (i, step) in finding.reproduction_steps.iter().enumerate() {
                println!("      {}. {}", i + 1, step);
            }
            println!();
        }

        if !finding.breakpoint_hints.is_empty() {
            println!("    {}", "Breakpoint Hints:".cyan().underline());
            for hint in &finding.breakpoint_hints {
                println!("      {} {}", "→".cyan(), hint);
            }
            println!();
        }

        if !finding.references.is_empty() {
            println!("    {}", "References:".dimmed().underline());
            for r in &finding.references {
                println!("      {}", r.dimmed());
            }
            println!();
        }
    }

    p::separator();
}

/// Naive word-wrap for long description strings.
fn wrap_print(text: &str, max_width: usize, indent: &str) {
    let mut line_len = 0;
    let mut current = String::new();
    for word in text.split_whitespace() {
        if line_len + word.len() + 1 > max_width && !current.is_empty() {
            println!("{}{}", indent, current);
            current = word.to_string();
            line_len = word.len();
        } else {
            if !current.is_empty() {
                current.push(' ');
                line_len += 1;
            }
            current.push_str(word);
            line_len += word.len();
        }
    }
    if !current.is_empty() {
        println!("{}{}", indent, current);
    }
}

/// Parse a list of "name=value" strings into tuples.
fn parse_variables(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|s| {
            let mut parts = s.splitn(2, '=');
            let name = parts
                .next()
                .filter(|n| !n.is_empty())
                .ok_or_else(|| anyhow::anyhow!("Invalid variable format '{}': expected NAME=VALUE", s))?
                .to_string();
            let value = parts.next().unwrap_or("").to_string();
            Ok((name, value))
        })
        .collect()
}
