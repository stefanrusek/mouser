# probe_dualstack

Standalone probe, outside the workspace and the gate: the evidence behind "IPv4 through a dual-stack socket" in `specs/001-mouser-v1/foundational-notes.md`. Plain quinn 0.11.12 (quinn-udp 0.5.16) and plain std/socket2 UDP sockets, same versions as mouser-core. `cargo run --release -- <mode>`:

- `ports`: which ephemeral ports the kernel hands a socket when a socket of the other family holds ports, and which explicit binds it refuses. On macOS an `[::]:0` socket (dual-stack or v6-only) is handed ports IPv4 sockets hold (about 2000 × 2000 / 16384, i.e. unchecked); an IPv4 `:0` socket never gets a dual-stack socket's port; an explicit dual-stack `[::]:p` binds although `0.0.0.0:p` is held.
- `deliver <v4-bind> <target>`: makes such a shared port and sends 100 IPv4 datagrams to it: all go to the IPv4 socket, none to the dual-stack one; IPv6 still reaches the dual-stack one.
- `listen <v4-bind> <secs>`: the same shared port, counting what another computer sends to it.
- `quic <server> <client> <target> <n> <held>`: `n` QUIC handshakes between fresh endpoints (`dual`, `v6only` or an IP), holding `held` extra IPv4 loopback sockets, with a 1 s idle timeout; each failure prints the kernel's sockets (`netstat`) on both ports, at bind time and after the failure.
