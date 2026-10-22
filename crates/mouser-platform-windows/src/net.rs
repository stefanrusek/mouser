//! Interface enumeration and physical-LAN classification (research R3):
//! `GetAdaptersAddresses` IfType plus a list of virtual-adapter names.

use mouser_platform::{InterfaceKind, NetInterface};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use windows::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, IF_TYPE_PPP, IF_TYPE_PROP_VIRTUAL,
    IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL, IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6,
};

/// Words in an adapter's name or description that mark it virtual, whatever its
/// IfType (Hyper-V switches and VM/VPN drivers present themselves as Ethernet).
const VIRTUAL_WORDS: &[&str] = &[
    "vethernet",
    "hyper-v",
    "vmware",
    "virtualbox",
    "tap",
    "wireguard",
    "tailscale",
];

/// IfTypes of tunnels and software adapters.
const TUNNEL_TYPES: &[u32] = &[IF_TYPE_TUNNEL, IF_TYPE_PPP, IF_TYPE_PROP_VIRTUAL];

pub(crate) fn classify(if_type: u32, name: &str, description: &str) -> InterfaceKind {
    let text = format!("{name} {description}").to_lowercase();
    if if_type == IF_TYPE_SOFTWARE_LOOPBACK {
        InterfaceKind::Loopback
    } else if TUNNEL_TYPES.contains(&if_type) || VIRTUAL_WORDS.iter().any(|w| text.contains(w)) {
        InterfaceKind::Virtual
    } else if if_type == IF_TYPE_ETHERNET_CSMACD || if_type == IF_TYPE_IEEE80211 {
        InterfaceKind::PhysicalLan
    } else {
        InterfaceKind::Virtual
    }
}

fn wide_str(p: windows::core::PWSTR) -> String {
    unsafe { p.to_string() }.expect("adapter string")
}

pub(crate) fn interfaces() -> Vec<NetInterface> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = 0u32;
    let probe = unsafe { GetAdaptersAddresses(AF_UNSPEC.0 as u32, flags, None, None, &mut size) };
    assert_eq!(probe, ERROR_BUFFER_OVERFLOW.0, "GetAdaptersAddresses size");
    // u64 elements keep the buffer aligned for IP_ADAPTER_ADDRESSES_LH.
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    let first = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
    let r =
        unsafe { GetAdaptersAddresses(AF_UNSPEC.0 as u32, flags, None, Some(first), &mut size) };
    assert_eq!(r, 0, "GetAdaptersAddresses");
    let mut out = Vec::new();
    let mut adapter = first as *const IP_ADAPTER_ADDRESSES_LH;
    while let Some(a) = unsafe { adapter.as_ref() } {
        let name = wide_str(a.FriendlyName);
        let description = wide_str(a.Description);
        let mut addrs = Vec::new();
        let mut unicast = a.FirstUnicastAddress;
        while let Some(u) = unsafe { unicast.as_ref() } {
            addrs.push(sockaddr_ip(u.Address.lpSockaddr));
            unicast = u.Next;
        }
        out.push(NetInterface {
            kind: classify(a.IfType, &name, &description),
            name,
            addrs,
        });
        adapter = a.Next;
    }
    out
}

fn sockaddr_ip(sa: *const windows::Win32::Networking::WinSock::SOCKADDR) -> IpAddr {
    let family = unsafe { (*sa).sa_family };
    if family == AF_INET {
        let v4 = unsafe { &*(sa as *const SOCKADDR_IN) };
        IpAddr::V4(Ipv4Addr::from(u32::from_be(unsafe {
            v4.sin_addr.S_un.S_addr
        })))
    } else {
        assert_eq!(family, AF_INET6, "unicast address family");
        let v6 = unsafe { &*(sa as *const SOCKADDR_IN6) };
        IpAddr::V6(Ipv6Addr::from(unsafe { v6.sin6_addr.u.Byte }))
    }
}
