#[cfg(feature = "cli-tool")]
pub mod cli_tool;

#[cfg(feature = "a2a")]
pub mod a2a;

#[cfg(feature = "agent")]
pub mod agent;

#[cfg(feature = "catalog")]
pub mod catalog;

#[cfg(feature = "external-adapter")]
pub mod external_adapter;

#[cfg(feature = "mcp")]
pub mod mcp;

#[cfg(feature = "payment-rails")]
pub mod payment_supervisor;
