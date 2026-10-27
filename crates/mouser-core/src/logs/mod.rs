//! Collecting the desk's logs (Story 2 scenario 10, FR-007): answering a
//! desk-mate's request, collecting every computer's part, and the bundle
//! file they make.

pub mod bundle;
pub(crate) mod collect;
pub(crate) mod serve;

#[cfg(test)]
mod bundle_tests;
#[cfg(test)]
mod collect_tests;
#[cfg(test)]
mod serve_tests;
