//! Receipts cluster.
//!
//! - `seal`: step and graph receipt sealing helpers.
//! - `store`: the local on-disk receipt store and index.
//! - `tree`: receipt-tree resolution and proof validation.
//! - `paths`: workspace and receipt-store path resolution.

pub mod paths;
pub mod seal;
pub mod store;
pub mod tree;

pub(crate) use seal::{
    RuntimeReceiptProofContextProvider, StepReceiptWithDisposition, graph_receipt_with_disposition,
    step_receipt_with_disposition,
};
pub use seal::{RuntimeReceiptSignaturePolicy, graph_receipt, step_receipt};
