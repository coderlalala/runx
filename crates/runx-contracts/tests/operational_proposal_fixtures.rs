use serde::Deserialize;

use runx_contracts::OperationalProposal;

const POSITIVE_FIXTURES: &[&str] = &[
    include_str!("../../../fixtures/contracts/operational-proposal/proposal-prepared.json"),
    include_str!("../../../fixtures/contracts/operational-proposal/proposal-blocked.json"),
];

const INVALID_FIXTURES: &[&str] = &[
    include_str!("../../../fixtures/contracts/operational-proposal/invalid-authority-claim.json"),
    include_str!("../../../fixtures/contracts/operational-proposal/invalid-missing-redaction.json"),
    include_str!(
        "../../../fixtures/contracts/operational-proposal/invalid-missing-source-ref.json"
    ),
    include_str!(
        "../../../fixtures/contracts/operational-proposal/invalid-provider-specific-field.json"
    ),
    include_str!(
        "../../../fixtures/contracts/operational-proposal/invalid-product-specific-field.json"
    ),
];

#[derive(Debug, Deserialize)]
struct Fixture {
    expected: serde_json::Value,
}

#[test]
fn operational_proposal_fixtures_match_wire_shape() -> Result<(), serde_json::Error> {
    for fixture_json in POSITIVE_FIXTURES {
        let fixture: Fixture = serde_json::from_str(fixture_json)?;
        let parsed: OperationalProposal = serde_json::from_value(fixture.expected.clone())?;
        let actual = serde_json::to_value(parsed)?;
        assert_eq!(actual, fixture.expected);
    }
    Ok(())
}

#[test]
fn operational_proposal_rejects_invalid_public_shapes() -> Result<(), serde_json::Error> {
    for fixture_json in INVALID_FIXTURES {
        let fixture: Fixture = serde_json::from_str(fixture_json)?;
        assert!(serde_json::from_value::<OperationalProposal>(fixture.expected).is_err());
    }
    Ok(())
}
