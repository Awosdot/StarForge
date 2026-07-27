//! `starforge ai` — local LLM assistant powered by Ollama.
//!
//! Sub-commands
//! ────────────
//! - `status`   – show Ollama installation and runtime status
//! - `models`   – list locally available models
//! - `pull`     – download a model from the Ollama registry
//! - `ask`      – send a free-form question to the local LLM
//! - `audit`    – AI-powered security audit of a Soroban contract file
//! - `explain`  – plain-English explanation of a Soroban contract
//! - `test`     – generate a test suite for a Soroban contract
//! - `optimise` – gas-optimisation suggestions for a Soroban contract
//! - `plan`     – AI-driven deployment planning with risk assessment

use crate::utils::{
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

    /// Translate text using the local AI (for natural language support)
    Translate {
        /// The text to translate
        #[arg(value_name = "TEXT")]
        text: String,

        /// Target language
        #[arg(short, long)]
        target: String,

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
        AiCommands::Translate { text, target, model } => handle_translate(&text, &target, &model).await,
    }
}

// ─── Existing handlers (unchanged) ──────────────────────────────────────────

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

async fn handle_translate(text: &str, target: &str, model: &str) -> Result<()> {
    if text.trim().is_empty() {
        anyhow::bail!("Please provide text to translate.");
    }
    if target.trim().is_empty() {
        anyhow::bail!("Please provide a target language using --target");
    }

    ensure_ollama_running().await?;

    p::header(&format!("AI Translation — to {}", target));
    p::separator();
    p::kv("Model", model);
    println!();

    let prompt = ollama::prompts::translation_prompt(text, target);
    let opts = GenerateOptions {
        temperature: Some(0.1),
        num_predict: Some(4096),
        num_ctx: Some(8192),
    };

    let spinner = p::spinner("Translating…");
    let response = ollama::generate(model, &prompt, Some(opts))
        .await
        .context("LLM translation failed")?;
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

// ─── NEW: AI Deployment Planning Handler ────────────────────────────────────

async fn handle_plan(
    file: &PathBuf,
    network: &str,
    output: &str,
    max_gas_price: u64,
    prefer_testnet: bool,
    model: &str,
    plan_only: bool,
) -> Result<()> {
    use crate::utils::ai_deployment_planner::{AiDeploymentPlanner, OutputFormat};

    p::header(&format!("AI Deployment Planning — {}", file.display()));
    p::separator();

    p::kv("Contract", &file.display().to_string());
    p::kv("Network", network);
    p::kv("Model", model);
    p::kv("Max Gas Price", &format!("{} gwei", max_gas_price));
    p::kv("Prefer Testnet", if prefer_testnet { "yes" } else { "no" });
    p::kv("Plan Only", if plan_only { "yes" } else { "no" });

    if !plan_only {
        p::info("⚠️  This will execute the deployment if approved.");
        println!();
    }

    let output_format = match output {
        "json" => OutputFormat::Json,
        "yaml" => OutputFormat::Yaml,
        _ => OutputFormat::Table,
    };

    let planner = AiDeploymentPlanner::new(
        file.clone(),
        network.to_string(),
        if max_gas_price > 0 { Some(max_gas_price) } else { None },
        prefer_testnet,
        model.to_string(),
        output_format,
    );

    let spinner = p::spinner("🤖 Analyzing contract and generating deployment plan…");
    let plan = planner.generate_plan().await?;
    spinner.finish_and_clear();

    planner.print_plan(&plan)?;

    if plan_only {
        p::info("✅ Plan generated successfully (plan-only mode).");
    } else {
        println!();
        p::info("🚀 Ready to deploy? Run: starforge deploy --plan <plan-file>");
    }

    p::separator();
    Ok(())
}

// ─── Task enum ────────────────────────────────────────────────────────────────

enum Task {
    Audit,
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
}
