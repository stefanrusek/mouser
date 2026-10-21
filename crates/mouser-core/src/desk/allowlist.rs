//! Allowlist enforcement (Story 1 scenario 7).

use std::net::IpAddr;

use ipnet::IpNet;

/// Whether a join request from `ip` may be considered: always when the
/// allowlist is empty (off), otherwise only from inside one of its networks.
pub fn permits(allowlist: &[String], ip: IpAddr) -> bool {
    // A dual-stack socket reports IPv4 peers as IPv4-mapped IPv6 addresses.
    let ip = ip.to_canonical();
    allowlist.is_empty()
        || allowlist
            .iter()
            .any(|net| net.parse::<IpNet>().expect("validated CIDR").contains(&ip))
}
