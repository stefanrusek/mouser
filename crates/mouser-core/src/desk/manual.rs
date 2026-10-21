//! Joining by network address (Story 1 scenario 6).

use std::net::SocketAddr;

/// Resolve `host:port` (a name or an IP address); IPv4 is preferred.
pub async fn resolve(address: &str) -> Option<SocketAddr> {
    tokio::net::lookup_host(address)
        .await
        .ok()?
        .min_by_key(|a| a.is_ipv6())
}
