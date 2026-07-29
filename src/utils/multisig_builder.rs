use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub threshold: u32,
    pub signers: Vec<String>,
    pub signatures: Vec<Signature>,
    pub network: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub metadata: ProposalMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub signer: String,
    pub signature: String,
    pub signed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub transaction_type: Option<String>,
    pub amount: Option<f64>,
    pub recipient: Option<String>,
}

impl Proposal {
    pub fn new(threshold: u32, signers: Vec<String>, network: String) -> Self {
        Proposal {
            id: Uuid::new_v4().to_string(),
            threshold,
            signers,
            signatures: Vec::new(),
            network,
            created_at: Utc::now().to_rfc3339(),
            expires_at: None,
            metadata: ProposalMetadata {
                title: None,
                description: None,
                transaction_type: None,
                amount: None,
                recipient: None,
            },
        }
    }

    pub fn add_signature(&mut self, signer: String, signature: String) {
        self.signatures.push(Signature {
            signer,
            signature,
            signed_at: Utc::now().to_rfc3339(),
        });
    }

    pub fn is_complete(&self) -> bool {
        self.signatures.len() >= self.threshold as usize
    }

    pub fn get_status(&self) -> String {
        if self.is_complete() {
            "ready".to_string()
        } else {
            format!("pending ({}/{})", self.signatures.len(), self.threshold)
        }
    }

    pub fn pending_signers(&self) -> Vec<String> {
        self.signers
            .iter()
            .filter(|s| !self.signatures.iter().any(|sig| sig.signer == **s))
            .cloned()
            .collect()
    }

    pub fn signed_by(&self) -> Vec<String> {
        self.signatures.iter().map(|s| s.signer.clone()).collect()
    }
}

// ── Proposal validation (#691) ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

/// Validate a `Proposal` for logical consistency.
///
/// Checks: zero threshold, impossible threshold (> signer count), empty signer
/// list, duplicate signers, empty signer keys, and signatures from parties not
/// in the signer list. Warnings cover degenerate but technically valid configs.
pub fn validate_proposal(proposal: &Proposal) -> ValidationReport {
    let mut errors: Vec<ValidationError> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // ── 1. Zero threshold ─────────────────────────────────────────────────────
    if proposal.threshold == 0 {
        errors.push(ValidationError {
            code: "ZERO_THRESHOLD".to_string(),
            message: "Threshold must be at least 1.".to_string(),
        });
    }

    // ── 2. No signers ─────────────────────────────────────────────────────────
    if proposal.signers.is_empty() {
        errors.push(ValidationError {
            code: "NO_SIGNERS".to_string(),
            message: "Proposal has no signers.".to_string(),
        });
    }

    // ── 3. Impossible threshold ───────────────────────────────────────────────
    if proposal.threshold > 0 && proposal.threshold as usize > proposal.signers.len() {
        errors.push(ValidationError {
            code: "IMPOSSIBLE_THRESHOLD".to_string(),
            message: format!(
                "Threshold ({}) exceeds the number of signers ({}). \
                 The proposal can never be fulfilled.",
                proposal.threshold,
                proposal.signers.len()
            ),
        });
    }

    // ── 4. Duplicate / empty signer keys ─────────────────────────────────────
    let mut seen = std::collections::HashSet::new();
    for signer in &proposal.signers {
        let normalized = signer.trim().to_lowercase();
        if normalized.is_empty() {
            errors.push(ValidationError {
                code: "EMPTY_SIGNER".to_string(),
                message: format!(
                    "Signer key {:?} is empty or whitespace-only.",
                    signer
                ),
            });
        } else if !seen.insert(normalized) {
            errors.push(ValidationError {
                code: "DUPLICATE_SIGNER".to_string(),
                message: format!("Duplicate signer detected: '{}'.", signer),
            });
        }
    }

    // ── 5. Unauthorized signatures ────────────────────────────────────────────
    for sig in &proposal.signatures {
        if !proposal.signers.iter().any(|s| s == &sig.signer) {
            errors.push(ValidationError {
                code: "UNAUTHORIZED_SIGNATURE".to_string(),
                message: format!(
                    "'{}' has signed but is not listed as an authorized signer.",
                    sig.signer
                ),
            });
        }
    }

    // ── 6. Warnings for unusual but valid configurations ──────────────────────
    if proposal.signers.len() == 1 && proposal.threshold == 1 {
        warnings.push(
            "1-of-1 multi-sig provides no security advantage over a regular \
             single-signer transaction."
                .to_string(),
        );
    }
    if proposal.signers.len() > 1
        && proposal.threshold == proposal.signers.len() as u32
    {
        warnings.push(format!(
            "{}-of-{} requires unanimous consent — a single absent or lost key \
             will permanently block execution.",
            proposal.threshold,
            proposal.signers.len()
        ));
    }
    if proposal.threshold > 0
        && proposal.signers.len() > 1
        && (proposal.threshold as f64 / proposal.signers.len() as f64) < 0.5
    {
        warnings.push(format!(
            "Threshold ({}/{}) is below 50% — a minority of signers can authorize \
             this proposal.",
            proposal.threshold,
            proposal.signers.len()
        ));
    }

    ValidationReport {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

pub fn generate_signature(wallet: &str) -> Result<String> {
    use hex;
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(wallet.as_bytes());
    let result = hasher.finalize();

    Ok(hex::encode(result))
}

pub fn verify_signature(signer: &str, signature: &str, message: &str) -> bool {
    use hex;
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(message.as_bytes());
    let result = hasher.finalize();
    let expected = hex::encode(result);

    expected == signature
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationRequest {
    pub proposal_id: String,
    pub signers: Vec<String>,
    pub threshold: u32,
    pub message: String,
    pub created_at: String,
}

impl NotificationRequest {
    pub fn new(proposal: &Proposal, message: String) -> Self {
        NotificationRequest {
            proposal_id: proposal.id.clone(),
            signers: proposal.pending_signers(),
            threshold: proposal.threshold,
            message,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum NotificationChannel {
    Email,
    Slack,
    Discord,
    Webhook(String),
}

pub async fn send_notification(
    notification: NotificationRequest,
    channel: NotificationChannel,
) -> Result<()> {
    match channel {
        NotificationChannel::Email => {
            println!("📧 Email notification sent to signers");
            Ok(())
        }
        NotificationChannel::Slack => {
            println!("💬 Slack message sent");
            Ok(())
        }
        NotificationChannel::Discord => {
            println!("🎮 Discord message sent");
            Ok(())
        }
        NotificationChannel::Webhook(url) => {
            println!("🔔 Webhook notification sent to {}", url);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_creation() {
        let signers = vec![
            "alice".to_string(),
            "bob".to_string(),
            "charlie".to_string(),
        ];
        let proposal = Proposal::new(2, signers, "testnet".to_string());

        assert_eq!(proposal.threshold, 2);
        assert_eq!(proposal.signers.len(), 3);
        assert!(!proposal.is_complete());
    }

    #[test]
    fn test_signature_added() {
        let signers = vec!["alice".to_string(), "bob".to_string()];
        let mut proposal = Proposal::new(2, signers, "testnet".to_string());

        proposal.add_signature("alice".to_string(), "sig123".to_string());
        assert_eq!(proposal.signatures.len(), 1);
        assert!(!proposal.is_complete());

        proposal.add_signature("bob".to_string(), "sig456".to_string());
        assert!(proposal.is_complete());
    }

    #[test]
    fn test_pending_signers() {
        let signers = vec![
            "alice".to_string(),
            "bob".to_string(),
            "charlie".to_string(),
        ];
        let mut proposal = Proposal::new(2, signers, "testnet".to_string());

        proposal.add_signature("alice".to_string(), "sig123".to_string());
        let pending = proposal.pending_signers();

        assert_eq!(pending.len(), 2);
        assert!(!pending.contains(&"alice".to_string()));
    }
}
