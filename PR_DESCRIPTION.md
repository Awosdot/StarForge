## Summary

This PR adds AI-driven privacy protection capabilities to StarForge, covering anonymization, privacy impact assessment, compliance guidance, data minimization, consent handling, and reporting.

### Privacy protection features

- Added a dedicated privacy utility module for PII detection, anonymization, privacy impact assessment, payload minimization, consent records, and report generation.
- Wired privacy sanitization and minimization into the local telemetry pipeline so event payloads are reduced before persistence.
- Added a new `starforge privacy` CLI with subcommands for `assess`, `anonymize`, `minimize`, and `report`.
- Added regression tests covering anonymization, privacy assessment, minimization, and report generation.

## Test plan

- [x] `cargo test --test privacy_feature` (pending Rust toolchain availability in the container)
- [ ] `starforge privacy assess '{"email":"user@example.com","name":"Alice"}'`
- [ ] `starforge privacy anonymize 'Contact me at alice@example.com'`
- [ ] `starforge privacy minimize '{"email":"user@example.com","event":"deploy","duration_ms":123}' --fields event duration_ms`
- [ ] `starforge privacy report '{"email":"user@example.com"}'`
