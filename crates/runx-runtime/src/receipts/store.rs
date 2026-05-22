// rust-style-allow: large-file -- local store read/write/index semantics stay
// together until the receipt-store API finishes the hard-cutover review.
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use runx_contracts::{HARNESS_RECEIPT_SCHEMA, HarnessReceipt};
use runx_receipts::{ReceiptProofContextProvider, verify_harness_receipt_proof};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::paths::{
    ReceiptStoreLabel, ReceiptStorePublicProjection, safe_receipt_store_projection,
};
use super::seal::{RuntimeReceiptProofContextProvider, RuntimeReceiptSignaturePolicy};

const RECEIPT_STORE_INDEX_SCHEMA: &str = "runx.receipt_store_index.v1";
const INDEX_FILE_NAME: &str = "index.json";

#[derive(Clone, Debug)]
pub struct LocalReceiptStore {
    root: PathBuf,
}

impl LocalReceiptStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn public_projection(
        &self,
        workspace_base: &Path,
        project_runx_dir: &Path,
    ) -> ReceiptStorePublicProjection {
        safe_receipt_store_projection(&self.root, workspace_base, project_runx_dir)
    }

    pub fn read_exact(&self, receipt_id: &str) -> Result<HarnessReceipt, ReceiptStoreError> {
        self.read_exact_with_policy(
            receipt_id,
            RuntimeReceiptSignaturePolicy::local_development(),
        )
    }

    pub fn read_exact_with_policy(
        &self,
        receipt_id: &str,
        signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    ) -> Result<HarnessReceipt, ReceiptStoreError> {
        let file_name = receipt_file_name(receipt_id)?;
        self.ensure_store_dir()?;
        read_receipt_file(&self.root.join(file_name), receipt_id, signature_policy)
    }

    pub fn write_receipt(&self, receipt: &HarnessReceipt) -> Result<(), ReceiptStoreError> {
        self.write_receipt_with_policy(receipt, RuntimeReceiptSignaturePolicy::local_development())
    }

    pub fn write_receipt_with_policy(
        &self,
        receipt: &HarnessReceipt,
        signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    ) -> Result<(), ReceiptStoreError> {
        let file_name = receipt_file_name(&receipt.id)?;
        self.ensure_or_create_store_dir()?;
        let file_path = self.root.join(&file_name);
        let contents =
            serde_json::to_vec(receipt).map_err(|source| ReceiptStoreError::MalformedReceipt {
                path: file_path.clone(),
                message: source.to_string(),
            })?;

        if file_path.exists() {
            let existing =
                fs::read(&file_path).map_err(|source| ReceiptStoreError::ReceiptUnreadable {
                    path: file_path.clone(),
                    source,
                })?;
            if existing == contents {
                verify_receipt_proof(&file_path, receipt, signature_policy)?;
                return Ok(());
            }
            return Err(ReceiptStoreError::ReceiptAlreadyExists {
                receipt_id: receipt.id.clone(),
            });
        }

        verify_receipt_proof(&file_path, receipt, signature_policy)?;
        write_atomic(&self.root, &file_name, &contents)?;
        match self.rebuild_index_with_policy(signature_policy) {
            Ok(_) => Ok(()),
            Err(error) => Err(ReceiptStoreError::ReceiptIndexStale {
                path: self.index_path(),
                message: error.to_string(),
            }),
        }
    }

    pub fn list(&self) -> Result<Vec<HarnessReceipt>, ReceiptStoreError> {
        self.list_with_policy(RuntimeReceiptSignaturePolicy::local_development())
    }

    pub fn list_with_policy(
        &self,
        signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    ) -> Result<Vec<HarnessReceipt>, ReceiptStoreError> {
        self.ensure_store_dir()?;
        let mut receipts = Vec::new();
        for entry in
            fs::read_dir(&self.root).map_err(|source| ReceiptStoreError::StoreUnreadable {
                path: self.root.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| ReceiptStoreError::StoreUnreadable {
                path: self.root.clone(),
                source,
            })?;
            let path = entry.path();
            if path.extension() != Some(OsStr::new("json"))
                || path.file_name() == Some(OsStr::new(INDEX_FILE_NAME))
            {
                continue;
            }
            let Some(receipt_id) = path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };
            receipts.push(read_receipt_file(&path, receipt_id, signature_policy)?);
        }
        receipts.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(receipts)
    }

    pub(crate) fn list_without_proof_for_history(
        &self,
    ) -> Result<Vec<HarnessReceipt>, ReceiptStoreError> {
        self.ensure_store_dir()?;
        let mut receipts = Vec::new();
        for entry in
            fs::read_dir(&self.root).map_err(|source| ReceiptStoreError::StoreUnreadable {
                path: self.root.clone(),
                source,
            })?
        {
            let entry = entry.map_err(|source| ReceiptStoreError::StoreUnreadable {
                path: self.root.clone(),
                source,
            })?;
            let path = entry.path();
            if path.extension() != Some(OsStr::new("json"))
                || path.file_name() == Some(OsStr::new(INDEX_FILE_NAME))
            {
                continue;
            }
            let Some(receipt_id) = path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };
            receipts.push(read_receipt_file_without_proof(&path, receipt_id)?);
        }
        receipts.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(receipts)
    }

    pub fn load_index(&self) -> Result<ReceiptStoreIndex, ReceiptStoreError> {
        self.load_index_with_policy(RuntimeReceiptSignaturePolicy::local_development())
    }

    pub fn load_index_with_policy(
        &self,
        signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    ) -> Result<ReceiptStoreIndex, ReceiptStoreError> {
        self.ensure_store_dir()?;
        let index_path = self.index_path();
        let contents = match fs::read_to_string(&index_path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == ErrorKind::NotFound => {
                return self.rebuild_index_with_policy(signature_policy);
            }
            Err(source) => {
                return Err(ReceiptStoreError::StoreUnreadable {
                    path: index_path,
                    source,
                });
            }
        };
        let index = serde_json::from_str::<ReceiptStoreIndex>(&contents).map_err(|source| {
            ReceiptStoreError::MalformedIndex {
                path: index_path.clone(),
                message: source.to_string(),
            }
        })?;
        if index.schema != RECEIPT_STORE_INDEX_SCHEMA {
            return Err(ReceiptStoreError::MalformedIndex {
                path: index_path,
                message: format!("unsupported index schema {}", index.schema),
            });
        }
        self.verify_index(&index, signature_policy)?;
        Ok(index)
    }

    pub fn rebuild_index(&self) -> Result<ReceiptStoreIndex, ReceiptStoreError> {
        self.rebuild_index_with_policy(RuntimeReceiptSignaturePolicy::local_development())
    }

    pub fn rebuild_index_with_policy(
        &self,
        signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    ) -> Result<ReceiptStoreIndex, ReceiptStoreError> {
        let entries = self
            .list_with_policy(signature_policy)?
            .into_iter()
            .map(|receipt| ReceiptStoreIndexEntry {
                receipt_id: receipt.id.clone(),
                file_name: format!("{}.json", receipt.id),
                created_at: receipt.created_at,
            })
            .collect::<Vec<_>>();
        let index = ReceiptStoreIndex {
            schema: RECEIPT_STORE_INDEX_SCHEMA.to_owned(),
            generated_at: generated_at_nanos(),
            entries,
        };
        self.write_index(&index)?;
        Ok(index)
    }

    fn verify_index(
        &self,
        index: &ReceiptStoreIndex,
        signature_policy: RuntimeReceiptSignaturePolicy<'_>,
    ) -> Result<(), ReceiptStoreError> {
        let listed = self.list_with_policy(signature_policy)?;
        let listed_ids = listed
            .iter()
            .map(|receipt| receipt.id.as_str())
            .collect::<Vec<_>>();
        let indexed_ids = index
            .entries
            .iter()
            .map(|entry| entry.receipt_id.as_str())
            .collect::<Vec<_>>();
        if listed_ids != indexed_ids {
            return Err(ReceiptStoreError::ReceiptIndexStale {
                path: self.index_path(),
                message: "index entries do not match receipt JSON files".to_owned(),
            });
        }
        for entry in &index.entries {
            let expected_file_name = receipt_file_name(&entry.receipt_id)?;
            if entry.file_name != expected_file_name {
                return Err(ReceiptStoreError::ReceiptIndexStale {
                    path: self.index_path(),
                    message: "index file name does not match receipt id".to_owned(),
                });
            }
            let receipt = self.read_exact_with_policy(&entry.receipt_id, signature_policy)?;
            if receipt.created_at != entry.created_at {
                return Err(ReceiptStoreError::ReceiptIndexStale {
                    path: self.index_path(),
                    message: "index created_at does not match receipt JSON".to_owned(),
                });
            }
        }
        Ok(())
    }

    fn write_index(&self, index: &ReceiptStoreIndex) -> Result<(), ReceiptStoreError> {
        let contents =
            serde_json::to_vec(index).map_err(|source| ReceiptStoreError::MalformedIndex {
                path: self.index_path(),
                message: source.to_string(),
            })?;
        write_atomic(&self.root, INDEX_FILE_NAME, &contents)
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(INDEX_FILE_NAME)
    }

    fn ensure_store_dir(&self) -> Result<(), ReceiptStoreError> {
        match fs::metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() => Ok(()),
            Ok(_) => Err(ReceiptStoreError::StoreNotDirectory {
                path: self.root.clone(),
            }),
            Err(source) if source.kind() == ErrorKind::NotFound => {
                Err(ReceiptStoreError::MissingStore {
                    path: self.root.clone(),
                })
            }
            Err(source) => Err(ReceiptStoreError::StoreUnreadable {
                path: self.root.clone(),
                source,
            }),
        }
    }

    fn ensure_or_create_store_dir(&self) -> Result<(), ReceiptStoreError> {
        match fs::metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() => Ok(()),
            Ok(_) => Err(ReceiptStoreError::StoreNotDirectory {
                path: self.root.clone(),
            }),
            Err(source) if source.kind() == ErrorKind::NotFound => fs::create_dir_all(&self.root)
                .map_err(|source| ReceiptStoreError::StoreUnreadable {
                    path: self.root.clone(),
                    source,
                }),
            Err(source) => Err(ReceiptStoreError::StoreUnreadable {
                path: self.root.clone(),
                source,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptStoreIndex {
    pub schema: String,
    pub generated_at: String,
    pub entries: Vec<ReceiptStoreIndexEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptStoreIndexEntry {
    pub receipt_id: String,
    pub file_name: String,
    pub created_at: String,
}

#[derive(Debug, Error)]
pub enum ReceiptStoreError {
    #[error("receipt store is missing")]
    MissingStore { path: PathBuf },
    #[error("receipt store path is not a directory")]
    StoreNotDirectory { path: PathBuf },
    #[error("receipt store is unreadable: {source}")]
    StoreUnreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("receipt id is invalid for local store lookup: {receipt_id}")]
    InvalidReceiptId { receipt_id: String },
    #[error("receipt is missing")]
    MissingReceipt { path: PathBuf },
    #[error("receipt is unreadable: {source}")]
    ReceiptUnreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("receipt JSON is malformed: {message}")]
    MalformedJson { path: PathBuf, message: String },
    #[error("receipt has unsupported schema: {schema}")]
    WrongSchema { path: PathBuf, schema: String },
    #[error("receipt shape is invalid: {message}")]
    MalformedReceipt { path: PathBuf, message: String },
    #[error("receipt id '{receipt_id}' does not match file name '{file_stem}'")]
    IdFilenameMismatch {
        path: PathBuf,
        receipt_id: String,
        file_stem: String,
    },
    #[error("receipt proof is invalid for {receipt_id}: {message}")]
    ReceiptProofInvalid {
        path: PathBuf,
        receipt_id: String,
        message: String,
    },
    #[error("receipt already exists with different content: {receipt_id}")]
    ReceiptAlreadyExists { receipt_id: String },
    #[error("receipt store index is malformed: {message}")]
    MalformedIndex { path: PathBuf, message: String },
    #[error("receipt store index is stale: {message}")]
    ReceiptIndexStale { path: PathBuf, message: String },
    #[error("receipt store path cannot be projected safely: {reason}")]
    UnsafePathProjection { reason: String },
}

impl ReceiptStoreError {
    #[must_use]
    pub fn public_message(&self, store_label: &ReceiptStoreLabel) -> String {
        match self {
            Self::MissingStore { .. } => format!("receipt store {store_label} is missing"),
            Self::StoreNotDirectory { .. } => {
                format!("receipt store {store_label} is not a directory")
            }
            Self::StoreUnreadable { .. } => format!("receipt store {store_label} is unreadable"),
            Self::InvalidReceiptId { .. } => {
                "receipt id is invalid for local store lookup".to_owned()
            }
            Self::MissingReceipt { .. } => format!("receipt is missing in store {store_label}"),
            Self::ReceiptUnreadable { .. } => {
                format!("receipt is unreadable in store {store_label}")
            }
            Self::MalformedJson { .. } => {
                format!("receipt JSON is malformed in store {store_label}")
            }
            Self::WrongSchema { schema, .. } => {
                format!("receipt has unsupported schema in store {store_label}: {schema}")
            }
            Self::MalformedReceipt { .. } => {
                format!("receipt shape is invalid in store {store_label}")
            }
            Self::IdFilenameMismatch { .. } => {
                format!("receipt id does not match file name in store {store_label}")
            }
            Self::ReceiptProofInvalid { .. } => {
                format!("receipt proof is invalid in store {store_label}")
            }
            Self::ReceiptAlreadyExists { .. } => {
                format!("receipt already exists with different content in store {store_label}")
            }
            Self::MalformedIndex { .. } => {
                format!("receipt store index is malformed in store {store_label}")
            }
            Self::ReceiptIndexStale { .. } => {
                format!("receipt store index is stale in store {store_label}")
            }
            Self::UnsafePathProjection { .. } => {
                "receipt store path cannot be projected safely".to_owned()
            }
        }
    }
}

fn receipt_file_name(receipt_id: &str) -> Result<String, ReceiptStoreError> {
    if receipt_id.is_empty()
        || receipt_id == "."
        || receipt_id == ".."
        || receipt_id.contains('/')
        || receipt_id.contains('\\')
    {
        return Err(ReceiptStoreError::InvalidReceiptId {
            receipt_id: receipt_id.to_owned(),
        });
    }
    Ok(format!("{receipt_id}.json"))
}

fn read_receipt_file(
    path: &Path,
    expected_id: &str,
    signature_policy: RuntimeReceiptSignaturePolicy<'_>,
) -> Result<HarnessReceipt, ReceiptStoreError> {
    let contents = fs::read_to_string(path).map_err(|source| {
        if source.kind() == ErrorKind::NotFound {
            ReceiptStoreError::MissingReceipt {
                path: path.to_path_buf(),
            }
        } else {
            ReceiptStoreError::ReceiptUnreadable {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    parse_receipt_contents(&contents, path, expected_id, signature_policy)
}

fn read_receipt_file_without_proof(
    path: &Path,
    expected_id: &str,
) -> Result<HarnessReceipt, ReceiptStoreError> {
    let contents = fs::read_to_string(path).map_err(|source| {
        if source.kind() == ErrorKind::NotFound {
            ReceiptStoreError::MissingReceipt {
                path: path.to_path_buf(),
            }
        } else {
            ReceiptStoreError::ReceiptUnreadable {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    parse_receipt_contents_without_proof(&contents, path, expected_id)
}

fn parse_receipt_contents(
    contents: &str,
    path: &Path,
    expected_id: &str,
    signature_policy: RuntimeReceiptSignaturePolicy<'_>,
) -> Result<HarnessReceipt, ReceiptStoreError> {
    let receipt = parse_receipt_contents_without_proof(contents, path, expected_id)?;
    verify_receipt_proof(path, &receipt, signature_policy)?;
    Ok(receipt)
}

fn parse_receipt_contents_without_proof(
    contents: &str,
    path: &Path,
    expected_id: &str,
) -> Result<HarnessReceipt, ReceiptStoreError> {
    let probe = serde_json::from_str::<ReceiptSchemaProbe>(contents).map_err(|source| {
        ReceiptStoreError::MalformedJson {
            path: path.to_path_buf(),
            message: source.to_string(),
        }
    })?;
    let schema = probe.schema.as_deref().unwrap_or("<missing>");
    if schema != HARNESS_RECEIPT_SCHEMA {
        return Err(ReceiptStoreError::WrongSchema {
            path: path.to_path_buf(),
            schema: schema.to_owned(),
        });
    }
    let receipt = serde_json::from_str::<HarnessReceipt>(contents).map_err(|source| {
        ReceiptStoreError::MalformedReceipt {
            path: path.to_path_buf(),
            message: source.to_string(),
        }
    })?;
    if receipt.id != expected_id {
        return Err(ReceiptStoreError::IdFilenameMismatch {
            path: path.to_path_buf(),
            receipt_id: receipt.id,
            file_stem: expected_id.to_owned(),
        });
    }
    Ok(receipt)
}

#[derive(Debug, Deserialize)]
struct ReceiptSchemaProbe {
    schema: Option<String>,
}

fn verify_receipt_proof(
    path: &Path,
    receipt: &HarnessReceipt,
    signature_policy: RuntimeReceiptSignaturePolicy<'_>,
) -> Result<(), ReceiptStoreError> {
    let proof_contexts = RuntimeReceiptProofContextProvider::new(signature_policy);
    let context = proof_contexts.proof_context(receipt);
    let verification = verify_harness_receipt_proof(receipt, &context);
    if verification.valid {
        Ok(())
    } else {
        Err(ReceiptStoreError::ReceiptProofInvalid {
            path: path.to_path_buf(),
            receipt_id: receipt.id.clone(),
            message: format!("{:?}", verification.findings),
        })
    }
}

fn write_atomic(dir: &Path, file_name: &str, contents: &[u8]) -> Result<(), ReceiptStoreError> {
    let temp_name = temp_file_name(file_name);
    let temp_path = dir.join(&temp_name);
    let final_path = dir.join(file_name);
    let write_result = write_temp_file(&temp_path, contents)
        .and_then(|()| fs::rename(&temp_path, &final_path))
        .and_then(|()| sync_directory(dir));
    if let Err(source) = write_result {
        let _ignored = fs::remove_file(&temp_path);
        return Err(ReceiptStoreError::StoreUnreadable {
            path: final_path,
            source,
        });
    }
    Ok(())
}

fn write_temp_file(path: &Path, contents: &[u8]) -> Result<(), std::io::Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.flush()?;
    file.sync_all()
}

fn sync_directory(path: &Path) -> Result<(), std::io::Error> {
    File::open(path)?.sync_all()
}

fn temp_file_name(file_name: &str) -> String {
    format!(".{file_name}.tmp.{}-{}", std::process::id(), unix_nanos())
}

fn generated_at_nanos() -> String {
    unix_nanos().to_string()
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
