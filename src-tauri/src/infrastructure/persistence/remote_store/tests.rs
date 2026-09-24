use super::*;
use crate::domain::remote::Remote;
use std::fs;

#[test]
fn store_survives_reload_and_rejects_duplicates() {
    fn remote(name: &str) -> Remote {
        Remote {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            url: "http://pc2.local:18032".into(),
            token: "fxp_test-token".into(),
            preset: None,
            created_at: chrono::Local::now().to_rfc3339(),
        }
    }

    let dir = std::env::temp_dir().join(format!("ixprint-remote-{}", uuid::Uuid::new_v4()));
    let store = RemoteStore::load(&dir);
    store
        .update(|list| {
            list.push(remote("PC2"));
            Ok(())
        })
        .unwrap();
    assert_eq!(RemoteStore::load(&dir).list().len(), 1);
    assert!(store
        .update(|list| {
            list.push(remote("pc2"));
            Ok(())
        })
        .is_err());
    fs::write(dir.join("remotes.json"), "broken").unwrap();
    assert!(RemoteStore::load(&dir).list().is_empty());
    let _ = fs::remove_dir_all(dir);
}
