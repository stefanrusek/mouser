//! Secret Service when the session offers one, else 0600 files in the config
//! dir.

use crate::bus::{block_on, connect};
use mouser_platform::{Result, SecretStore};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use zbus::zvariant::{Dict, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

const DEST: &str = "org.freedesktop.secrets";
const SERVICE_PATH: &str = "/org/freedesktop/secrets";
const SERVICE_IFACE: &str = "org.freedesktop.Secret.Service";
const COLLECTION_IFACE: &str = "org.freedesktop.Secret.Collection";
const ITEM_IFACE: &str = "org.freedesktop.Secret.Item";
const APPLICATION: &str = "com.mouser.app";
/// The "no prompt needed" prompt path.
const NO_PROMPT: &str = "/";

/// `$XDG_CONFIG_HOME`, else `$HOME/.config`.
pub(crate) fn config_dir(env: &HashMap<String, String>) -> PathBuf {
    match env.get("XDG_CONFIG_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env.get("HOME").expect("HOME")).join(".config"),
    }
}

/// The Secret Service store when one answers and its default collection is
/// unlocked, else the file store.
pub(crate) fn open(env: &HashMap<String, String>) -> Box<dyn SecretStore> {
    let dir = config_dir(env).join("mouser/secrets");
    let env = env.clone();
    match block_on(async move { SecretService::connect(&env).await }) {
        Some(service) => Box::new(service),
        None => Box::new(FileSecrets { dir }),
    }
}

/// One 0600 file per secret.
pub(crate) struct FileSecrets {
    pub(crate) dir: PathBuf,
}

impl FileSecrets {
    /// The file for `name`; a name such as `desk_key/<id>` must stay one file.
    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name.replace('%', "%25").replace('/', "%2F"))
    }
}

impl SecretStore for FileSecrets {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        Ok(std::fs::read(self.file(name))
            .map(Some)
            .unwrap_or_else(|e| {
                assert_eq!(e.kind(), std::io::ErrorKind::NotFound, "read secret: {e}");
                None
            }))
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.dir).expect("create secrets dir");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.file(name))
            .expect("open secret file");
        file.write_all(value).expect("write secret");
        Ok(())
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        let path = self.file(name);
        if path.exists() {
            std::fs::remove_file(path).expect("remove secret");
        }
        Ok(())
    }
}

type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

/// A plain-text Secret Service session on the default collection.
#[derive(Clone)]
pub(crate) struct SecretService {
    conn: Connection,
    session: OwnedObjectPath,
    collection: OwnedObjectPath,
}

impl SecretService {
    async fn connect(env: &HashMap<String, String>) -> Option<Self> {
        let conn = connect(env).await.ok()?;
        let service = proxy(&conn, SERVICE_PATH, SERVICE_IFACE).await;
        let (_, session): (OwnedValue, OwnedObjectPath) = service
            .call("OpenSession", &("plain", Value::from("")))
            .await
            .ok()?;
        let collection: OwnedObjectPath = service.call("ReadAlias", &("default",)).await.ok()?;
        let locked: bool = proxy(&conn, collection.as_str(), COLLECTION_IFACE)
            .await
            .get_property("Locked")
            .await
            .ok()?;
        (collection.as_str() != NO_PROMPT && !locked).then_some(SecretService {
            conn,
            session,
            collection,
        })
    }

    async fn get(&self, name: &str) -> Option<Vec<u8>> {
        let found = self.find(name).await;
        let item = found.first()?;
        let (_, _, value, _): Secret = proxy(&self.conn, item.as_str(), ITEM_IFACE)
            .await
            .call("GetSecret", &(&self.session,))
            .await
            .expect("get secret");
        Some(value)
    }

    async fn set(&self, name: &str, value: &[u8]) {
        let properties = HashMap::from([
            (
                "org.freedesktop.Secret.Item.Label",
                Value::from(format!("Mouser {name}")),
            ),
            (
                "org.freedesktop.Secret.Item.Attributes",
                Value::from(Dict::from(attributes(name))),
            ),
        ]);
        let secret: Secret = (
            self.session.clone(),
            Vec::new(),
            value.to_vec(),
            "application/octet-stream".into(),
        );
        let (_, prompt): (OwnedObjectPath, OwnedObjectPath) =
            proxy(&self.conn, self.collection.as_str(), COLLECTION_IFACE)
                .await
                .call("CreateItem", &(properties, secret, true))
                .await
                .expect("store secret");
        assert_eq!(
            prompt.as_str(),
            NO_PROMPT,
            "secret store asked for a prompt"
        );
    }

    async fn delete(&self, name: &str) {
        for item in self.find(name).await {
            let prompt: OwnedObjectPath = proxy(&self.conn, item.as_str(), ITEM_IFACE)
                .await
                .call("Delete", &())
                .await
                .expect("delete secret");
            assert_eq!(
                prompt.as_str(),
                NO_PROMPT,
                "secret store asked for a prompt"
            );
        }
    }

    async fn find(&self, name: &str) -> Vec<OwnedObjectPath> {
        proxy(&self.conn, self.collection.as_str(), COLLECTION_IFACE)
            .await
            .call("SearchItems", &(attributes(name),))
            .await
            .expect("search secrets")
    }
}

async fn proxy<'a>(conn: &Connection, path: &'a str, iface: &'a str) -> Proxy<'a> {
    Proxy::new(conn, DEST, path, iface)
        .await
        .expect("secret service proxy")
}

fn attributes(name: &str) -> HashMap<String, String> {
    HashMap::from([
        ("application".to_string(), APPLICATION.to_string()),
        ("name".to_string(), name.to_string()),
    ])
}

impl SecretStore for SecretService {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let (this, name) = (self.clone(), name.to_string());
        Ok(block_on(
            async move { SecretService::get(&this, &name).await },
        ))
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        let (this, name, value) = (self.clone(), name.to_string(), value.to_vec());
        block_on(async move { SecretService::set(&this, &name, &value).await });
        Ok(())
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        let (this, name) = (self.clone(), name.to_string());
        block_on(async move { SecretService::delete(&this, &name).await });
        Ok(())
    }
}
