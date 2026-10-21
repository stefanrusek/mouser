//! Desks: creation, joining, replication, membership and the allowlist.

pub mod allowlist;
pub mod create;
pub mod join;
pub mod manual;
pub mod membership;
pub mod replication;

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
mod replication_tests;
