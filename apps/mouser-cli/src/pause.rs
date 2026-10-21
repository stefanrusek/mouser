//! `pause` and `resume`: stop or restart crossing on every computer (Story 2 sc. 12).

use mouser_proto::control::Method;
use serde_json::json;

use crate::{Failure, Mouser};

pub async fn run(mouser: &mut Mouser, pause: bool) -> Result<(), Failure> {
    let method = if pause {
        Method::ActivePause
    } else {
        Method::ActiveResume
    };
    let value = mouser.call(method, json!({})).await?;
    mouser.print(&value, || if pause { "Paused" } else { "Resumed" }.into());
    Ok(())
}
