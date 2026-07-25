//! Ollama local LLM provider for StarForge.
//!
//! Provides an HTTP client for communicating with a locally running Ollama
//! instance at `http://localhost:11434`, plus helpers for:
//!
//! - Auto-detecting whether Ollama is installed and running
//! - Listing available models
//! - Pulling (downloading) models
//! - Sending chat / generate requests with Soroban-optimised prompts
//! - Falling back to a cloud-provider suggestion when Ollama is unavailable

use crate::utils::http_client::get_client;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default base URL for a locally running Ollama daemon.
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Recommended default model for Soroban contract analysis.
pub const DEFAULT_MODEL: &str = "codellama:7b";

// ─── Ollama API types ────────────────────────────────────────────────────────

/// A model reported by `GET /api/tags`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    /// Full model name, e.g. `"codellama:7b"`.
    pub name: String,
    /// Size in bytes (optional – older Ollama builds may omit this).
    #[serde(default)]
    pub size: u64,
    /// Human-readable modification timestamp.
    #[serde(default)]
    pub modified_at: String,
    /// Digest / hash of the model blob.
    #[serde(default)]
    pub digest: String,
}

/// Response payload from `GET /api/tags`.
#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<OllamaModel>,
}

/// Request body for `POST /api/generate`.
#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<GenerateOptions>,
}

/// Tunable generation hyper-parameters.
#[derive(Debug, Serialize)]
pub struct GenerateOptions {
    /// Sampling temperature (0.0 – 1.0). Lower = more deterministic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<u32>,
    /// Context window size (tokens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u32>,
}

/// Successful response from `POST /api/generate` (non-streaming).
#[derive(Debug, Deserialize)]
pub struct GenerateResponse {
    /// The generated text.
    pub response: String,
    /// Whether the generation finished naturally.
    #[serde(default)]
    pub done: bool,
    /// Total duration in nanoseconds (if reported).
    #[serde(default)]
    pub total_duration: u64,
}

/// Request body for `POST /api/pull`.
#[derive(Debug, Serialize)]
struct PullRequest<'a> {
    name: &'a str,
    stream: bool,
}

/// A single status line returned by `POST /api/pull` (streaming).
#[derive(Debug, Deserialize)]
pub struct PullStatus {
    pub status: String,
    #[serde(default)]
    pub completed: u64,
    #[serde(default)]
    pub total: u64,
}

// ─── Detection helpers ────────────────────────────────────────────────────────

/// Returns `true` if the Ollama binary exists anywhere on `PATH`.
pub fn is_ollama_installed() -> bool {
    which_ollama().is_some()
}

/// Returns the path to the `ollama` binary if it is on `PATH`.
pub fn which_ollama() -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join("ollama");
            if candidate.is_file() {
                Some(candidate)
            } else {
                // On Windows the binary may have a `.exe` suffix.
                let candidate_exe = dir.join("ollama.exe");
                if candidate_exe.is_file() {
                    Some(candidate_exe)
                } else {
                    None
                }
            }
        })
    })
}

/// Performs a lightweight health-check against `GET /api/tags`.
///
/// Returns `true` when Ollama is responding, `false` otherwise.
pub async fn is_ollama_running() -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    client
        .get(format!("{}/api/tags", OLLAMA_BASE_URL))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Comprehensive status snapshot for the Ollama service.
#[derive(Debug)]
pub struct OllamaStatus {
    pub installed: bool,
    pub running: bool,
    pub binary_path: Option<std::path::PathBuf>,
    pub models: Vec<OllamaModel>,
}

/// Collects installation + runtime status in one call.
pub async fn get_status() -> OllamaStatus {
    let binary_path = which_ollama();
    let installed = binary_path.is_some();
    let running = is_ollama_running().await;
    let models = if running {
        list_models().await.unwrap_or_default()
    } else {
        vec![]
    };

    OllamaStatus {
        installed,
        running,
        binary_path,
        models,
    }
}

// ─── Model management ─────────────────────────────────────────────────────────

/// Returns the list of models currently available in the local Ollama store.
pub async fn list_models() -> Result<Vec<OllamaModel>> {
    let url = format!("{}/api/tags", OLLAMA_BASE_URL);
    let resp: TagsResponse = get_client()
        .get(&url)
        .send()
        .await
        .context("Failed to reach Ollama – is `ollama serve` running?")?
        .error_for_status()
        .context("Ollama returned an error response for /api/tags")?
        .json()
        .await
        .context("Failed to parse Ollama /api/tags response")?;
    Ok(resp.models)
}

/// Pulls a model by name, streaming progress lines back to a callback.
///
/// # Arguments
/// * `model_name` – e.g. `"codellama:7b"` or `"llama3"`.
/// * `on_progress` – called for each status line emitted during the pull.
pub async fn pull_model<F>(model_name: &str, mut on_progress: F) -> Result<()>
where
    F: FnMut(PullStatus),
{
    let url = format!("{}/api/pull", OLLAMA_BASE_URL);
    let body = PullRequest {
        name: model_name,
        stream: true,
    };

    let mut response = get_client()
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to reach Ollama for model pull")?
        .error_for_status()
        .context("Ollama returned an error while initiating model pull")?;

    // Stream NDJSON lines.
    let mut buf = String::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Error reading Ollama pull stream")?
    {
        let text = String::from_utf8_lossy(&chunk);
        buf.push_str(&text);
        // Process complete lines.
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf = buf[nl + 1..].to_string();
            if line.is_empty() {
                continue;
            }
            if let Ok(status) = serde_json::from_str::<PullStatus>(&line) {
                on_progress(status);
            }
        }
    }
    let line = buf.trim();
    if !line.is_empty() {
        if let Ok(status) = serde_json::from_str::<PullStatus>(line) {
            on_progress(status);
        }
    }
    Ok(())
}

// ─── Generation / inference ───────────────────────────────────────────────────

/// Sends a plain text prompt to Ollama and returns the full generated response.
///
/// Uses non-streaming mode for simplicity; for long contracts consider
/// `generate_streaming` instead.
pub async fn generate(
    model: &str,
    prompt: &str,
    options: Option<GenerateOptions>,
) -> Result<GenerateResponse> {
    let url = format!("{}/api/generate", OLLAMA_BASE_URL);
    let body = GenerateRequest {
        model,
        prompt,
        stream: false,
        options,
    };

    // Use a longer timeout for LLM inference (default 30s may be too short).
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .context("Failed to build HTTP client for LLM request")?;

    let resp: GenerateResponse = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to reach Ollama generate endpoint")?
        .error_for_status()
        .context("Ollama returned an error during generation")?
        .json()
        .await
        .context("Failed to parse Ollama generate response")?;

    Ok(resp)
}

// ─── Soroban prompt engineering ───────────────────────────────────────────────

/// Pre-built system / task prompt prefixes tuned for Soroban smart-contract work.
pub mod prompts {
    /// System context injected before every user prompt.
    pub const SYSTEM_CONTEXT: &str = "\
You are an expert Stellar and Soroban smart-contract developer assistant integrated \
into the StarForge CLI. You help developers write, review, audit, and optimise \
Soroban contracts written in Rust. Always produce idiomatic Rust that compiles with \
soroban-sdk. Keep answers concise and actionable.\n\n";

    /// Wraps a user question with the Soroban system context.
    pub fn wrap_soroban(user_prompt: &str) -> String {
        format!("{}{}", SYSTEM_CONTEXT, user_prompt)
    }

    /// Prompt for auditing a Soroban contract for security issues.
    pub fn audit_prompt(contract_code: &str) -> String {
        format!(
            "{}Audit the following Soroban contract for security vulnerabilities, \
storage inefficiencies, and potential exploits. List each issue with its \
severity (Critical / High / Medium / Low) and a recommended fix.\n\n\
```rust\n{}\n```",
            SYSTEM_CONTEXT, contract_code
        )
    }

    /// Prompt for explaining what a Soroban contract does.
    pub fn explain_prompt(contract_code: &str) -> String {
        format!(
            "{}Explain what the following Soroban smart contract does in plain English. \
Include a summary of its storage model, entry-point functions, and any notable design \
patterns.\n\n```rust\n{}\n```",
            SYSTEM_CONTEXT, contract_code
        )
    }

    /// Prompt for generating a test suite for a contract.
    pub fn test_prompt(contract_code: &str) -> String {
        format!(
            "{}Generate a comprehensive test suite for the following Soroban contract \
using the soroban-sdk testing harness. Cover happy paths, edge cases, and failure \
conditions.\n\n```rust\n{}\n```",
            SYSTEM_CONTEXT, contract_code
        )
    }

    /// Prompt for optimising a contract's gas usage.
    pub fn optimise_prompt(contract_code: &str) -> String {
        format!(
            "{}Identify gas optimisation opportunities in the following Soroban contract \
and rewrite it to minimise resource consumption while preserving behaviour.\n\n\
```rust\n{}\n```",
            SYSTEM_CONTEXT, contract_code
        )
    }

    /// Prompt for translating text with high accuracy and cultural adaptation.
    pub fn translation_prompt(text: &str, target_lang: &str) -> String {
        format!(
            "{}Translate the following text into {}, ensuring high accuracy (>90%) and appropriate cultural adaptation. \
If the text contains CLI commands, error messages, or technical documentation, preserve the technical meaning perfectly.\n\n\
Text to translate:\n{}",
            SYSTEM_CONTEXT, target_lang, text
        )
    }
}

// ─── Cloud fallback ────────────────────────────────────────────────────────────

/// Suggestion message shown when Ollama is unavailable.
pub fn cloud_fallback_message() -> &'static str {
    "\
Ollama is not available on this machine. To use local AI features:\n\
  1. Install Ollama: https://ollama.ai/download\n\
  2. Start the daemon: ollama serve\n\
  3. Pull a model:    ollama pull codellama:7b\n\n\
Alternatively, consider cloud providers:\n\
  • OpenAI  – https://platform.openai.com\n\
  • Anthropic Claude – https://www.anthropic.com\n\
  • Google Gemini    – https://ai.google.dev"
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_non_empty() {
        assert!(!DEFAULT_MODEL.is_empty());
    }

    #[test]
    fn base_url_points_to_localhost() {
        assert!(OLLAMA_BASE_URL.contains("localhost"));
    }

    #[test]
    fn prompts_wrap_system_context() {
        let wrapped = prompts::wrap_soroban("What does storage_get do?");
        assert!(wrapped.contains(prompts::SYSTEM_CONTEXT));
        assert!(wrapped.contains("What does storage_get do?"));
    }

    #[test]
    fn audit_prompt_includes_contract_code() {
        let code = "#[contract] pub struct Token;";
        let prompt = prompts::audit_prompt(code);
        assert!(prompt.contains(code));
        assert!(prompt.to_lowercase().contains("audit"));
    }

    #[test]
    fn explain_prompt_includes_contract_code() {
        let code = "pub fn mint(env: Env) {}";
        let prompt = prompts::explain_prompt(code);
        assert!(prompt.contains(code));
    }

    #[test]
    fn test_prompt_includes_contract_code() {
        let code = "pub fn transfer(env: Env) {}";
        let prompt = prompts::test_prompt(code);
        assert!(prompt.contains(code));
    }

    #[test]
    fn optimise_prompt_includes_contract_code() {
        let code = "pub fn heavy(env: Env) {}";
        let prompt = prompts::optimise_prompt(code);
        assert!(prompt.contains(code));
    }

    #[test]
    fn cloud_fallback_message_mentions_ollama() {
        let msg = cloud_fallback_message();
        assert!(msg.contains("ollama") || msg.contains("Ollama"));
    }

    #[test]
    fn generate_options_serialises_cleanly() {
        let opts = GenerateOptions {
            temperature: Some(0.1),
            num_predict: Some(512),
            num_ctx: Some(4096),
        };
        let json = serde_json::to_string(&opts).unwrap();
        assert!(json.contains("temperature"));
        assert!(json.contains("num_predict"));
        assert!(json.contains("num_ctx"));
    }

    #[test]
    fn ollama_model_deserialises_from_json() {
        let json = r#"{"name":"codellama:7b","size":3826793472,"modified_at":"2024-01-01T00:00:00Z","digest":"abc123"}"#;
        let model: OllamaModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "codellama:7b");
        assert_eq!(model.size, 3826793472);
    }

    #[test]
    fn ollama_model_deserialises_with_missing_optional_fields() {
        let json = r#"{"name":"llama3"}"#;
        let model: OllamaModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "llama3");
        assert_eq!(model.size, 0);
    }
}
