//! Exact damage attribution and server-owned recent damage history.

pub(crate) mod combat;
mod history;
mod source;

pub(crate) use history::DamageHistoryBinding;
pub use history::{DamageHistory, RecentDamageSource};
pub use source::DamageSource;

#[cfg(test)]
mod tests;
