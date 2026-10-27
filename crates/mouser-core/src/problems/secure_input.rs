//! macOS Secure Input (Story 2 scenario 10): an application holding it blocks
//! Mouser from typing on that computer. The desk user is told which one, on
//! that computer and on every desk-mate; the connection stays up.

use crate::lock::LockExt;
use mouser_proto::control::ProblemKind;
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::SecureInputHolder;

use crate::engine::Ctx;

fn message(holder: &SecureInputHolder, computer: &str) -> String {
    format!(
        "{app} is blocking Mouser from typing on {computer} (Secure Input). \
         Switch away from {app} or close it and typing works again.",
        app = holder.app_name
    )
}

impl Ctx {
    /// Raise (or clear) the Secure Input problem of `computer`.
    pub(super) fn show_secure_input(
        &self,
        computer: ComputerId,
        name: &str,
        holder: Option<&SecureInputHolder>,
    ) {
        let subject = computer.to_string();
        self.problems.clear(ProblemKind::SecureInput, &subject);
        if let Some(holder) = holder {
            self.problems.raise(
                ProblemKind::SecureInput,
                &subject,
                computer,
                message(holder, name),
                None,
            );
        }
    }

    /// Poll this computer's Secure Input holder; on a change, tell the user.
    /// The desk-mates hear of it through the next status exchange.
    pub(crate) fn sync_secure_input(&self) {
        let holder = self.platform.secure_input();
        let name = {
            let mut core = self.core.guard();
            if core.secure_holder == holder {
                return;
            }
            core.secure_holder = holder.clone();
            self.display_name_in(&core)
        };
        self.show_secure_input(
            self.me(),
            &name,
            holder
                .map(|h| SecureInputHolder {
                    pid: h.pid,
                    app_name: h.app_name,
                })
                .as_ref(),
        );
    }
}
