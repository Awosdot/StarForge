pub mod ai_debugger;
pub mod ai_deployment_testing;
pub mod ai_docs;
pub mod ai_gas_estimation;
pub mod ai_ide_integration;
pub mod ai_performance_profiler;
pub mod ai_test_maintenance;
pub mod approval_engine;
pub mod audit;
pub mod backup;
pub mod benchmarking;
pub mod bindings;
pub mod bridge;
pub mod call_graph;
pub mod completion;
pub mod compliance;
pub mod config;
pub mod confirmation;
pub mod contract_assertions;
pub mod contract_deps;
pub mod contract_fixtures;
pub mod contract_mocks;
pub mod contract_profiler;
pub mod contract_test_framework;
pub mod contract_test_runner;
pub mod contract_testing;
pub mod cost_estimation;
pub mod crypto;
pub mod database;
pub mod debugger;
pub mod deploy_history;
pub mod deploy_orchestrator;
pub mod deployment_automation;
pub mod deployment_optimizer;
pub mod deployment_verify;
pub mod doc_generator;
pub mod docs;
pub mod feature_flags;
pub mod gas_analyzer;
pub mod governance;
pub mod hardware_wallet;
pub mod history;
pub mod horizon;
pub mod http_client;
pub mod logging;
pub mod mnemonic;
pub mod mock_soroban;
pub mod multi_network_deploy;
pub mod multisig;
pub mod multisig_builder;
pub mod network_simulator;
pub mod node;
pub mod notifications;
pub mod ollama;
pub mod optimizer;
pub mod performance;
pub mod pipeline_builder;
pub mod print;
pub mod privacy;
pub mod profiler;
pub mod registry;
pub mod repl;
pub mod rollback_testing;
pub mod sandbox;
pub mod scheduler;
pub mod security;
pub mod security_scanner;
pub mod social;
pub mod soroban;
pub mod stream;
pub mod telemetry;
pub mod template;
pub mod template_security_scanner;
pub mod template_vcs;
pub mod templates;
pub mod test_automation;
pub mod test_coverage;
pub mod test_generator;
pub mod test_runner;
pub mod testnet_integration;
pub mod tutorial_engine;
pub mod tx_batch;
pub mod wallet_signer;

/// Serialises unit tests that replace or depend on the process-wide `HOME`.
///
/// `std::env::set_var` affects every thread, while libtest runs unit tests on
/// parallel threads. Without this lock a test that repoints `HOME` at a temp
/// dir silently redirects any concurrent test resolving a path under the real
/// home — which is how `performance::test_detect_regression` used to read back
/// an empty history it had just written.
#[cfg(test)]
pub(crate) fn lock_home_env() -> std::sync::MutexGuard<'static, ()> {
    static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    HOME_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
