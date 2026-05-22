// rust-style-allow: large-file because payment state currently owns persisted
// idempotency, spend-capability consumption, rail mutation recovery, and the
// step-persistence transaction until the payment execution boundary splits.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use runx_contracts::{JsonObject, JsonValue};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::payment_packets::{PaymentPacketError, PaymentRailPacket, read_payment_rail_packet};
use crate::payment_supervisor::{
    PaymentSupervisorProof, PaymentSupervisorProofMatch, validate_payment_supervisor_proof,
};

pub const PAYMENT_STATE_SCHEMA_VERSION: &str = "runx.payment_state.v4";
pub const RUNX_PAYMENT_STATE_PATH_ENV: &str = "RUNX_PAYMENT_STATE_PATH";
const PAYMENT_STATE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const PAYMENT_STATE_LOCK_RETRY: Duration = Duration::from_millis(10);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentIdempotencyKey {
    pub rail: String,
    pub counterparty: String,
    pub key: String,
}

impl PaymentIdempotencyKey {
    pub fn new(
        rail: impl Into<String>,
        counterparty: impl Into<String>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            rail: rail.into(),
            counterparty: counterparty.into(),
            key: key.into(),
        }
    }
}

impl PaymentIdempotencyKey {
    fn index_key(&self) -> String {
        format!("{}\u{1f}{}\u{1f}{}", self.rail, self.counterparty, self.key)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentIdempotencyEntry {
    pub idempotency_key: PaymentIdempotencyKey,
    pub receipt_ref: String,
    pub receipt_created_at: String,
    pub receipt_digest: String,
    pub rail_proof_ref: String,
    pub supervisor_proof: PaymentSupervisorProof,
    pub amount_minor: u64,
    pub currency: String,
    pub outputs: JsonObject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpendCapabilityConsumption {
    pub capability_ref: String,
    pub idempotency_key: PaymentIdempotencyKey,
    pub receipt_ref: Option<String>,
    pub recovery_state: Option<PaymentRecoveryState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentRecoveryState {
    InFlight,
    Sealed,
    Escalated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RailMutation {
    pub idempotency_key: PaymentIdempotencyKey,
    pub rail: String,
    pub amount_minor: u64,
    pub currency: String,
    pub counterparty: String,
    pub status: RailMutationStatus,
    pub proof_ref: Option<String>,
    pub recovery_state: PaymentRecoveryState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RailMutationStatus {
    Partial,
    Fulfilled,
    Escalated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentStepStateInput {
    pub idempotency_key: PaymentIdempotencyKey,
    pub spend_capability_ref: String,
    pub rail: String,
    pub counterparty: String,
    pub amount_minor: u64,
    pub currency: String,
    pub act_id: String,
}

#[derive(Debug)]
pub struct FileBackedPaymentStateStore {
    path: PathBuf,
    state: PaymentStateDocument,
}

#[derive(Debug)]
struct PaymentStateLock {
    path: PathBuf,
    _file: File,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaymentStateDocument {
    schema_version: String,
    idempotency_entries: BTreeMap<String, PaymentIdempotencyEntry>,
    consumed_spend_capabilities: BTreeMap<String, SpendCapabilityConsumption>,
    rail_mutations: BTreeMap<String, RailMutation>,
}

impl Default for PaymentStateDocument {
    fn default() -> Self {
        Self {
            schema_version: PAYMENT_STATE_SCHEMA_VERSION.to_owned(),
            idempotency_entries: BTreeMap::new(),
            consumed_spend_capabilities: BTreeMap::new(),
            rail_mutations: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PaymentStateError {
    #[error("payment state path {path} has no parent directory")]
    MissingParent { path: PathBuf },
    #[error("failed to read payment state {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse payment state {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to create payment state directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write payment state {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to lock payment state {path}: {message}")]
    Lock { path: PathBuf, message: String },
    #[error("failed to serialize payment state {path}: {source}")]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported payment state schema version {schema_version}")]
    UnsupportedSchemaVersion { schema_version: String },
    #[error("idempotency key {idempotency_key} was already recorded")]
    IdempotencyAlreadyRecorded { idempotency_key: String },
    #[error("rail mutation for idempotency key {idempotency_key} was already recorded")]
    RailMutationAlreadyRecorded { idempotency_key: String },
    #[error("spend capability {capability_ref} was already consumed")]
    SpendCapabilityAlreadyConsumed { capability_ref: String },
    #[error("failed to serialize replay-safe payment outputs: {source}")]
    ReplayOutputSerialize {
        #[source]
        source: serde_json::Error,
    },
    #[error("payment supervisor proof is required before sealing rail proof {proof_ref}")]
    MissingSupervisorProof { proof_ref: String },
    #[error("payment supervisor proof mismatch: {message}")]
    SupervisorProof { message: String },
    #[error(transparent)]
    PaymentPacket(#[from] PaymentPacketError),
}

impl FileBackedPaymentStateStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, PaymentStateError> {
        let path = path.into();
        let state = load_payment_state(&path)?;
        Ok(Self { path, state })
    }

    pub fn lookup_idempotency(
        &self,
        key: &PaymentIdempotencyKey,
    ) -> Option<&PaymentIdempotencyEntry> {
        self.state.idempotency_entries.get(&key.index_key())
    }

    pub fn record_idempotency(
        &mut self,
        entry: PaymentIdempotencyEntry,
    ) -> Result<(), PaymentStateError> {
        let index_key = entry.idempotency_key.index_key();
        if self.state.idempotency_entries.contains_key(&index_key) {
            return Err(PaymentStateError::IdempotencyAlreadyRecorded {
                idempotency_key: index_key,
            });
        }
        self.with_locked_state(|state| {
            if state.idempotency_entries.contains_key(&index_key) {
                return Err(PaymentStateError::IdempotencyAlreadyRecorded {
                    idempotency_key: index_key.clone(),
                });
            }
            state.idempotency_entries.insert(index_key, entry);
            Ok(())
        })
    }

    pub fn lookup_consumed_spend_capability(
        &self,
        capability_ref: &str,
    ) -> Option<&SpendCapabilityConsumption> {
        self.state.consumed_spend_capabilities.get(capability_ref)
    }

    pub fn consume_spend_capability(
        &mut self,
        consumption: SpendCapabilityConsumption,
    ) -> Result<(), PaymentStateError> {
        let capability_ref = consumption.capability_ref.clone();
        if self
            .state
            .consumed_spend_capabilities
            .contains_key(&capability_ref)
        {
            return Err(PaymentStateError::SpendCapabilityAlreadyConsumed { capability_ref });
        }
        self.with_locked_state(|state| {
            if state
                .consumed_spend_capabilities
                .contains_key(&capability_ref)
            {
                return Err(PaymentStateError::SpendCapabilityAlreadyConsumed {
                    capability_ref: capability_ref.clone(),
                });
            }
            state
                .consumed_spend_capabilities
                .insert(capability_ref, consumption);
            Ok(())
        })
    }

    pub fn lookup_rail_mutation(&self, key: &PaymentIdempotencyKey) -> Option<&RailMutation> {
        self.state.rail_mutations.get(&key.index_key())
    }

    pub fn escalate_rail_mutation(
        &mut self,
        key: &PaymentIdempotencyKey,
    ) -> Result<Option<RailMutation>, PaymentStateError> {
        if !self.state.rail_mutations.contains_key(&key.index_key()) {
            return Ok(None);
        }
        self.with_locked_state(|state| {
            let Some(mutation) = state.rail_mutations.get_mut(&key.index_key()) else {
                return Ok(None);
            };
            mutation.status = RailMutationStatus::Escalated;
            mutation.recovery_state = PaymentRecoveryState::Escalated;
            Ok(Some(mutation.clone()))
        })
    }

    pub fn record_rail_mutation(
        &mut self,
        mutation: RailMutation,
    ) -> Result<(), PaymentStateError> {
        let index_key = mutation.idempotency_key.index_key();
        if self.state.rail_mutations.contains_key(&index_key) {
            return Err(PaymentStateError::RailMutationAlreadyRecorded {
                idempotency_key: index_key,
            });
        }
        self.with_locked_state(|state| {
            if state.rail_mutations.contains_key(&index_key) {
                return Err(PaymentStateError::RailMutationAlreadyRecorded {
                    idempotency_key: index_key.clone(),
                });
            }
            state.rail_mutations.insert(index_key, mutation);
            Ok(())
        })
    }

    fn with_locked_state<T>(
        &mut self,
        update: impl FnOnce(&mut PaymentStateDocument) -> Result<T, PaymentStateError>,
    ) -> Result<T, PaymentStateError> {
        let _lock = PaymentStateLock::acquire(&self.path)?;
        let mut state = load_payment_state(&self.path)?;
        let result = update(&mut state)?;
        persist_payment_state(&self.path, &state)?;
        self.state = state;
        Ok(result)
    }
}

impl PaymentStateLock {
    fn acquire(path: &Path) -> Result<Self, PaymentStateError> {
        let parent = path
            .parent()
            .ok_or_else(|| PaymentStateError::MissingParent {
                path: path.to_path_buf(),
            })?;
        fs::create_dir_all(parent).map_err(|source| PaymentStateError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
        let lock_path = payment_state_lock_path(path);
        let started = Instant::now();
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(file) => {
                    return Ok(Self {
                        path: lock_path,
                        _file: file,
                    });
                }
                Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                    if started.elapsed() >= PAYMENT_STATE_LOCK_TIMEOUT {
                        return Err(PaymentStateError::Lock {
                            path: path.to_path_buf(),
                            message: format!("timed out waiting for lock {}", lock_path.display()),
                        });
                    }
                    thread::sleep(PAYMENT_STATE_LOCK_RETRY);
                }
                Err(source) => {
                    return Err(PaymentStateError::Lock {
                        path: path.to_path_buf(),
                        message: source.to_string(),
                    });
                }
            }
        }
    }
}

impl Drop for PaymentStateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn load_payment_state(path: &Path) -> Result<PaymentStateDocument, PaymentStateError> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let version: PaymentStateVersion =
                serde_json::from_str(&contents).map_err(|source| PaymentStateError::Parse {
                    path: path.to_path_buf(),
                    source,
                })?;
            match version.schema_version.as_str() {
                PAYMENT_STATE_SCHEMA_VERSION => {
                    serde_json::from_str(&contents).map_err(|source| PaymentStateError::Parse {
                        path: path.to_path_buf(),
                        source,
                    })
                }
                "runx.payment_state.v3" => {
                    let legacy: PaymentStateDocumentV3 =
                        serde_json::from_str(&contents).map_err(|source| {
                            PaymentStateError::Parse {
                                path: path.to_path_buf(),
                                source,
                            }
                        })?;
                    Ok(legacy.into_current())
                }
                "runx.payment_state.v2" => {
                    let legacy: PaymentStateDocumentV2 =
                        serde_json::from_str(&contents).map_err(|source| {
                            PaymentStateError::Parse {
                                path: path.to_path_buf(),
                                source,
                            }
                        })?;
                    Ok(legacy.into_current())
                }
                schema_version => Err(PaymentStateError::UnsupportedSchemaVersion {
                    schema_version: schema_version.to_owned(),
                }),
            }
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(PaymentStateDocument::default())
        }
        Err(source) => Err(PaymentStateError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn persist_payment_state(
    path: &Path,
    state: &PaymentStateDocument,
) -> Result<(), PaymentStateError> {
    let parent = path
        .parent()
        .ok_or_else(|| PaymentStateError::MissingParent {
            path: path.to_path_buf(),
        })?;
    fs::create_dir_all(parent).map_err(|source| PaymentStateError::CreateDirectory {
        path: parent.to_path_buf(),
        source,
    })?;
    write_json_atomically(path, state)
}

fn payment_state_lock_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("payment-state.json");
    path.with_file_name(format!(".{file_name}.lock"))
}

#[derive(Clone, Debug, Deserialize)]
struct PaymentStateVersion {
    schema_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaymentStateDocumentV2 {
    schema_version: String,
    idempotency_entries: BTreeMap<String, serde_json::Value>,
    consumed_spend_capabilities: BTreeMap<String, SpendCapabilityConsumption>,
    rail_mutations: BTreeMap<String, RailMutation>,
}

impl PaymentStateDocumentV2 {
    fn into_current(self) -> PaymentStateDocument {
        let _ = self.schema_version;
        let _ = self.idempotency_entries;
        PaymentStateDocument {
            schema_version: PAYMENT_STATE_SCHEMA_VERSION.to_owned(),
            idempotency_entries: BTreeMap::new(),
            consumed_spend_capabilities: self.consumed_spend_capabilities,
            rail_mutations: self.rail_mutations,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaymentStateDocumentV3 {
    schema_version: String,
    idempotency_entries: BTreeMap<String, serde_json::Value>,
    consumed_spend_capabilities: BTreeMap<String, SpendCapabilityConsumption>,
    rail_mutations: BTreeMap<String, RailMutation>,
}

impl PaymentStateDocumentV3 {
    fn into_current(self) -> PaymentStateDocument {
        let _ = self.schema_version;
        let _ = self.idempotency_entries;
        PaymentStateDocument {
            schema_version: PAYMENT_STATE_SCHEMA_VERSION.to_owned(),
            idempotency_entries: BTreeMap::new(),
            consumed_spend_capabilities: self.consumed_spend_capabilities,
            rail_mutations: self.rail_mutations,
        }
    }
}

pub fn consumed_spend_capability_recorded(
    env: &BTreeMap<String, String>,
    cwd: &Path,
    capability_ref: &str,
) -> Result<bool, PaymentStateError> {
    let Some(path) = resolve_payment_state_path(env, cwd) else {
        return Ok(false);
    };
    let store = FileBackedPaymentStateStore::open(&path)?;
    Ok(store
        .lookup_consumed_spend_capability(capability_ref)
        .is_some())
}

pub fn lookup_payment_idempotency_entry(
    env: &BTreeMap<String, String>,
    cwd: &Path,
    key: &PaymentIdempotencyKey,
) -> Result<Option<PaymentIdempotencyEntry>, PaymentStateError> {
    let Some(path) = resolve_payment_state_path(env, cwd) else {
        return Ok(None);
    };
    let store = FileBackedPaymentStateStore::open(&path)?;
    Ok(store.lookup_idempotency(key).cloned())
}

pub fn lookup_payment_rail_mutation(
    env: &BTreeMap<String, String>,
    cwd: &Path,
    key: &PaymentIdempotencyKey,
) -> Result<Option<RailMutation>, PaymentStateError> {
    let Some(path) = resolve_payment_state_path(env, cwd) else {
        return Ok(None);
    };
    let store = FileBackedPaymentStateStore::open(&path)?;
    Ok(store.lookup_rail_mutation(key).cloned())
}

pub fn escalate_payment_rail_mutation(
    env: &BTreeMap<String, String>,
    cwd: &Path,
    key: &PaymentIdempotencyKey,
) -> Result<Option<RailMutation>, PaymentStateError> {
    let Some(path) = resolve_payment_state_path(env, cwd) else {
        return Ok(None);
    };
    let mut store = FileBackedPaymentStateStore::open(&path)?;
    store.escalate_rail_mutation(key)
}

// rust-style-allow: long-function because payment state persistence binds
// authority, output, receipt, and recovery-state invariants in one transaction.
pub fn persist_payment_step_state(
    env: &BTreeMap<String, String>,
    cwd: &Path,
    input: &PaymentStepStateInput,
    outputs: &JsonObject,
    receipt: &runx_contracts::HarnessReceipt,
    supervisor_proof: Option<&PaymentSupervisorProof>,
) -> Result<(), PaymentStateError> {
    let Some(path) = resolve_payment_state_path(env, cwd) else {
        return Ok(());
    };
    let rail_packet = read_payment_rail_packet(outputs)?;
    let recovery_state = payment_recovery_state(rail_packet.as_ref());
    let rail_touched = rail_packet
        .as_ref()
        .and_then(|packet| packet.result.as_ref())
        .and_then(|result| result.status.as_deref())
        .is_some();

    let mut store = FileBackedPaymentStateStore::open(&path)?;

    if rail_touched
        && store
            .lookup_consumed_spend_capability(&input.spend_capability_ref)
            .is_none()
    {
        store.consume_spend_capability(SpendCapabilityConsumption {
            capability_ref: input.spend_capability_ref.clone(),
            idempotency_key: input.idempotency_key.clone(),
            receipt_ref: Some(receipt.id.clone()),
            recovery_state: Some(recovery_state.clone()),
        })?;
    }

    let proof_ref = rail_packet
        .as_ref()
        .and_then(|packet| packet.proof.as_ref())
        .map(|proof| proof.proof_ref.as_str());
    let supervisor_proof = proof_ref
        .map(|proof_ref| {
            validate_sealed_supervisor_proof(input, receipt, proof_ref, supervisor_proof)
        })
        .transpose()?;

    if let Some(proof_ref) = proof_ref
        && store.lookup_idempotency(&input.idempotency_key).is_none()
    {
        let result = rail_packet
            .as_ref()
            .and_then(|packet| packet.result.as_ref());
        store.record_idempotency(PaymentIdempotencyEntry {
            idempotency_key: input.idempotency_key.clone(),
            receipt_ref: receipt.id.clone(),
            receipt_created_at: receipt.created_at.clone(),
            receipt_digest: receipt.seal.digest.clone(),
            rail_proof_ref: proof_ref.to_owned(),
            supervisor_proof: supervisor_proof.cloned().ok_or_else(|| {
                PaymentStateError::MissingSupervisorProof {
                    proof_ref: proof_ref.to_owned(),
                }
            })?,
            amount_minor: result
                .and_then(|result| result.amount_minor)
                .unwrap_or(input.amount_minor),
            currency: result
                .and_then(|result| result.currency.as_deref())
                .unwrap_or(&input.currency)
                .to_owned(),
            outputs: replay_safe_outputs(outputs)?,
        })?;
    }

    if rail_touched && store.lookup_rail_mutation(&input.idempotency_key).is_none() {
        let result = rail_packet
            .as_ref()
            .and_then(|packet| packet.result.as_ref());
        store.record_rail_mutation(RailMutation {
            idempotency_key: input.idempotency_key.clone(),
            rail: result
                .and_then(|result| result.rail.as_deref())
                .unwrap_or(&input.rail)
                .to_owned(),
            amount_minor: result
                .and_then(|result| result.amount_minor)
                .unwrap_or(input.amount_minor),
            currency: result
                .and_then(|result| result.currency.as_deref())
                .unwrap_or(&input.currency)
                .to_owned(),
            counterparty: result
                .and_then(|result| result.counterparty.as_deref())
                .unwrap_or(&input.counterparty)
                .to_owned(),
            status: rail_mutation_status(&recovery_state),
            proof_ref: proof_ref.map(str::to_owned),
            recovery_state,
        })?;
    }

    Ok(())
}

fn validate_sealed_supervisor_proof<'a>(
    input: &PaymentStepStateInput,
    receipt: &runx_contracts::HarnessReceipt,
    proof_ref: &str,
    supervisor_proof: Option<&'a PaymentSupervisorProof>,
) -> Result<&'a PaymentSupervisorProof, PaymentStateError> {
    let proof = supervisor_proof.ok_or_else(|| PaymentStateError::MissingSupervisorProof {
        proof_ref: proof_ref.to_owned(),
    })?;
    validate_payment_supervisor_proof(
        proof,
        PaymentSupervisorProofMatch {
            proof_ref,
            rail: &input.rail,
            counterparty: &input.counterparty,
            amount_minor: input.amount_minor,
            currency: &input.currency,
            idempotency_key: &input.idempotency_key.key,
            spend_capability_ref: &input.spend_capability_ref,
            act_id: &input.act_id,
            receipt_ref: &receipt.id,
            receipt_digest: &receipt.seal.digest,
        },
    )
    .map_err(|source| PaymentStateError::SupervisorProof {
        message: source.to_string(),
    })?;
    Ok(proof)
}

fn replay_safe_outputs(outputs: &JsonObject) -> Result<JsonObject, PaymentStateError> {
    let mut safe_outputs = outputs.clone();
    sanitize_replay_payload(&mut safe_outputs);

    let mut stdout_payload = safe_outputs.clone();
    stdout_payload.remove("stdout");
    stdout_payload.remove("stderr");
    stdout_payload.remove("status");
    sanitize_replay_payload(&mut stdout_payload);

    let stdout = serde_json::to_string(&JsonValue::Object(stdout_payload))
        .map_err(|source| PaymentStateError::ReplayOutputSerialize { source })?;
    safe_outputs.insert("stdout".to_owned(), JsonValue::String(stdout));
    safe_outputs
        .entry("stderr".to_owned())
        .or_insert_with(|| JsonValue::String(String::new()));
    safe_outputs
        .entry("status".to_owned())
        .or_insert_with(|| JsonValue::String("success".to_owned()));
    Ok(safe_outputs)
}

fn sanitize_replay_payload(payload: &mut JsonObject) {
    let Some(JsonValue::Object(packet)) = payload.get_mut("payment_rail_packet") else {
        return;
    };
    let Some(JsonValue::Object(data)) = packet.get_mut("data") else {
        return;
    };
    if let Some(JsonValue::Object(proof)) = data.get_mut("rail_proof") {
        proof.remove("rail_session_material_ref");
    }
}

fn payment_recovery_state(packet: Option<&PaymentRailPacket>) -> PaymentRecoveryState {
    match packet {
        Some(PaymentRailPacket {
            recovery_status: Some(status),
            ..
        }) if status == "sealed" => PaymentRecoveryState::Sealed,
        Some(PaymentRailPacket {
            recovery_status: Some(status),
            ..
        }) if status == "terminal_decline" || status == "escalated" => {
            PaymentRecoveryState::Escalated
        }
        Some(PaymentRailPacket {
            recovery_status: Some(status),
            ..
        }) if status == "recoverable_timeout" || status == "partial" || status == "in_flight" => {
            PaymentRecoveryState::InFlight
        }
        Some(PaymentRailPacket { proof: Some(_), .. }) => PaymentRecoveryState::Sealed,
        _ => PaymentRecoveryState::InFlight,
    }
}

fn rail_mutation_status(recovery_state: &PaymentRecoveryState) -> RailMutationStatus {
    match recovery_state {
        PaymentRecoveryState::Sealed => RailMutationStatus::Fulfilled,
        PaymentRecoveryState::Escalated => RailMutationStatus::Escalated,
        PaymentRecoveryState::InFlight => RailMutationStatus::Partial,
    }
}

pub fn resolve_payment_state_path(env: &BTreeMap<String, String>, cwd: &Path) -> Option<PathBuf> {
    env.get(RUNX_PAYMENT_STATE_PATH_ENV)
        .filter(|value| !value.trim().is_empty())
        .map(|value| resolve_path(Path::new(value), cwd))
        .or_else(|| {
            env.get(crate::receipts::paths::RUNX_RECEIPT_DIR_ENV)
                .filter(|value| !value.trim().is_empty())
                .map(|value| resolve_path(Path::new(value), cwd).join("payment-state.json"))
        })
}

fn resolve_path(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn write_json_atomically<T: Serialize>(path: &Path, value: &T) -> Result<(), PaymentStateError> {
    let parent = path
        .parent()
        .ok_or_else(|| PaymentStateError::MissingParent {
            path: path.to_path_buf(),
        })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("payment-state.json");
    let temp_path = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        monotonicish_nanos()
    ));

    let write_result = (|| {
        let mut file = File::create(&temp_path).map_err(|source| PaymentStateError::Write {
            path: temp_path.clone(),
            source,
        })?;
        serde_json::to_writer_pretty(&mut file, value).map_err(|source| {
            PaymentStateError::Serialize {
                path: temp_path.clone(),
                source,
            }
        })?;
        file.write_all(b"\n")
            .map_err(|source| PaymentStateError::Write {
                path: temp_path.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| PaymentStateError::Write {
            path: temp_path.clone(),
            source,
        })?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    fs::rename(&temp_path, path).map_err(|source| {
        let _ = fs::remove_file(&temp_path);
        PaymentStateError::Write {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn monotonicish_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
