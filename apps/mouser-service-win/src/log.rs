//! The service's own log (contracts/log-events.md): `service.log` in the
//! machine folder's `logs`, under the machine config's logging switch and
//! `max_log_mb`, starting each file with `log.header` (mode `service`) and
//! holding only the catalog's `service.*` records.

use std::path::Path;

use mouser_core::logging::{Context, Log, SERVICE_FILE};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ComputerMode;

/// The log of computer `id` whose machine folder is `machine`; it goes by
/// its configured name, or `host_name` when it has none.
pub fn open(machine: &Path, id: ComputerId, host_name: &str) -> Log {
    let config = mouser_core::config::load_or_create(machine);
    let name = match config.general.computer_name.is_empty() {
        true => host_name.to_string(),
        false => config.general.computer_name,
    };
    let context = Context {
        mode: ComputerMode::Service,
        ..Context::fallback()
    };
    Log::named(
        machine,
        SERVICE_FILE,
        id,
        name,
        config.diagnostics.logging,
        config.diagnostics.max_log_mb,
        context,
    )
}
