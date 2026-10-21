# Plan-Phase Spike Findings: Network (QUIC transport, migration, LAN discovery)

Date 2026-10-21. Host macOS 26.5 (Darwin 25.5.0) arm64, rustc/cargo 1.99.0. Throwaway probes; build output in the scratchpad. Saved by the plan orchestrator from the probe agent's report.

## Crate versions (exact pins; Cargo.lock kept per probe)
| Probe | Direct deps | Notable transitive |
|---|---|---|
| spike_quic/ | quinn =0.11.12, rcgen =0.14.10, tokio =1.54.0, bytes =1.12.1, anyhow =1.0.104 | quinn-proto 0.11.19, quinn-udp 0.5.16, rustls 0.23.46 (ring 0.17.14), socket2 0.6.5 |
| spike_mdns/ | mdns-sd =0.21.6, if-addrs =0.15.0 (feature link-local), anyhow =1.0.104 | mdns-sd uses if-addrs 0.15 |

## Host network facts
```
networksetup -listallhardwareports -> en0 = Ethernet, en1 = Wi-Fi
en0: inet 192.168.1.238 netmask 0xfffffc00 broadcast 192.168.3.255
en1: inet 192.168.2.164 netmask 0xfffffc00 broadcast 192.168.3.255
netstat -rn: default 192.168.1.1 on en0 (UGScg) and en1 (UGScIg, ifscoped); 192.168.0/22 on both
route -n get 192.168.1.238 -> interface: lo0 ; route -n get -ifscope en1 192.168.1.238 -> interface: en1
```
- en0 and en1 are on the SAME /22 (not different subnets).
- en0 IP → en0 IP goes via lo0 (no wire). A socket bound to en1's IP sending to en0's IP leaves over Wi-Fi and returns over Ethernet — the only real-medium measurement possible from one host.
- utun0-3 have only IPv6 link-local addresses (no VPN IPv4). bridge0, anpi0-2, ap1, en2-4 have no IPs.

## A1. quinn + rcgen: connection, bi streams, datagrams, latency
- **Assumption:** quinn sets up TLS 1.3 QUIC with an rcgen self-signed cert pinned as the only root; bi streams and RFC 9221 datagrams work; small-message RTT is low.
- **Probe:** spikes/spike_quic (`bench`): server echoes datagrams and 32-byte stream frames; client 50 warm-up then 1000 timed datagram echoes and 1000 stream echoes. **Command:** `spike_quic bench <server_ip> <client_ip>`
- **Observed:**
  - loopback: handshake=5.1ms max_datagram_size=Some(1162); datagram p50=204.375µs p99=349.041µs max=464.25µs lost=0; stream p50=194.584µs p99=306.833µs max=430.917µs
  - en0→itself (via lo0): datagram p50=193.291µs p99=325.417µs lost=0; stream p50=192.291µs p99=298.458µs
  - en1 (Wi-Fi) → en0 (Ethernet), over the air: handshake=12.2ms; datagram min=3.507ms p50=4.578ms p99=10.280ms max=23.136ms lost=0; stream p50=5.392ms p99=11.911ms max=304.767ms; path rtt estimate 6.41ms
- **Verdict:** Confirmed.
- **Implication:** QUIC overhead ~0.2 ms; the medium dominates (Wi-Fi p99 ~10 ms, one 305 ms head-of-line stall on the stream). Pointer motion as datagrams (latest-wins); keys/buttons/clipboard on streams. Keep datagram payloads < ~1100 bytes (initial max_datagram_size 1162–1288). True Ethernet figure needs a second machine. Trust via per-peer pinned cert or custom verifier, not WebPKI.

## A2. QUIC connection migration via Endpoint::rebind
- **Assumption:** connection and an open stream survive the client rebinding to a new socket/address.
- **Probe:** spike_quic `migrate`: 600 messages (stream + datagram echo, 2 ms apart), `rebind(UdpSocket::bind(<new ip>:0))` at msg 300; server polls remote_address() every 20 ms; keep-alive 500 ms, idle 3 s, migration(true). **Command:** `spike_quic migrate <server_ip> <client_before> <client_after>`
- **Observed:**
  - loopback port change: rebind took 14.541µs; msg 300 rtt=2.095ms; server saw 127.0.0.1:55453 → 127.0.0.1:65173; 600/600 stream in order; datagrams ok=600 lost=0; close_reason=None
  - Wi-Fi → Ethernet: 192.168.2.164:60688 → 192.168.1.238:62967; msg 299 5.74ms, 300 2.65ms, 301 0.88ms; 600/600, 0 lost
  - Ethernet → Wi-Fi: 192.168.1.238:62942 → 192.168.2.164:55362; msg 300 12.4ms then ~6ms; 600/600, 0 lost
- **Verdict:** Confirmed (client-initiated). First message after rebind pays one path-validation round trip.
- **Implication:** QUIC v1 migrates only the client; if the accepting side's address changes the peer must re-dial, so session state must survive reconnect-and-resume (the side whose network changed re-dials). quinn does not detect interface changes; if-addrs IfChangeNotifier is cfg'd out on macOS — use SCDynamicStore/NWPathMonitor or polling. Untested (needs root): passive migration of a 0.0.0.0-bound client.

## A3. Keep-alive / idle timeout; hard path loss
- **Probe:** spike_quic `pathloss`: in-process UDP relay black-holes both directions after 2 s idle. Knobs: `TransportConfig::keep_alive_interval`, `max_idle_timeout` (effective = min of peers), `ServerConfig::migration`. **Command:** `spike_quic pathloss <keepalive_ms> <idle_ms> <blackhole_ms>`
- **Observed:** 250/1000/10000 → idle held; loss detected 1.221 s (both sides "timed out"). 1000/3000/10000 → 3.083 s. 1000/5000/10000 → 5.067 s. 250/1000/600 → survived, post-recovery 5/5 echoes. 1000/3000/2000 → survived, 5/5.
- **Verdict:** Confirmed. Detection ≈ max_idle_timeout; keep-alive only prevents idle closure.
- **Implication:** max_idle_timeout is the "peer gone" budget; keep-alive ≈ 1/3–1/4 of it so VPN/Wi-Fi route flaps don't drop the session. Drive "reconnecting" UI from RTT/app heartbeats. Input while black-holed falls back locally.

## A4. mdns-sd register/browse with TXT desk name
- **Probe:** spike_mdns registers `_mouser-spike._udp.local.`, instance `<desk>-<pid>`, port 47999, TXT desk/ver/id, `enable_addr_auto()`; cross-checked with `dns-sd`. **Commands:** `spike_mdns same Stefan-Desk default`; `spike_mdns register Second-Desk 14 default &` + `spike_mdns browse 3`; `dns-sd -B/-L/-G`.
- **Observed:** `dns-sd -L`: `Second-Desk-34034._mouser-spike._udp.local. can be reached at mouser-spike-34034.local.:47999 (interface 15) desk=Second-Desk ver=0.0.1 id=abcd1234`. Second process: `RESOLVED ... desk=Some("Second-Desk"); V4 192.168.1.238 [en1,en0]; V4 192.168.2.164 [en1,en0] ...`. Same process: first resolve held only 127.0.0.1, growing to 13 addresses within ~1 s, each with interface_ids/scope_id. Processes that exited without unregister (or shutdown() without waiting) still listed by `dns-sd -B` minutes later (PTR/TXT TTL 4500 s, SRV/A 120 s). With `unregister(full)?.recv_timeout(3s)` → Ok and `shutdown()?.recv_timeout` → Ok, entry vanished immediately. A fresh mdns-sd browser did not report the stale entries.
- **Verdict:** Confirmed.
- **Implication:** desk name + protocol version in TXT; treat discovery as hints and verify by connecting; always unregister and wait on exit; handle incremental, multiple addresses; use scope_id for fe80.

## A5. Advertised addresses; interface restriction
- **Probe:** spike_mdns default vs `lan` mode (`disable_interface(IfKind::All)` + `enable_interface(IfKind::Predicate(IfPredicate::new(|i| classify(i)==PhysicalLanCandidate)))`).
- **Observed:** default: `SearchStarted ... on 3 interfaces [en1 (15), en0 (7), lo0 (1)]`; `dns-sd -G v4v6` shows fe80 addrs incl. FE80::1 on en1/en0/lo0, ::1, 127.0.0.1, and both IPv4s on both en0 and en1. lan: `SearchStarted ... on 2 interfaces [en1 (15), en0 (7)]`; resolved both IPv4s and fe80 addrs on en0/en1. mdns-sd 0.21.6 source (`my_ip_interfaces_inner`): excludes not-oper-up and point-to-point interfaces (utun), excludes awdl*/llw* by name (`include_apple_p2p(true)` re-enables), includes loopback by default. utun/awdl/llw never advertised. Defects: lo0 fe80::1 not treated as loopback (if-addrs is_loopback checks only ::1) so it was advertised on en0/en1; link-local announced on the other interface even in lan mode; both IPv4s on both interfaces (shared subnet). API: `IfKind::{All, IPv4, IPv6, Name, Addr, LoopbackV4, LoopbackV6, IndexV4, IndexV6, Predicate}`, `include_apple_p2p`, `set_ip_check_interval` (default 5 s); IfPredicate takes if_addrs::Interface — pin the same if-addrs version.
- **Verdict:** Filtering confirmed; defaults partial (leak lo0 fe80::1 and cross-interface link-local; bridge/VM IPv4 adapters not excluded).
- **Implication:** always use an explicit physical-LAN allow-list predicate; prefer IPv4 or restrict fe80 to the interface heard on; keep "add by address" fallback.

## A6. Interface enumeration and classification (if-addrs)
- **Probe:** spike_mdns `ifaces`; classify() by name prefixes (utun ipsec ppp tun tap wg tailscale zt gif stf bridge awdl llw anpi ap vmnet vboxnet docker br- virbr veth vEthernet), is_p2p flag, loopback = is_loopback() || name starts with "lo".
- **Observed:** awdl0 fe80 p2p=false → Virtual(AWDL); llw0 → Virtual; en0/en1 → PhysicalLanCandidate; lo0 127.0.0.1/::1 → Loopback; lo0 fe80::1 → PhysicalLanCandidate before name fix, Loopback after; utun0-3 fe80 p2p=true → Virtual. if-addrs lists only interfaces with addresses (bridge0/anpi/ap1/en2-4 absent). en2-4 are Thunderbolt-bridge members despite en* names; awdl/llw not flagged p2p.
- **Verdict:** Partial — heuristic correct on this host; if-addrs gives no hardware type.
- **Implication:** classify per OS: macOS SCNetworkInterfaceGetInterfaceType (Ethernet/IEEE80211); Windows GetAdaptersAddresses IfType + names (vEthernet, Hyper-V, VMware, VirtualBox, TAP, WireGuard, Tailscale); Linux /sys/class/net/<if>/device and type. Name list as fallback, plus user override. Untested: utun with IPv4, docker/VM bridges with IPv4, Windows/Linux.

## A7. macOS Local Network permission prompt
- **Observed:** no prompt, no failures; `log show --last 25m --predicate 'process == "nehelper" OR subsystem == "com.apple.networkextension" OR eventMessage CONTAINS[c] "local network"'` filtered for spike/local network → empty. Permission attributed to Warp.app (process chain spike_* → zsh → claude → zsh → Warp).
- **Verdict:** Not reproducible from the terminal harness.
- **Implication:** Mouser.app (or its login agent) is its own responsible process: needs `NSLocalNetworkUsageDescription` + `NSBonjourServices` (`_mouser._udp`), expect the prompt on first LAN/multicast use; onboarding must handle denial (e.g. EHOSTUNREACH). Re-verify with a signed .app.

## Summary
| # | Assumption | Verdict |
|---|---|---|
| A1 | quinn+rcgen streams/datagrams, latency | Confirmed (loopback p50 0.2/p99 0.35 ms; Wi-Fi p50 4.6/p99 10 ms; 0 loss) |
| A2 | Survives Endpoint::rebind mid-stream | Confirmed, client side only; app must trigger |
| A3 | Keep-alive/idle knobs; loss detection | Confirmed; detection ≈ max_idle_timeout; sub-idle outages survived |
| A4 | mdns-sd TXT desk name register/browse | Confirmed; must unregister and wait |
| A5 | VPN/virtual excluded / filterable | Partial; Predicate filter works; defaults leak |
| A6 | if-addrs classification | Partial; needs OS hardware-type APIs |
| A7 | macOS Local Network prompt | Not reproducible; real .app needs Info.plist keys + prompt handling |
