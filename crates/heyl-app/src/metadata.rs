//! List or search live login metadata without opening protected fields.

use heyl_domain::VaultId;
use heyl_vault::login::{Liveness, Login};

use crate::{AppError, Ports, get, session::Slot};

/// A login's identifiers and searchable metadata.
#[derive(serde::Serialize)]
pub struct Entry {
    /// Vault containing the login.
    pub vault_id: String,
    /// Login identifier within the vault.
    pub login_id: String,
    /// Entry title, falling back to its display name.
    pub title: String,
    /// Website hosts, with duplicates removed.
    pub domains: Vec<String>,
}

/// Metadata results, including any vaults skipped during an account-wide scan.
#[derive(serde::Serialize)]
pub struct Report {
    /// Active entries matching the filter.
    pub entries: Vec<Entry>,
    /// Vault identifiers omitted because they could not be read.
    pub skipped_vaults: Vec<String>,
}

/// List live entries in scope, optionally matching a case-insensitive substring.
///
/// # Errors
/// Returns unlock and vault selection failures, and read failures for an explicitly selected vault.
pub async fn run(
    ports: &Ports<'_>,
    slot: &Slot,
    selector: &get::VaultSelector,
    search: Option<&str>,
    wait: Option<u64>,
) -> Result<Report, AppError> {
    let session = crate::unlock::ensure(ports, slot, wait).await?;
    let scope = get::resolve_scope(ports, &session, selector).await?;
    let mut report = Report {
        entries: Vec::new(),
        skipped_vaults: Vec::new(),
    };
    for vault in scope {
        match get::open_vault(ports, &session, vault).await {
            Ok(opened) => report.entries.extend(select(
                vault.id,
                heyl_vault::login::logins(&opened.document),
                search,
            )),
            Err(_) if matches!(selector, get::VaultSelector::Any) => {
                report.skipped_vaults.push(vault.id.to_string());
            }
            Err(err) => return Err(err),
        }
    }
    Ok(report)
}

/// Project parsed logins into metadata. URL credentials, paths, and queries are omitted.
#[must_use]
pub fn select(vault_id: VaultId, logins: Vec<Login>, search: Option<&str>) -> Vec<Entry> {
    let search = search.unwrap_or_default().trim().to_lowercase();
    let mut entries = Vec::new();
    for login in logins {
        if login.liveness() != Liveness::Live {
            continue;
        }

        let mut domains: Vec<_> = login
            .websites
            .iter()
            .filter_map(|site| domain(site))
            .collect();
        domains.sort_unstable();
        domains.dedup();

        let display = login.display_headline.unwrap_or_default();
        if !search.is_empty()
            && !login.title.to_lowercase().contains(&search)
            && !display.to_lowercase().contains(&search)
            && !domains.iter().any(|domain| domain.contains(&search))
        {
            continue;
        }
        let title = if login.title.is_empty() {
            display
        } else {
            login.title
        };
        entries.push(Entry {
            vault_id: vault_id.to_string(),
            login_id: login.id.to_string(),
            title,
            domains,
        });
    }
    entries.sort_unstable_by(|a, b| a.title.cmp(&b.title).then(a.login_id.cmp(&b.login_id)));
    entries
}

fn domain(site: &str) -> Option<String> {
    let site = site.trim();
    let address = match site.split_once("://") {
        Some((scheme, rest)) if !scheme.contains(['/', '?', '#']) => {
            if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
                return None;
            }
            rest
        }
        _ => site,
    };
    let raw = address.split(['/', '?', '#']).next()?;
    if raw.contains(['@', '\\']) {
        return None;
    }
    let authority = raw.parse::<http::uri::Authority>().ok()?;
    let host = authority.host();
    if raw.len() != host.len() && authority.port_u16().is_none() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}
