use runx_contracts::{JsonValue, Receipt, sha256_prefixed};

use crate::ReceiptError;

pub fn canonical_receipt_json(receipt: &Receipt) -> Result<String, ReceiptError> {
    let value = receipt_value(receipt)?;
    canonical_json_value(&value)
}

pub fn canonical_receipt_digest(receipt: &Receipt) -> Result<String, ReceiptError> {
    canonical_receipt_json(receipt).map(|json| sha256_prefixed(json.as_bytes()))
}

pub fn canonical_receipt_body_json(receipt: &Receipt) -> Result<String, ReceiptError> {
    let mut value = receipt_value(receipt)?;
    strip_body_proof_fields(&mut value);
    canonical_json_value(&value)
}

pub fn canonical_receipt_body_digest(receipt: &Receipt) -> Result<String, ReceiptError> {
    canonical_receipt_body_json(receipt).map(|json| sha256_prefixed(json.as_bytes()))
}

/// The canonical body that the content-addressed `id` commits: every intrinsic
/// run field except the envelope's `id` (which it derives), `signature`,
/// `digest`, the runtime-local `metadata` read aid, and `lineage`. `lineage` is
/// post-hoc graph wiring (parent/children refs) attached after the children's
/// own ids are known; excluding it breaks the parent<->child id circularity
/// while keeping the address stable. The full `digest` still commits `lineage`.
pub fn canonical_receipt_identity_json(receipt: &Receipt) -> Result<String, ReceiptError> {
    let mut value = receipt_value(receipt)?;
    strip_body_proof_fields(&mut value);
    if let JsonValue::Object(map) = &mut value {
        map.remove("id");
        map.remove("lineage");
    }
    canonical_json_value(&value)
}

/// `id = hash(canonical_body)` under `runx.receipt.c14n.v1`: the content address
/// of this receipt. References to a receipt use this id.
pub fn content_addressed_receipt_id(receipt: &Receipt) -> Result<String, ReceiptError> {
    canonical_receipt_identity_json(receipt).map(|json| sha256_prefixed(json.as_bytes()))
}

fn receipt_value(receipt: &Receipt) -> Result<JsonValue, ReceiptError> {
    let serialized =
        serde_json::to_string(receipt).map_err(|source| ReceiptError::Serialization {
            message: source.to_string(),
        })?;
    serde_json::from_str(&serialized).map_err(|source| ReceiptError::Serialization {
        message: source.to_string(),
    })
}

/// The signed body commits every flat field except the envelope's own
/// `signature` and `digest`. `metadata` is a runtime-local read aid and is not
/// part of the signed body.
fn strip_body_proof_fields(value: &mut JsonValue) {
    if let JsonValue::Object(map) = value {
        map.remove("signature");
        map.remove("digest");
        map.remove("metadata");
    }
}

fn canonical_json_value(value: &JsonValue) -> Result<String, ReceiptError> {
    let mut output = String::new();
    write_canonical_json_value(value, &mut output)?;
    Ok(output)
}

fn write_canonical_json_value(value: &JsonValue, output: &mut String) -> Result<(), ReceiptError> {
    match value {
        JsonValue::Null => output.push_str("null"),
        JsonValue::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        JsonValue::Number(value) => output.push_str(&value.to_string()),
        JsonValue::String(value) => {
            write_json_string(value, output)?;
        }
        JsonValue::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical_json_value(item, output)?;
            }
            output.push(']');
        }
        JsonValue::Object(map) => {
            output.push('{');
            for (index, (key, value)) in map.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_json_string(key, output)?;
                output.push(':');
                write_canonical_json_value(value, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn write_json_string(value: &str, output: &mut String) -> Result<(), ReceiptError> {
    let encoded = serde_json::to_string(value).map_err(|source| ReceiptError::Serialization {
        message: source.to_string(),
    })?;
    output.push_str(&encoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use runx_contracts::JsonValue;
    use runx_contracts::Receipt;
    use serde::Deserialize;

    use super::{
        ReceiptError, canonical_receipt_body_digest, canonical_receipt_body_json,
        canonical_receipt_digest, canonical_receipt_json, sha256_prefixed,
    };

    const SUCCESS_RECEIPT: &str =
        include_str!("../../../fixtures/contracts/harness-spine/receipt-success.json");
    const ABNORMAL_RECEIPT: &str =
        include_str!("../../../fixtures/contracts/harness-spine/receipt-abnormal.json");
    const POST_MERGE_OBSERVER_RECEIPT: &str = include_str!(
        "../../../fixtures/contracts/harness-spine/post-merge-observer-merged-verified.json"
    );
    const RECEIPT_ORACLE: &str = include_str!(
        "../../../fixtures/contracts/canonical-json/runx-receipt-c14n-v1.oracles.json"
    );

    #[derive(Debug, Deserialize)]
    struct Fixture {
        expected: Receipt,
    }

    #[derive(Debug, Deserialize)]
    struct ReceiptOracleFixture {
        canonicalization: String,
        cases: Vec<ReceiptOracleCase>,
    }

    #[derive(Debug, Deserialize)]
    struct ReceiptOracleCase {
        name: String,
        fixture: String,
        full_canonical_json: String,
        full_sha256: String,
        body_canonical_json: String,
        body_sha256: String,
    }

    #[test]
    fn sha256_prefixes_digest() {
        assert_eq!(
            sha256_prefixed(b"runx"),
            "sha256:8186b7035bea2f66ebe27c1f5cf7de4e94ef935e259a2f3160352adffc752f28"
        );
    }

    #[test]
    fn canonical_receipt_json_is_stable_and_sorted() -> Result<(), ReceiptError> {
        let receipt = fixture()?;
        let first = canonical_receipt_json(&receipt)?;
        let second = canonical_receipt_json(&receipt)?;

        assert_eq!(first, second);
        assert!(first.contains("\"created_at\":\""));
        assert!(canonical_receipt_digest(&receipt)?.starts_with("sha256:"));
        Ok(())
    }

    #[test]
    fn body_commitment_excludes_signature_and_seal_derived_fields() -> Result<(), ReceiptError> {
        let mut receipt = fixture()?;
        let baseline_json = canonical_receipt_body_json(&receipt)?;
        let baseline_digest = canonical_receipt_body_digest(&receipt)?;

        receipt.signature.value = "base64:changed".into();
        receipt.digest = "sha256:changed".into();

        assert_eq!(canonical_receipt_body_json(&receipt)?, baseline_json);
        assert_eq!(canonical_receipt_body_digest(&receipt)?, baseline_digest);
        Ok(())
    }

    #[test]
    fn body_commitment_excludes_metadata_read_aid() -> Result<(), ReceiptError> {
        let mut receipt = fixture()?;
        let baseline_digest = canonical_receipt_body_digest(&receipt)?;

        receipt.metadata.get_or_insert_default().insert(
            "skill_name".to_owned(),
            JsonValue::String("changed-read-aid".to_owned()),
        );

        assert_eq!(canonical_receipt_body_digest(&receipt)?, baseline_digest);
        Ok(())
    }

    #[test]
    fn receipt_oracle_matches_rust_canonical_json() -> Result<(), ReceiptError> {
        let oracle: ReceiptOracleFixture =
            serde_json::from_str(RECEIPT_ORACLE).map_err(|source| ReceiptError::Serialization {
                message: source.to_string(),
            })?;
        assert_eq!(oracle.canonicalization, "runx.receipt.c14n.v1");

        for case in oracle.cases {
            let receipt = fixture_by_path(&case.fixture)?;
            assert_eq!(
                canonical_receipt_json(&receipt)?,
                case.full_canonical_json,
                "{} full canonical JSON drifted",
                case.name
            );
            assert_eq!(
                canonical_receipt_digest(&receipt)?,
                case.full_sha256,
                "{} full digest drifted",
                case.name
            );
            assert_eq!(
                canonical_receipt_body_json(&receipt)?,
                case.body_canonical_json,
                "{} body canonical JSON drifted",
                case.name
            );
            assert_eq!(
                canonical_receipt_body_digest(&receipt)?,
                case.body_sha256,
                "{} body digest drifted",
                case.name
            );
        }
        Ok(())
    }

    fn fixture() -> Result<Receipt, ReceiptError> {
        serde_json::from_str::<Fixture>(SUCCESS_RECEIPT)
            .map(|fixture| fixture.expected)
            .map_err(|source| ReceiptError::Serialization {
                message: source.to_string(),
            })
    }

    fn fixture_by_path(path: &str) -> Result<Receipt, ReceiptError> {
        let json = match path {
            "harness-spine/receipt-abnormal.json" => ABNORMAL_RECEIPT,
            "harness-spine/receipt-success.json" => SUCCESS_RECEIPT,
            "harness-spine/post-merge-observer-merged-verified.json" => POST_MERGE_OBSERVER_RECEIPT,
            _ => {
                return Err(ReceiptError::Serialization {
                    message: format!("unknown receipt oracle fixture: {path}"),
                });
            }
        };
        serde_json::from_str::<Fixture>(json)
            .map(|fixture| fixture.expected)
            .map_err(|source| ReceiptError::Serialization {
                message: source.to_string(),
            })
    }
}
