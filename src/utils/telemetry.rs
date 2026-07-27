use crate::utils::{config, privacy};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct TelemetryData {
    pub timestamp: DateTime<Utc>,
    pub event: String,
    pub properties: serde_json::Value,
    pub anonymous_id: String,
}

pub fn track_event(event: &str, properties: serde_json::Value) -> Result<()> {
    // Check environment variable first (for CI/automation that cannot modify config)
    if let Ok(env_val) = std::env::var("STARFORGE_TELEMETRY") {
        let disabled = matches!(
            env_val.to_lowercase().as_str(),
            "0" | "false" | "off" | "disabled" | "no"
        );
        if disabled {
            return Ok(());
        }
    }

    let cfg = config::load()?;

    // Check if telemetry is enabled (default to true, but respect opt-out)
    if !cfg.telemetry_enabled.unwrap_or(true) {
        return Ok(());
    }

    let anonymous_id = get_or_create_anonymous_id()?;
    let minimized_properties =
        privacy::minimize_payload(&properties, &["event", "success", "duration_ms"]);
    let sanitized_properties = privacy::sanitize_payload(&minimized_properties);
    let assessment = privacy::assess_privacy_impact(&sanitized_properties, "telemetry", true);
    let consent = privacy::ConsentRecord::new("telemetry", true);
    let report = privacy::build_privacy_report(&assessment, &consent);
    let _ = privacy::persist_privacy_report(&report);

    let data = TelemetryData {
        timestamp: Utc::now(),
        event: event.to_string(),
        properties: sanitized_properties,
        anonymous_id,
    };

    // Telemetry is saved ONLY locally in the data directory.
    // Absolutely NO network requests are made for telemetry transmission.
    save_telemetry_locally(&data)?;

    Ok(())
}

fn get_or_create_anonymous_id() -> Result<String> {
    let data_dir = config::get_data_dir()?;
    let id_file = data_dir.join("anonymous_id");

    if id_file.exists() {
        Ok(fs::read_to_string(id_file)?.trim().to_string())
    } else {
        let id = Uuid::new_v4().to_string();
        fs::write(id_file, &id)?;
        Ok(id)
    }
}

fn save_telemetry_locally(data: &TelemetryData) -> Result<()> {
    let data_dir = config::get_data_dir()?;
    let telemetry_log = data_dir.join("telemetry.log");

    let json = serde_json::to_string(data)?;

    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(telemetry_log)?;

    writeln!(file, "{}", json)?;

    Ok(())
}

#[allow(dead_code)]
pub fn set_telemetry_enabled(enabled: bool) -> Result<()> {
    let mut cfg = config::load()?;
    cfg.telemetry_enabled = Some(enabled);
    config::save(&cfg)?;
    Ok(())
}
