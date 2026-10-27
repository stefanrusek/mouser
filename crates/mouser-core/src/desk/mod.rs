//! Desks: creation, joining, replication, membership and the allowlist.

pub mod allowlist;
pub mod create;
pub mod join;
pub mod manual;
pub mod membership;
pub mod rename;
pub mod replication;
pub mod settings;

#[cfg(test)]
mod allowlist_tests;
#[cfg(test)]
mod create_tests;
#[cfg(test)]
mod join_tests;
#[cfg(test)]
mod manual_tests;
#[cfg(test)]
mod membership_tests;
#[cfg(test)]
mod rename_tests;
#[cfg(test)]
mod replication_tests;
#[cfg(test)]
mod settings_tests;
