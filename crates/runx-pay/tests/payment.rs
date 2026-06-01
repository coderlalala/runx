// Payment integration tests. Submodules live under tests/payment/.
#[path = "payment/execution.rs"]
mod execution;
#[path = "payment/ledger_projection.rs"]
mod ledger_projection;
#[path = "payment/receipts.rs"]
mod receipts;
#[path = "payment/refunds.rs"]
mod refunds;
#[path = "payment/state.rs"]
mod state;
#[path = "payment/stripe_spt.rs"]
mod stripe_spt;
