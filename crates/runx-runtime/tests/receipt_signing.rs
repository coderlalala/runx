use std::error::Error;

use runx_contracts::{
    HarnessReceipt, JsonObject, ReceiptIssuer, ReceiptIssuerType, ReceiptSignature,
    SignatureAlgorithm,
};
use runx_core::state_machine::StepAdmissionWitness;
use runx_receipts::{
    ReceiptFindingCode, ReceiptProofContext, ReceiptVerification, canonical_receipt_body_digest,
    validate_harness_receipt_proof, verify_harness_receipt_proof,
};
use runx_runtime::receipts::{
    Ed25519ReceiptSigner, Ed25519ReceiptVerifier, ProductionReceiptKey,
    RuntimeReceiptSignaturePolicy, RuntimeReceiptSigner, RuntimeReceiptSigningError,
    graph_receipt_with_signature_policy, step_receipt_with_signature_policy,
};
use runx_runtime::{InvocationStatus, SkillOutput, StepRun};

const CREATED_AT: &str = "2026-05-22T00:00:00Z";
const FIXTURE_KID: &str = "runx-runtime-prod-fixture-key";
const FIXTURE_SEED: [u8; 32] = [0x42; 32];

#[test]
fn production_step_receipt_uses_real_ed25519_signature() -> Result<(), Box<dyn Error>> {
    let signer = fixture_signer()?;
    let verifier = fixture_verifier(&signer);
    let receipt = production_step_receipt(&signer, &verifier)?;

    assert_eq!(receipt.issuer.kid, FIXTURE_KID);
    assert_eq!(
        receipt.issuer.public_key_sha256,
        signer.production_key().public_key_sha256()
    );
    assert!(receipt.signature.value.starts_with("base64:"));
    assert!(!receipt.signature.value.starts_with("sig:"));
    assert!(!serde_json::to_string(&receipt)?.contains("QkJCQkJC"));
    assert!(validate_harness_receipt_proof(&receipt, &proof_context(&verifier)).is_ok());
    Ok(())
}

#[test]
fn production_graph_receipt_resigns_children_and_verifies_tree() -> Result<(), Box<dyn Error>> {
    let signer = fixture_signer()?;
    let verifier = fixture_verifier(&signer);
    let mut steps = vec![production_step_run(
        "prod_graph",
        "plan",
        &signer,
        &verifier,
    )?];

    let graph = graph_receipt_with_signature_policy(
        "prod_graph",
        &mut steps,
        Vec::new(),
        CREATED_AT,
        RuntimeReceiptSignaturePolicy::production_signing(&signer, &verifier),
    )?;
    let children = steps
        .iter()
        .map(|step| step.receipt.clone())
        .collect::<Vec<_>>();

    assert!(graph.signature.value.starts_with("base64:"));
    assert!(children[0].signature.value.starts_with("base64:"));
    assert_eq!(
        children[0].harness.parent_harness_ref.as_ref(),
        Some(&graph.harness.harness_ref)
    );
    assert!(
        runx_runtime::receipt_tree::validate_runtime_receipt_tree_with_policy(
            &graph,
            children,
            runx_receipts::ReceiptTreeConfig::default(),
            RuntimeReceiptSignaturePolicy::production(&verifier),
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn production_sealing_fails_closed_without_signer_or_verifier() -> Result<(), Box<dyn Error>> {
    let signer = fixture_signer()?;
    let verifier = fixture_verifier(&signer);

    let Err(missing_signer) = step_receipt_with_signature_policy(
        "prod_missing",
        "signer",
        1,
        &skill_output(InvocationStatus::Success),
        CREATED_AT,
        RuntimeReceiptSignaturePolicy::production(&verifier),
    ) else {
        return Err("production sealing without signer must fail".into());
    };
    assert!(missing_signer.to_string().contains("requires a signer"));

    let Err(missing_verifier) = step_receipt_with_signature_policy(
        "prod_missing",
        "verifier",
        1,
        &skill_output(InvocationStatus::Success),
        CREATED_AT,
        RuntimeReceiptSignaturePolicy::production_signing_without_verifier(&signer),
    ) else {
        return Err("production sealing without verifier must fail".into());
    };
    assert!(missing_verifier.to_string().contains("requires a verifier"));
    Ok(())
}

#[test]
fn production_sealing_rejects_missing_issuer_metadata() -> Result<(), Box<dyn Error>> {
    let signer = fixture_signer()?;
    let verifier = fixture_verifier(&signer);
    let missing_kid = FixedSigner {
        issuer: ReceiptIssuer {
            issuer_type: ReceiptIssuerType::Local,
            kid: String::new(),
            public_key_sha256: signer.production_key().public_key_sha256(),
        },
    };
    let missing_hash = FixedSigner {
        issuer: ReceiptIssuer {
            issuer_type: ReceiptIssuerType::Local,
            kid: FIXTURE_KID.to_owned(),
            public_key_sha256: String::new(),
        },
    };

    let Err(missing_kid_error) = sign_with_fixed_signer(&missing_kid, &verifier) else {
        return Err("production sealing without kid must fail".into());
    };
    assert!(missing_kid_error.to_string().contains("key id is missing"));

    let Err(missing_hash_error) = sign_with_fixed_signer(&missing_hash, &verifier) else {
        return Err("production sealing without public key hash must fail".into());
    };
    assert!(
        missing_hash_error
            .to_string()
            .contains("public key hash is missing")
    );
    Ok(())
}

#[test]
fn production_verifier_reports_tamper_findings() -> Result<(), Box<dyn Error>> {
    let signer = fixture_signer()?;
    let verifier = fixture_verifier(&signer);
    let receipt = production_step_receipt(&signer, &verifier)?;

    let mut tampered_body = receipt.clone();
    tampered_body.harness.acts[0].summary = "tampered body".to_owned();
    let verification = verify_harness_receipt_proof(&tampered_body, &proof_context(&verifier));
    assert_finding(&verification, ReceiptFindingCode::SealDigestMismatch);
    assert_finding(&verification, ReceiptFindingCode::SignatureInvalid);

    let mut tampered_seal = receipt.clone();
    tampered_seal.seal.digest = format!("sha256:{}", "0".repeat(64));
    if let Some(seal) = tampered_seal.harness.seal.as_mut() {
        seal.digest = tampered_seal.seal.digest.clone();
    }
    let verification = verify_harness_receipt_proof(&tampered_seal, &proof_context(&verifier));
    assert_finding(&verification, ReceiptFindingCode::SealDigestMismatch);

    let mut malformed_signature = receipt.clone();
    malformed_signature.signature.value = "base64:!".to_owned();
    let verification =
        verify_harness_receipt_proof(&malformed_signature, &proof_context(&verifier));
    assert_finding(&verification, ReceiptFindingCode::SignatureMalformed);

    let mut missing_key = receipt.clone();
    missing_key.issuer.kid = "missing-key".to_owned();
    let verification = verify_harness_receipt_proof(&missing_key, &proof_context(&verifier));
    assert_finding(&verification, ReceiptFindingCode::SignatureKeyMissing);

    let mut hash_mismatch = receipt.clone();
    hash_mismatch.issuer.public_key_sha256 = format!("sha256:{}", "1".repeat(64));
    let verification = verify_harness_receipt_proof(&hash_mismatch, &proof_context(&verifier));
    assert_finding(&verification, ReceiptFindingCode::SignatureKeyHashMismatch);

    let mut missing_verifier = receipt;
    refresh_digest_and_signature(&mut missing_verifier, &signer)?;
    let verification =
        verify_harness_receipt_proof(&missing_verifier, &ReceiptProofContext::default());
    assert_finding(&verification, ReceiptFindingCode::SignatureVerifierMissing);
    Ok(())
}

#[test]
fn production_verifier_rejects_pseudo_and_malformed_public_key() -> Result<(), Box<dyn Error>> {
    let signer = fixture_signer()?;
    let verifier = fixture_verifier(&signer);
    let mut receipt = production_step_receipt(&signer, &verifier)?;

    let digest = canonical_receipt_body_digest(&receipt)?;
    receipt.signature.value = format!("sig:{digest}");
    let verification = verify_harness_receipt_proof(&receipt, &proof_context(&verifier));
    assert_finding(&verification, ReceiptFindingCode::SignatureMalformed);

    let bad_key = ProductionReceiptKey::new(FIXTURE_KID, vec![0x99; 31]);
    let bad_verifier = Ed25519ReceiptVerifier::new([bad_key]);
    let verification = verify_harness_receipt_proof(&receipt, &proof_context(&bad_verifier));
    assert_finding(&verification, ReceiptFindingCode::SignatureKeyMalformed);
    Ok(())
}

fn production_step_receipt(
    signer: &Ed25519ReceiptSigner,
    verifier: &Ed25519ReceiptVerifier,
) -> Result<HarnessReceipt, Box<dyn Error>> {
    Ok(step_receipt_with_signature_policy(
        "prod_step",
        "seal",
        1,
        &skill_output(InvocationStatus::Success),
        CREATED_AT,
        RuntimeReceiptSignaturePolicy::production_signing(signer, verifier),
    )?)
}

fn production_step_run(
    graph_name: &str,
    step_id: &str,
    signer: &Ed25519ReceiptSigner,
    verifier: &Ed25519ReceiptVerifier,
) -> Result<StepRun, Box<dyn Error>> {
    let output = skill_output(InvocationStatus::Success);
    let receipt = step_receipt_with_signature_policy(
        graph_name,
        step_id,
        1,
        &output,
        CREATED_AT,
        RuntimeReceiptSignaturePolicy::production_signing(signer, verifier),
    )?;
    Ok(StepRun {
        step_id: step_id.to_owned(),
        attempt: 1,
        skill: step_id.to_owned(),
        runner: None,
        fanout_group: None,
        output,
        outputs: JsonObject::new(),
        receipt,
        admission_witness: StepAdmissionWitness::local_runtime(step_id, "receipt"),
    })
}

fn fixture_signer() -> Result<Ed25519ReceiptSigner, RuntimeReceiptSigningError> {
    Ed25519ReceiptSigner::from_seed(FIXTURE_KID, ReceiptIssuerType::Local, &FIXTURE_SEED)
}

fn fixture_verifier(signer: &Ed25519ReceiptSigner) -> Ed25519ReceiptVerifier {
    Ed25519ReceiptVerifier::new([signer.production_key()])
}

fn proof_context(verifier: &Ed25519ReceiptVerifier) -> ReceiptProofContext<'_> {
    ReceiptProofContext {
        signature_verifier: Some(verifier),
        authority_verified: false,
        external_attestations_verified: true,
        verified_redaction_refs: Default::default(),
        verified_hash_commitments: Default::default(),
    }
}

fn skill_output(status: InvocationStatus) -> SkillOutput {
    let (stdout, stderr, exit_code) = match status {
        InvocationStatus::Success => ("ok".to_owned(), String::new(), Some(0)),
        InvocationStatus::Failure => (String::new(), "failed".to_owned(), Some(1)),
    };
    SkillOutput {
        status,
        stdout,
        stderr,
        exit_code,
        duration_ms: 1,
        metadata: JsonObject::new(),
    }
}

fn refresh_digest_and_signature(
    receipt: &mut HarnessReceipt,
    signer: &Ed25519ReceiptSigner,
) -> Result<(), Box<dyn Error>> {
    let digest = canonical_receipt_body_digest(receipt)?;
    receipt.seal.digest = digest.clone();
    if let Some(seal) = receipt.harness.seal.as_mut() {
        seal.digest = digest.clone();
    }
    receipt.signature = signer.sign_receipt_body(&digest)?;
    Ok(())
}

fn sign_with_fixed_signer(
    signer: &FixedSigner,
    verifier: &Ed25519ReceiptVerifier,
) -> Result<HarnessReceipt, runx_runtime::RuntimeError> {
    step_receipt_with_signature_policy(
        "prod_bad_metadata",
        "seal",
        1,
        &skill_output(InvocationStatus::Success),
        CREATED_AT,
        RuntimeReceiptSignaturePolicy::production_signing(signer, verifier),
    )
}

struct FixedSigner {
    issuer: ReceiptIssuer,
}

impl RuntimeReceiptSigner for FixedSigner {
    fn issuer(&self) -> ReceiptIssuer {
        self.issuer.clone()
    }

    fn sign_receipt_body(
        &self,
        _body_digest: &str,
    ) -> Result<ReceiptSignature, RuntimeReceiptSigningError> {
        Ok(ReceiptSignature {
            alg: SignatureAlgorithm::Ed25519,
            value: "base64:fixed-signature".to_owned(),
        })
    }
}

fn assert_finding(verification: &ReceiptVerification, code: ReceiptFindingCode) {
    assert!(
        verification
            .findings
            .iter()
            .any(|finding| finding.code == code),
        "expected finding {code:?}; got {:?}",
        verification.findings
    );
}
