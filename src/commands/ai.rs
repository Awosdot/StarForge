//! `starforge ai` — local LLM assistant powered by Ollama.
//!
//! Sub-commands
//! ────────────
//! - `status`          – show Ollama installation and runtime status
//! - `models`          – list locally available models
//! - `pull`            – download a model from the Ollama registry
//! - `ask`             – send a free-form question to the local LLM
//! - `audit`           – AI-powered security audit of a Soroban contract file
//! - `explain`         – plain-English explanation of a Soroban contract
//! - `test`            – generate a test suite for a Soroban contract
//! - `optimise`        – gas-optimisation suggestions for a Soroban contract
//! - `profile`         – AI-driven performance profiling of a compiled WASM
//! - `compare-profiles`– AI-generated comparative analysis between two profile snapshots

use crate::utils::{
    contract_profiler,
    ollama::{self, GenerateOptions},
    print as p,
};
use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::PathBuf;

// ─── Sub-command enum ─────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum AiCommands {
    /// Show Ollama installation and runtime status
    Status,

    /// List locally available Ollama models
    Models,

    /// Download an Ollama model (e.g. `codellama:7b`)
    Pull {
        /// Model name to download, e.g. `codellama:7b`
        #[arg(default_value = ollama::DEFAULT_MODEL)]
        model: String,
    },

    /// Ask the local LLM a free-form question (Soroban context included)
    Ask {
        /// Your question or task description
        #[arg(trailing_var_arg = true, num_args = 1..)]
        question: Vec<String>,

        /// Model to use (defaults to `codellama:7b`)
        #[arg(short, long, default_value = ollama::DEFAULT_MODEL)]
        model: String,

        /// Sampling temperature 0.0–1.0 (lower = more deterministic)
        #[arg(long, default_value_t = 0.2)]
        temperature: f32,

        /// Maximum tokens to generate
        #[arg(long, default_value_t = 1024)]
        max_tokens: u32,
    },

    /// AI security audit of a Soroban contract source file
    Audit {
        /// Path to the Rust source file to audit
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Model to use
        #[arg(short, long, default_value = ollama::DEFAULT_MODEL)]
        model: String,
    },

    /// Explain what a Soroban contract does (plain English)
    Explain {
        /// Path to the Rust source file to explain
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Model to use
        #[arg(short, long, default_value = ollama::DEFAULT_MODEL)]
        model: String,
    },

    /// Generate a test suite for a Soroban contract
    Test {
        /// Path to the Rust source file to generate tests for
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Model to use
        #[arg(short, long, default_value = ollama::DEFAULT_MODEL)]
        model: String,
    },

    /// Suggest gas optimisations for a Soroban contract
    Optimise {
        /// Path to the Rust source file to optimise
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Model to use
        #[arg(short, long, default_value = ollama::DEFAULT_MODEL)]
        model: String,
    },

    /// AI-driven performance profiling of a compiled Soroban WASM
    ///
    /// Runs StarForge's static WASM analyser and then asks the local LLM to
    /// explain bottlenecks, rank optimisation opportunities, and give advice
    /// on tracking performance over time.
    Profile {
        /// Path to the compiled contract WASM
        #[arg(value_name = "WASM")]
        wasm: PathBuf,

        /// Human-readable label for the contract (shown in the report)
        #[arg(long)]
        label: Option<String>,

        /// Previous JSON profile snapshot for regression comparison
        #[arg(long)]
        baseline: Option<PathBuf>,

        /// Write the static JSON profile to this path (optional)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Write an HTML performance dashboard to this path (optional)
        #[arg(long)]
        dashboard: Option<PathBuf>,

        /// Model to use for AI analysis
        #[arg(short, long, default_value = ollama::DEFAULT_MODEL)]
        model: String,
    },

    /// AI comparative analysis between two saved performance profile snapshots
    ///
    /// Loads two JSON profiles produced by `starforge ai profile --output` or
    /// `starforge advanced-perf profile --output` and asks the LLM to explain
    /// what changed and whether the candidate is safe to deploy.
    CompareProfiles {
        /// Path to the baseline JSON profile
        #[arg(value_name = "BASELINE")]
        baseline: PathBuf,

        /// Path to the candidate JSON profile
        #[arg(value_name = "CANDIDATE")]
        candidate: PathBuf,

        /// Model to use
        #[arg(short, long, default_value = ollama::DEFAULT_MODEL)]
        model: String,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────────

pub async fn handle(cmd: AiCommands) -> Result<()> {
    match cmd {
        AiCommands::Status => handle_status().await,
        AiCommands::Models => handle_models().await,
        AiCommands::Pull { model } => handle_pull(&model).await,
        AiCommands::Ask {
            question,
            model,
            temperature,
            max_tokens,
        } => handle_ask(&question.join(" "), &model, temperature, max_tokens).await,
        AiCommands::Audit { file, model } => handle_file_task(&file, &model, Task::Audit).await,
        AiCommands::Explain { file, model } => handle_file_task(&file, &model, Task::Explain).await,
        AiCommands::Test { file, model } => handle_file_task(&file, &model, Task::Test).await,
        AiCommands::Optimise { file, model } => {
            handle_file_task(&file, &model, Task::Optimise).await
        }
        AiCommands::Profile {
            wasm,
            label,
            baseline,
            output,
            dashboard,
            model,
        } => handle_profile(&wasm, label.as_deref(), baseline.as_deref(), output.as_deref(), dashboard.as_deref(), &model).await,
        AiCommands::CompareProfiles {
            baseline,
            candidate,
            model,
        } => handle_compare_profiles(&baseline, &candidate, &model).await,
    }
}

// ─── Handler implementations ──────────────────────────────────────────────────

async fn handle_status() -> Result<()> {
    p::header("Ollama Local LLM Status");
    p::separator();

    let status = ollama::get_status().await;

    let installed_str = if status.installed { "yes" } else { "no" };
    let running_str = if status.running { "yes ✓" } else { "no  ✗" };

    p::kv("Installed", installed_str);
    if let Some(ref path) = status.binary_path {
        p::kv("Binary path", &path.display().to_string());
    }
    p::kv("Running", running_str);
    p::kv("Base URL", ollama::OLLAMA_BASE_URL);

    if status.running {
        p::kv("Available models", &status.models.len().to_string());
        if !status.models.is_empty() {
            println!();
            let headers = &["Model", "Size", "Digest"];
            let rows: Vec<Vec<String>> = status
                .models
                .iter()
                .map(|m| {
                    vec![
                        m.name.clone(),
                        format_bytes(m.size),
                        m.digest.chars().take(12).collect::<String>(),
                    ]
                })
                .collect();
            p::table(headers, &rows);
        }
    } else if !status.installed {
        println!();
        p::warn("Ollama is not installed.");
        println!();
        println!("{}", ollama::cloud_fallback_message());
    } else {
        println!();
        p::warn("Ollama is installed but not running.");
        p::info("Start it with: ollama serve");
    }

    p::separator();
    Ok(())
}

async fn handle_models() -> Result<()> {
    ensure_ollama_running().await?;

    p::header("Local Ollama Models");
    p::separator();

    let models = ollama::list_models()
        .await
        .context("Failed to list Ollama models")?;

    if models.is_empty() {
        p::warn("No models found locally.");
        p::info(&format!(
            "Pull the recommended model: starforge ai pull {}",
            ollama::DEFAULT_MODEL
        ));
    } else {
        let headers = &["Model", "Size", "Modified", "Digest"];
        let rows: Vec<Vec<String>> = models
            .iter()
            .map(|m| {
                vec![
                    m.name.clone(),
                    format_bytes(m.size),
                    m.modified_at.chars().take(19).collect(),
                    m.digest.chars().take(12).collect::<String>(),
                ]
            })
            .collect();
        p::table(headers, &rows);
        println!();
        p::success(&format!("{} model(s) available.", models.len()));
    }

    p::separator();
    Ok(())
}

async fn handle_pull(model: &str) -> Result<()> {
    ensure_ollama_running().await?;

    p::header(&format!("Pulling model: {}", model));
    p::separator();
    p::info("Connecting to Ollama model registry…");

    use indicatif::{ProgressBar, ProgressStyle};

    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}% {msg}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.set_message("initialising…");

    let model_name = model.to_string();
    ollama::pull_model(&model_name, |status| {
        pb.set_message(status.status.clone());
        if status.total > 0 {
            let pct = ((status.completed as f64 / status.total as f64) * 100.0) as u64;
            pb.set_position(pct.min(100));
        } else {
            pb.tick();
        }
    })
    .await
    .context("Failed to pull model")?;

    pb.finish_with_message("done");
    println!();
    p::success(&format!("Model '{}' downloaded successfully.", model));
    p::separator();
    Ok(())
}

async fn handle_ask(question: &str, model: &str, temperature: f32, max_tokens: u32) -> Result<()> {
    if question.trim().is_empty() {
        anyhow::bail!("Please provide a question. Usage: starforge ai ask \"<question>\"");
    }

    ensure_ollama_running().await?;

    p::header("AI Assistant (Soroban)");
    p::separator();
    p::kv("Model", model);
    p::kv("Question", question);
    println!();

    let prompt = ollama::prompts::wrap_soroban(question);
    let opts = GenerateOptions {
        temperature: Some(temperature),
        num_predict: Some(max_tokens),
        num_ctx: Some(4096),
    };

    let spinner = p::spinner("Thinking…");
    let response = ollama::generate(model, &prompt, Some(opts))
        .await
        .context("LLM generation failed")?;
    spinner.finish_and_clear();

    println!("{}", response.response.trim());

    if response.total_duration > 0 {
        println!();
        let ms = response.total_duration / 1_000_000;
        p::kv("Time", &format!("{ms}ms"));
    }

    p::separator();
    Ok(())
}

/// Shared handler for file-based tasks (audit / explain / test / optimise).
async fn handle_file_task(file: &PathBuf, model: &str, task: Task) -> Result<()> {
    let code = std::fs::read_to_string(file)
        .with_context(|| format!("Cannot read source file: {}", file.display()))?;

    ensure_ollama_running().await?;

    let task_label = task.label();
    p::header(&format!("AI {} — {}", task_label, file.display()));
    p::separator();
    p::kv("Model", model);
    p::kv("File", &file.display().to_string());
    p::kv("Source lines", &code.lines().count().to_string());
    println!();

    let prompt = task.build_prompt(&code);
    let opts = GenerateOptions {
        temperature: Some(0.1),
        num_predict: Some(2048),
        num_ctx: Some(8192),
    };

    let spinner = p::spinner(&format!("Running {}…", task_label.to_lowercase()));
    let response = ollama::generate(model, &prompt, Some(opts))
        .await
        .with_context(|| format!("LLM {} failed", task_label.to_lowercase()))?;
    spinner.finish_and_clear();

    println!("{}", response.response.trim());

    if response.total_duration > 0 {
        println!();
        let ms = response.total_duration / 1_000_000;
        p::kv("Time", &format!("{ms}ms"));
    }

    p::separator();
    Ok(())
}

// ─── AI Performance Profiling ─────────────────────────────────────────────────

/// Run static WASM profiling then feed the structured report to the LLM for
/// AI-driven bottleneck identification, optimisation suggestions, comparative
/// analysis, historical tracking advice, and visual report guidance.
async fn handle_profile(
    wasm: &std::path::Path,
    label: Option<&str>,
    baseline: Option<&std::path::Path>,
    output: Option<&std::path::Path>,
    dashboard: Option<&std::path::Path>,
    model: &str,
) -> Result<()> {
    p::header("AI Contract Performance Profiling");
    p::separator();
    p::kv("WASM", &wasm.display().to_string());
    if let Some(l) = label {
        p::kv("Label", l);
    }
    if let Some(b) = baseline {
        p::kv("Baseline", &b.display().to_string());
    }
    p::kv("Model", model);
    p::separator();

    // ── Step 1: Static WASM analysis ─────────────────────────────────────────
    let spinner = p::spinner("Running static WASM analysis…");
    let report = contract_profiler::profile_contract_wasm(wasm, label, baseline)
        .with_context(|| format!("Failed to profile WASM: {}", wasm.display()))?;
    spinner.finish_and_clear();
    p::success("Static analysis complete.");

    // Show key static metrics immediately so the user has numbers even before
    // the LLM responds.
    println!();
    p::info("Static Profile Summary");
    p::kv("Profile ID", &report.id);
    p::kv(
        "Estimated gas",
        &report.dashboard_summary.total_estimated_gas.to_string(),
    );
    p::kv(
        "Est. invocation time",
        &format!("{:.2} ms", report.dashboard_summary.estimated_invocation_time_ms),
    );
    p::kv(
        "Est. peak memory",
        &format!("{} bytes", report.dashboard_summary.estimated_peak_memory_bytes),
    );
    p::kv(
        "Bottlenecks found",
        &report.dashboard_summary.bottleneck_count.to_string(),
    );
    p::kv(
        "Regression detected",
        &report.dashboard_summary.regression_detected.to_string(),
    );
    p::kv("Optimisation score", &format!("{}/100", report.optimization_score));

    if let Some(cmp) = &report.comparison {
        println!();
        p::info("Regression Comparison");
        p::kv("Verdict", &cmp.verdict);
        p::kv("Gas delta", &format!("{:+.2}%", cmp.gas_delta_pct));
        p::kv("Execution time delta", &format!("{:+.2}%", cmp.execution_time_delta_pct));
        p::kv("Memory delta", &format!("{:+.2}%", cmp.memory_delta_pct));
    }

    // ── Step 2: Persist static JSON & HTML artefacts ─────────────────────────
    let json_path = if let Some(path) = output {
        contract_profiler::write_profile_report(&report, path)?;
        path.to_path_buf()
    } else {
        contract_profiler::save_profile_report(&report)?
    };
    p::success(&format!("JSON profile saved → {}", json_path.display()));

    if let Some(path) = dashboard {
        contract_profiler::write_dashboard_html(&report, path)?;
        p::success(&format!("HTML dashboard saved → {}", path.display()));
    }

    // ── Step 3: AI analysis ───────────────────────────────────────────────────
    println!();
    ensure_ollama_running().await?;

    let profile_json = serde_json::to_string_pretty(&report)
        .context("Failed to serialise profile report for LLM")?;
    let prompt = ollama::prompts::performance_profile_prompt(&profile_json);
    let opts = GenerateOptions {
        temperature: Some(0.1),
        num_predict: Some(2048),
        num_ctx: Some(8192),
    };

    let spinner = p::spinner("Asking AI to analyse performance profile…");
    let response = ollama::generate(model, &prompt, Some(opts))
        .await
        .context("LLM performance profiling failed")?;
    spinner.finish_and_clear();

    println!();
    p::info("AI Performance Analysis");
    p::separator();
    println!("{}", response.response.trim());

    if response.total_duration > 0 {
        println!();
        let ms = response.total_duration / 1_000_000;
        p::kv("AI response time", &format!("{ms} ms"));
    }

    println!();
    p::info("Next Steps");
    for action in &report.dashboard_summary.next_actions {
        println!("  • {}", action);
    }

    p::separator();
    Ok(())
}

/// Load two saved profile snapshots and ask the LLM for a comparative analysis.
async fn handle_compare_profiles(
    baseline_path: &std::path::Path,
    candidate_path: &std::path::Path,
    model: &str,
) -> Result<()> {
    p::header("AI Profile Comparison");
    p::separator();
    p::kv("Baseline", &baseline_path.display().to_string());
    p::kv("Candidate", &candidate_path.display().to_string());
    p::kv("Model", model);
    p::separator();

    let baseline_report = contract_profiler::load_profile_report(baseline_path)?;
    let candidate_report = contract_profiler::load_profile_report(candidate_path)?;

    // Show a quick numeric diff before the LLM reply.
    println!();
    p::info("Metric Snapshot");

    let gas_delta_pct = percent_delta(
        candidate_report.dashboard_summary.total_estimated_gas as f64,
        baseline_report.dashboard_summary.total_estimated_gas as f64,
    );
    let time_delta_pct = percent_delta(
        candidate_report.dashboard_summary.estimated_invocation_time_ms,
        baseline_report.dashboard_summary.estimated_invocation_time_ms,
    );
    let mem_delta_pct = percent_delta(
        candidate_report.dashboard_summary.estimated_peak_memory_bytes as f64,
        baseline_report.dashboard_summary.estimated_peak_memory_bytes as f64,
    );

    p::kv("Gas delta", &format!("{:+.2}%", gas_delta_pct));
    p::kv("Invocation time delta", &format!("{:+.2}%", time_delta_pct));
    p::kv("Peak memory delta", &format!("{:+.2}%", mem_delta_pct));
    p::kv(
        "Baseline score",
        &format!("{}/100", baseline_report.optimization_score),
    );
    p::kv(
        "Candidate score",
        &format!("{}/100", candidate_report.optimization_score),
    );

    // ── AI comparative analysis ───────────────────────────────────────────────
    println!();
    ensure_ollama_running().await?;

    let baseline_json = serde_json::to_string_pretty(&baseline_report)
        .context("Failed to serialise baseline profile")?;
    let candidate_json = serde_json::to_string_pretty(&candidate_report)
        .context("Failed to serialise candidate profile")?;
    let prompt = ollama::prompts::profile_comparison_prompt(&baseline_json, &candidate_json);
    let opts = GenerateOptions {
        temperature: Some(0.1),
        num_predict: Some(2048),
        num_ctx: Some(8192),
    };

    let spinner = p::spinner("Asking AI to compare profiles…");
    let response = ollama::generate(model, &prompt, Some(opts))
        .await
        .context("LLM profile comparison failed")?;
    spinner.finish_and_clear();

    println!();
    p::info("AI Comparative Analysis");
    p::separator();
    println!("{}", response.response.trim());

    if response.total_duration > 0 {
        println!();
        let ms = response.total_duration / 1_000_000;
        p::kv("AI response time", &format!("{ms} ms"));
    }

    p::separator();
    Ok(())
}

/// Simple percentage delta helper (shared between profile handlers).
fn percent_delta(current: f64, baseline: f64) -> f64 {
    if baseline.abs() < f64::EPSILON {
        0.0
    } else {
        ((current - baseline) / baseline) * 100.0
    }
}

// ─── Task enum ────────────────────────────────────────────────────────────────

enum Task {
    Explain,
    Test,
    Optimise,
}

impl Task {
    fn label(&self) -> &'static str {
        match self {
            Task::Audit => "Security Audit",
            Task::Explain => "Explanation",
            Task::Test => "Test Generation",
            Task::Optimise => "Gas Optimisation",
        }
    }

    fn build_prompt(&self, code: &str) -> String {
        match self {
            Task::Audit => ollama::prompts::audit_prompt(code),
            Task::Explain => ollama::prompts::explain_prompt(code),
            Task::Test => ollama::prompts::test_prompt(code),
            Task::Optimise => ollama::prompts::optimise_prompt(code),
        }
    }
}

// ─── Guard ────────────────────────────────────────────────────────────────────

/// Checks that Ollama is reachable; returns a helpful error if not.
async fn ensure_ollama_running() -> Result<()> {
    if !ollama::is_ollama_running().await {
        anyhow::bail!(
            "Ollama is not running.\n\n{}",
            ollama::cloud_fallback_message()
        );
    }
    Ok(())
}

// ─── Utilities ────────────────────────────────────────────────────────────────

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "—".to_string();
    }
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} KB", bytes / 1024)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "—");
    }

    #[test]
    fn format_bytes_kilobytes() {
        assert!(format_bytes(512 * 1024).contains("KB"));
    }

    #[test]
    fn format_bytes_megabytes() {
        assert!(format_bytes(50 * 1_048_576).contains("MB"));
    }

    #[test]
    fn format_bytes_gigabytes() {
        assert!(format_bytes(3 * 1_073_741_824).contains("GB"));
    }

    #[test]
    fn task_labels_are_non_empty() {
        for task in [Task::Audit, Task::Explain, Task::Test, Task::Optimise] {
            assert!(!task.label().is_empty());
        }
    }

    #[test]
    fn task_prompts_include_code() {
        let code = "pub fn hello() {}";
        for task in [Task::Audit, Task::Explain, Task::Test, Task::Optimise] {
            assert!(task.build_prompt(code).contains(code));
        }
    }

    // ── AI profiling helpers ──────────────────────────────────────────────────

    #[test]
    fn percent_delta_zero_baseline_returns_zero() {
        assert_eq!(percent_delta(100.0, 0.0), 0.0);
    }

    #[test]
    fn percent_delta_improvement_is_negative() {
        let delta = percent_delta(80.0, 100.0);
        assert!(delta < 0.0, "expected negative delta for improvement");
        assert!((delta - -20.0).abs() < 0.001);
    }

    #[test]
    fn percent_delta_regression_is_positive() {
        let delta = percent_delta(120.0, 100.0);
        assert!(delta > 0.0, "expected positive delta for regression");
        assert!((delta - 20.0).abs() < 0.001);
    }

    #[test]
    fn percent_delta_no_change_is_zero() {
        assert_eq!(percent_delta(100.0, 100.0), 0.0);
    }
}
