//! Change one title in a personal vault through a guarded commit.

use heyl_domain::{LoginId, Timestamp, VaultId, VaultType};
use heyl_vault::Document;
use serde_json::{Value, json};

use crate::{AppError, Ports, get, session::Slot, unlock};

/// Write a title change to one personal vault and read it back.
///
/// # Errors
/// Returns errors if the vault cannot be opened, the entry or expected title
/// does not match, the commit fails, or the result cannot be confirmed.
pub async fn run(
    ports: &Ports<'_>,
    slot: &Slot,
    vault_id: VaultId,
    login_id: LoginId,
    expected: &str,
    title: &str,
    wait: Option<u64>,
) -> Result<(), AppError> {
    let session = unlock::ensure(ports, slot, wait).await?;
    let vault = session
        .sync
        .vaults
        .iter()
        .find(|vault| vault.id == vault_id && vault.vault_type == VaultType::Private)
        .ok_or(AppError::NoSuchVault)?;
    let opened = get::open_vault(ports, &session, vault).await?;
    let now = ports.clock.now();
    let Some(delta) = title_delta(&opened.document, login_id, expected, title, now)? else {
        return Ok(());
    };
    let latest = opened.latest_commit.ok_or(AppError::NoSuchLogin)?;
    let blob = heyl_vault::encode(&delta).map_err(|source| AppError::VaultContent {
        vault: vault_id,
        source,
    })?;
    let sealed = opened.vault_key.key().encrypt(&ports.random.nonce(), &blob);
    ports
        .api
        .create_commit(vault_id, latest, sealed, now)
        .await?;
    let confirmed = get::open_vault(ports, &session, vault)
        .await
        .map_err(|_| AppError::TitleUnverified)?;
    if current_title(&confirmed.document, login_id).ok() != Some(title) {
        return Err(AppError::TitleUnverified);
    }
    Ok(())
}

/// Build a delta containing only the targeted login, retaining unknown fields.
///
/// # Errors
/// Rejects a missing or archived login, an unexpected title, a blank replacement,
/// or a timestamp that would lose to the current entry during sync.
pub fn title_delta(
    vault: &Document,
    login_id: LoginId,
    expected: &str,
    title: &str,
    now: Timestamp,
) -> Result<Option<Document>, AppError> {
    if title.trim().is_empty() {
        return Err(AppError::BlankTitle);
    }
    let current = current_title(vault, login_id)?;
    if current != expected {
        return Err(AppError::TitleConflict);
    }
    if current == title {
        return Ok(None);
    }
    let id = login_id.to_string();
    let mut entry = vault.content["logins"][&id].clone();
    let timestamp = now.to_string();
    let previous = entry["updateTime"]
        .as_str()
        .ok_or(AppError::TitleConflict)?;
    if previous >= timestamp.as_str() {
        return Err(AppError::TitleConflict);
    }
    entry["title"] = json!(title);
    entry["editTime"] = json!(&timestamp);
    entry["updateTime"] = json!(timestamp);
    let mut delta = Document::empty();
    delta.format = vault.format;
    delta.document_type.clone_from(&vault.document_type);
    delta.version = vault.version;
    delta
        .content
        .insert("logins".into(), json!({ (id): entry }));
    Ok(Some(delta))
}

fn current_title(vault: &Document, login_id: LoginId) -> Result<&str, AppError> {
    let login = vault
        .content
        .get("logins")
        .and_then(|logins| logins.get(login_id.to_string()))
        .and_then(Value::as_object)
        .filter(|login| {
            login.get("isDeleted") != Some(&Value::Bool(true))
                && login.get("isArchived") != Some(&Value::Bool(true))
        })
        .ok_or(AppError::NoSuchLogin)?;
    match login.get("title") {
        None => Ok(""),
        Some(value) => value.as_str().ok_or(AppError::TitleConflict),
    }
}
