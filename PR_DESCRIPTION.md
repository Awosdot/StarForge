## Summary

This PR implements four AI-driven features for StarForge to enhance security, deployment optimization, multi-network support, and automation capabilities.

### Implemented Features

**#533 AI Template Security Scanning**
- Added comprehensive security scanning for Soroban contract templates
- Detects vulnerabilities: reentrancy, missing authorization, integer overflow, code injection, access control, cryptographic issues, data leakage
- Malicious code detection with pattern matching
- Security anti-pattern detection (unwrap usage, expect usage)
- Security scoring (0-100) and risk level assessment
- Fix suggestions with code examples and implementation effort estimates
- Continuous monitoring configuration
- CLI command: `starforge template-security scan <path>`

**#543 AI Deployment Optimization**
- AI-driven optimization for deployment processes
- Cost optimization: gas cost estimation and savings analysis
- Speed optimization: deployment time estimation and improvement
- Reliability optimization: retry logic and pre-deployment validation suggestions
- Resource utilization analysis (CPU, memory, network, storage)
- Network selection recommendations with cost/speed/reliability scores
- Batch optimization for multiple deployments
- Scheduling optimization with optimal deployment times
- CLI command: `starforge deployment-optimize analyze --wasm <file>`

**#550 AI Deployment Multi-Network Support**
- AI-driven multi-network deployment support (testnet, mainnet, custom networks)
- Network-specific configuration management
- Cross-network deployment with parallel/sequential/testnet-first strategies
- Network comparison with cost, speed, and reliability metrics
- Cost optimization across networks
- Risk assessment before deployment
- Synchronization status tracking across networks
- CLI commands: `starforge multi-network deploy`, `compare`, `add-network`, `list-networks`, `switch`

**#540 AI Deployment Automation**
- AI-driven automation for deployment processes
- Pre-deployment validation: WASM file checks, size validation, network connectivity, wallet balance
- Automated testing with coverage reporting
- Deployment execution with gas estimation
- Post-deployment verification: contract inspection, storage verification
- Automated rollback on failure
- Monitoring setup with event monitoring and alert thresholds
- Complete automation pipeline with configurable levels (basic, standard, full)
- CLI command: `starforge deployment-automate run --wasm <file>`

### Code Changes

- Created `src/utils/template_security_scanner.rs` - Security scanning engine
- Created `src/commands/template_security.rs` - CLI commands for security scanning
- Created `src/utils/deployment_optimizer.rs` - Deployment optimization engine
- Created `src/commands/deployment_optimize.rs` - CLI commands for optimization
- Created `src/utils/multi_network_deploy.rs` - Multi-network deployment engine
- Created `src/commands/multi_network.rs` - CLI commands for multi-network
- Created `src/utils/deployment_automation.rs` - Deployment automation engine
- Created `src/commands/deployment_automate.rs` - CLI commands for automation
- Updated `src/commands/mod.rs` - Added new command modules
- Updated `src/utils/mod.rs` - Added new utility modules
- Updated `src/main.rs` - Integrated new CLI commands

## Test plan

- [ ] `cargo test` - Run all unit tests
- [ ] `starforge template-security scan <template_path>`
- [ ] `starforge deployment-optimize analyze --wasm <wasm_file>`
- [ ] `starforge multi-network deploy --wasm <wasm_file> --networks testnet,mainnet`
- [ ] `starforge multi-network compare`
- [ ] `starforge deployment-automate run --wasm <wasm_file> --network testnet`

## Related Issues

close #533 AI Template Security Scanning
close #543 AI Deployment Optimization
close #550 AI Deployment Multi-Network Support
close #540 AI Deployment Automation
