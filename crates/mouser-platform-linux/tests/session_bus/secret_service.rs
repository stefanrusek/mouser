//! A fake `org.freedesktop.secrets` with plain sessions and one collection.

use crate::daemon::FAKES;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, ObjectServer, interface};

const COLLECTION: &str = "/org/freedesktop/secrets/collection/login";

#[derive(Clone, Copy)]
pub struct SecretServiceConfig {
    pub locked: bool,
    /// Whether a default collection exists at all.
    pub has_default: bool,
}

type Attributes = HashMap<String, String>;

#[derive(Default)]
struct Items {
    next: u32,
    by_path: HashMap<String, (Attributes, Vec<u8>)>,
}

type Shared = Arc<Mutex<Items>>;

struct Service(SecretServiceConfig);

#[interface(name = "org.freedesktop.Secret.Service")]
impl Service {
    fn open_session(&self, algorithm: String, _input: Value<'_>) -> (OwnedValue, OwnedObjectPath) {
        assert_eq!(algorithm, "plain");
        (
            OwnedValue::try_from(Value::from("")).unwrap(),
            OwnedObjectPath::try_from("/org/freedesktop/secrets/session/s1").unwrap(),
        )
    }

    fn read_alias(&self, name: String) -> OwnedObjectPath {
        assert_eq!(name, "default");
        OwnedObjectPath::try_from(if self.0.has_default { COLLECTION } else { "/" }).unwrap()
    }
}

struct Collection(SecretServiceConfig, Shared);

#[interface(name = "org.freedesktop.Secret.Collection")]
impl Collection {
    #[zbus(property, name = "Locked")]
    fn locked(&self) -> bool {
        self.0.locked
    }

    fn search_items(&self, attributes: Attributes) -> Vec<OwnedObjectPath> {
        let items = self.1.lock().unwrap();
        let mut found: Vec<_> = items
            .by_path
            .iter()
            .filter(|(_, (attrs, _))| attributes.iter().all(|(k, v)| attrs.get(k) == Some(v)))
            .map(|(path, _)| OwnedObjectPath::try_from(path.as_str()).unwrap())
            .collect();
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        found
    }

    async fn create_item(
        &self,
        properties: HashMap<String, OwnedValue>,
        secret: (OwnedObjectPath, Vec<u8>, Vec<u8>, String),
        replace: bool,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        assert!(replace);
        assert_eq!(secret.0.as_str(), "/org/freedesktop/secrets/session/s1");
        let attributes =
            Attributes::try_from(properties["org.freedesktop.Secret.Item.Attributes"].clone())
                .unwrap();
        let existing = {
            let items = self.1.lock().unwrap();
            items
                .by_path
                .iter()
                .find(|(_, (attrs, _))| *attrs == attributes)
                .map(|(path, _)| path.clone())
        };
        let (path, is_new) = match existing {
            Some(path) => (path, false),
            None => {
                let mut items = self.1.lock().unwrap();
                items.next += 1;
                (format!("{COLLECTION}/{}", items.next), true)
            }
        };
        self.1
            .lock()
            .unwrap()
            .by_path
            .insert(path.clone(), (attributes, secret.2));
        if is_new {
            server
                .at(path.as_str(), Item(self.1.clone(), path.clone()))
                .await
                .unwrap();
        }
        (
            OwnedObjectPath::try_from(path).unwrap(),
            OwnedObjectPath::try_from("/").unwrap(),
        )
    }
}

struct Item(Shared, String);

#[interface(name = "org.freedesktop.Secret.Item")]
impl Item {
    fn get_secret(&self, session: ObjectPath<'_>) -> (OwnedObjectPath, Vec<u8>, Vec<u8>, String) {
        let value = self.0.lock().unwrap().by_path[&self.1].1.clone();
        (
            OwnedObjectPath::from(session),
            Vec::new(),
            value,
            "application/octet-stream".into(),
        )
    }

    async fn delete(&self, #[zbus(object_server)] server: &ObjectServer) -> OwnedObjectPath {
        self.0.lock().unwrap().by_path.remove(&self.1);
        server.remove::<Item, _>(self.1.as_str()).await.unwrap();
        OwnedObjectPath::try_from("/").unwrap()
    }
}

pub struct SecretService {
    _conn: Connection,
}

impl SecretService {
    pub fn start(address: &str, config: SecretServiceConfig) -> Self {
        let items = Shared::default();
        let conn = FAKES
            .block_on(
                zbus::connection::Builder::address(address)
                    .unwrap()
                    .name("org.freedesktop.secrets")
                    .unwrap()
                    .serve_at("/org/freedesktop/secrets", Service(config))
                    .unwrap()
                    .serve_at(COLLECTION, Collection(config, items))
                    .unwrap()
                    .build(),
            )
            .unwrap();
        SecretService { _conn: conn }
    }
}
