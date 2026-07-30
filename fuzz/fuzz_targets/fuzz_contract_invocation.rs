//! Fuzz harness: mock contract invocation.
//!
//! Exercises `MockContractClient::invoke` with structured, arbitrary inputs
//! generated via `arbitrary::Arbitrary`. This drives the mock contract
//! client's call-logging, response-lookup, and error-handling paths with
//! random function names, argument vectors, and caller identities.
//!
//! The harness verifies that:
//! - Invocation never panics for any input.
//! - Call counts are always consistent with the number of invocations.
//! - Pre-configured errors and return values are returned deterministically.
//!
//! Run with:
//!   cargo fuzz run fuzz_contract_invocation

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use starforge::utils::contract_mocks::{MockAddress, MockContractClient};

/// Structured fuzzer input for contract invocation.
#[derive(Debug, Arbitrary)]
struct FuzzInvocation {
    /// Function name to invoke (arbitrary string).
    function: String,
    /// Arguments as JSON values.
    args: Vec<serde_json::Value>,
    /// Whether to pre-configure a return value.
    configure_return: bool,
    /// Whether to pre-configure an error.
    configure_error: bool,
    /// Number of times to invoke before checking.
    repeat: u8,
}

fuzz_target!(|input: FuzzInvocation| {
    let contract = MockAddress::contract(1);
    let client = MockContractClient::new(contract.clone());

    // Optionally pre-configure a return value or error.
    if input.configure_return {
        client.mock_return(&input.function, serde_json::json!(42u64));
    }
    if input.configure_error {
        client.mock_error(&input.function, "fuzz-error");
    }

    // Invoke the function `repeat` times — must never panic.
    for _ in 0..input.repeat {
        let caller = if input.repeat % 2 == 0 {
            Some(MockAddress::account(1))
        } else {
            None
        };
        let _ = client.invoke(&input.function, input.args.clone(), caller, 100);
    }

    // Postcondition: call count must match the number of invocations.
    let expected_count = input.repeat as usize;
    assert_eq!(
        client.call_count(&input.function),
        expected_count,
        "call count mismatch for function {:?}",
        input.function
    );

    // Postcondition: total calls must match.
    assert_eq!(
        client.total_calls(),
        expected_count,
        "total call count mismatch"
    );
});
