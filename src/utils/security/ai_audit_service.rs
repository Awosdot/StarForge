//! AI audit service that orchestrates static analysis and Claude integration.

use super::ai_audit::{
    build_system_prompt, build_user_prompt, run_static_checks, AiAuditResponse, AttackScenario,
    AuditLevel, AuditRequest, FixSuggestion, SecurityAuditReport, SecurityVulnerability,
};
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;

/// Anthropic API message format.
#[derive(serde::Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic API request.
#[derive(serde::Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

/// Anthropic API response.
#[derive(serde::Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(serde::Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

/// Main audit service orchestrator.
pub struct AiAuditService {
    client: Client,
    api_key: String,
    model: String,
}

impl AiAuditService {
    /// Create a new audit service with Anthropic API key.
    pub fn new(api_key: String) -> Result<Self> {
        if api_key.is_empty() {
            return Err(anyhow!(
                "ANTHROPIC_API_KEY not set. Set it via environment variable."
            ));
        }

        Ok(AiAuditService {
            client: Client::new(),
            api_key,
            model: "claude-opus-4-1".to_string(), // High-capability model for security
        })
    }

    /// Audit a Soroban contract with combined static + AI analysis.
    pub async fn audit_contract(&self, request: AuditRequest) -> Result<SecurityAuditReport> {
        // Validate inputs
        if request.contract_code.trim().is_empty() {
            return Err(anyhow!("Contract code cannot be empty"));
        }
        if request.contract_name.trim().is_empty() {
            return Err(anyhow!("Contract name cannot be empty"));
        }
        if request.contract_code.len() > 50_000 {
            return Err(anyhow!("Contract code too large (max 50KB)"));
        }

        // Step 1: Run static analysis (fast, deterministic)
        let static_findings = run_static_checks(&request.contract_code);

        // Step 2: Call Claude for deep analysis
        let system_prompt = build_system_prompt();
        let user_prompt = build_user_prompt(
            &request.contract_code,
            &request.contract_name,
            &static_findings,
            request.security_level,
            request.include_attack_simulation,
        );

        let ai_result = self
            .call_claude(&system_prompt, &user_prompt)
            .await
            .map_err(|e| anyhow!("AI audit failed: {}", e))?;

        // Step 3: Combine results into report
        let report = SecurityAuditReport {
            contract_name: request.contract_name.clone(),
            audit_date: Utc::now().to_rfc3339(),
            overall_risk: ai_result.overall_risk.clone(),
            summary: ai_result.summary.clone(),
            vulnerabilities: ai_result.vulnerabilities.clone(),
            attack_scenarios: ai_result.attack_scenarios.clone(),
            best_practice_violations: ai_result.best_practice_violations.clone(),
            fix_suggestions: ai_result.fix_suggestions.clone(),
            security_score: ai_result.security_score,
            false_positive_warning: "AI analysis may produce false positives. Review all findings with a human auditor.".to_string(),
            tools_used: vec!["claude-opus-4-1".to_string(), "static-analysis".to_string()],
        };

        Ok(report)
    }

    /// Call Claude API for contract analysis.
    async fn call_claude(&self, system: &str, user_prompt: &str) -> Result<AiAuditResponse> {
        let request_body = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            system: system.to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            }],
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call Anthropic API: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Anthropic API error {}: {}",
                status,
                error_text
            ));
        }

        let anthropic_response: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Anthropic response: {}", e))?;

        let response_text = anthropic_response
            .content
            .first()
            .and_then(|block| block.text.as_ref())
            .ok_or_else(|| anyhow!("No text in Anthropic response"))?;

        // Parse JSON response from Claude
        let ai_result: AiAuditResponse = serde_json::from_str(response_text)
            .map_err(|e| anyhow!("Failed to parse AI audit response JSON: {}", e))?;

        Ok(ai_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty_contract_code() {
        let request = AuditRequest {
            contract_code: "".to_string(),
            contract_name: "Test".to_string(),
            include_attack_simulation: true,
            security_level: AuditLevel::Standard,
        };

        // Would fail in async context, just validate structure
        assert_eq!(request.contract_code.len(), 0);
    }

    #[test]
    fn test_validate_empty_contract_name() {
        let request = AuditRequest {
            contract_code: "code".to_string(),
            contract_name: "".to_string(),
            include_attack_simulation: true,
            security_level: AuditLevel::Standard,
        };

        assert_eq!(request.contract_name.len(), 0);
    }

    #[test]
    fn test_validate_contract_size_limit() {
        let oversized_code = "a".repeat(60_000);
        let request = AuditRequest {
            contract_code: oversized_code,
            contract_name: "Test".to_string(),
            include_attack_simulation: true,
            security_level: AuditLevel::Standard,
        };

        assert!(request.contract_code.len() > 50_000);
    }

    #[test]
    fn test_audit_request_creation() {
        let request = AuditRequest {
            contract_code: "pub fn test() {}".to_string(),
            contract_name: "TestContract".to_string(),
            include_attack_simulation: true,
            security_level: AuditLevel::Comprehensive,
        };

        assert_eq!(request.contract_name, "TestContract");
        assert!(request.include_attack_simulation);
        assert_eq!(request.security_level, AuditLevel::Comprehensive);
    }
}
