//! `send-cad`: send Ctrl-Alt-Del to the computer being controlled (Story 12 sc. 4).

use mouser_proto::control::Method;
use serde_json::json;

use crate::{Failure, Mouser};

pub async fn run(mouser: &mut Mouser) -> Result<(), Failure> {
    let value = mouser.call(Method::SendCad, json!({})).await?;
    mouser.print(&value, || "Sent Ctrl-Alt-Del".into());
    Ok(())
}
