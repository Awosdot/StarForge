## Enforce CLI Startup and Command Latency Budgets (Issue #698)

### Summary

Adds a comprehensive latency budget enforcement system for the StarForge CLI. Tracks representative cold-start and command paths via Criterion benchmarks and flags statistically meaningful regressions in CI.

### Changes

**Core Engine** — `src/utils/latency_budget.rs` (new)
- `LatencyBudget` / `LatencyBudgets` — per-command threshold definitions with defaults for 13 CLI paths
- `check_measurement()` — checks a single benchmark measurement against its budget (Pass/Fail/Noisy/Skipped/Error)
- `check_latency_budget()` — runs a full budget check across all active budgets
- `is_significant_regression()` — CV-based z-score statistical regression detection
- `budget_report_to_json()` / `print_budget_summary()` — JSON and human-readable output
- Environment variable overrides via `STARFORGE_LATENCY_BUDGET_<LABEL>`
- 18 unit tests covering primary flow, boundaries, and failures

**Criterion Benchmarks** — `benches/benchmarks.rs` (modified)
- `bench_cli_cold_start` — measures `info`, `--help`, `--version` cold-start latency
- `bench_cli_command_latency` — measures 10 representative command dispatch paths (wallet, network, config, template, deploy, benchmark)
- `bench_latency_budget_check` — measures budget check engine overhead

**Integration Tests** — `tests/latency_budget_test.rs` (new, 22 tests)
- Primary flow: fast/slow measurements, single-budget pass/fail
- Boundary: zero budget, exact boundary, high CV, CV disabled
- Failure: zero sample size, missing measurement, inactive budget
- Environment: env var override, disable, invalid value (with proper save/restore)
- Statistical regression: detection, no-regression, zero baseline, zero CV
- JSON report: round-trip validation, print summary

**CI Workflow** — `.github/workflows/benchmark-latency.yml` (new)
- Runs latency benchmarks on every push/PR touching relevant files
- Parses Criterion output via `scripts/check-latency-budgets.sh` and checks budgets
- Fails the pipeline on budget violations
- Uploads Criterion artefact and posts PR comment with budget summary table

**Budget Check Script** — `scripts/check-latency-budgets.sh` (new)
- Parses Criterion's default stdout format to extract median latencies
- Mirrors budgets from the Rust code with env var override support
- Generates JSON report consumed by CI

**Documentation**
- `CLI_LATENCY_BUDGETS.md` — full user-facing docs with budgets table, env var overrides, CI integration, adding budgets, security/compatibility/migration notes
- `BENCHMARKS.md` — updated benchmark groups table and CI integration section

### Acceptance Criteria Met
- ✅ Handles invalid input (zero sample size, missing measurements, unparseable env vars)
- ✅ Handles unsupported environments (inactive budgets, env var "off" toggle)
- ✅ Handles failure paths (Error status propagation)
- ✅ Automated tests cover primary flow, boundary cases, and failure cases (22 integration + 18 unit = 40 tests)
- ✅ User-facing documentation includes compatibility, security, and migration notes

### Related Issue
Closes #698
