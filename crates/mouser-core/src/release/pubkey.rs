//! The Mouser updater public key, compiled in: the `plugins.updater.pubkey`
//! of `apps/mouser-app/tauri.conf.json` (base64 of the minisign `.pub` file).
//! Only releases signed with its private half are ever installed.

pub const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEQ3NTIwRUQyNkVDMEY1NkUKUldSdTljQnUwZzVTMTJXeWNXZDRXOTZEUGxhS2VVN2J2VzZDZEc2cUl0YThnZGdDekt5dUxCbE4K";
