use starforge::utils::multisig_builder::{
    generate_signature, proposal_from_template, render_progress_bar, template_definitions, Proposal,
};

#[test]
fn templates_create_proposals_with_metadata() {
    let templates = template_definitions();
    assert!(templates.iter().any(|template| template.name == "escrow"));

    let proposal = proposal_from_template("escrow").unwrap();
    assert_eq!(proposal.threshold, 2);
    assert_eq!(proposal.signers, vec!["buyer", "seller", "arbiter"]);
    assert_eq!(proposal.network, "testnet");
    assert_eq!(
        proposal.metadata.transaction_type.as_deref(),
        Some("escrow")
    );
}

#[test]
fn progress_tracks_valid_signatures_and_pending_signers() {
    let mut proposal = Proposal::new(
        2,
        vec!["alice".to_string(), "bob".to_string(), "carol".to_string()],
        "testnet".to_string(),
    );

    let signature = generate_signature(&proposal.id, "alice").unwrap();
    proposal
        .add_signature_checked("alice".to_string(), signature)
        .unwrap();

    assert_eq!(proposal.signatures.len(), 1);
    assert_eq!(proposal.threshold, 2);
    assert!(!proposal.is_complete());
    assert_eq!(proposal.pending_signers(), vec!["bob", "carol"]);

    let (_bar, percent) = render_progress_bar(proposal.signatures.len(), proposal.threshold);
    assert_eq!(percent, 50);
}

#[test]
fn signature_validation_rejects_invalid_and_duplicate_signatures() {
    let mut proposal = Proposal::new(
        2,
        vec!["alice".to_string(), "bob".to_string()],
        "testnet".to_string(),
    );

    let alice_signature = generate_signature(&proposal.id, "alice").unwrap();
    proposal
        .add_signature_checked("alice".to_string(), alice_signature)
        .unwrap();
    assert!(proposal
        .add_signature_checked("alice".to_string(), "duplicate".to_string())
        .is_err());

    proposal.add_signature("bob".to_string(), "not-a-valid-signature".to_string());

    assert_eq!(proposal.signatures.len(), 2);
}

#[test]
fn validation_marks_ready_when_threshold_is_met() {
    let mut proposal = Proposal::new(
        2,
        vec!["alice".to_string(), "bob".to_string(), "carol".to_string()],
        "testnet".to_string(),
    );

    let alice_signature = generate_signature(&proposal.id, "alice").unwrap();
    proposal
        .add_signature_checked("alice".to_string(), alice_signature)
        .unwrap();
    let bob_signature = generate_signature(&proposal.id, "bob").unwrap();
    proposal
        .add_signature_checked("bob".to_string(), bob_signature)
        .unwrap();

    assert!(proposal.is_complete());
    let (_bar, percent) = render_progress_bar(proposal.signatures.len(), proposal.threshold);
    assert_eq!(percent, 100);
}
