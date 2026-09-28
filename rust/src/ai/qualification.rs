//! Local receipts for successful, user-authorized provider operations.
//!
//! A receipt is deliberately small: it identifies only the official provider,
//! runtime version, authentication route, and capability.  It never stores an
//! account identifier, credential, prompt, artwork, response, or filesystem
//! path.  A version or route change therefore returns the capability to the
//! explicit first-use qualification state.

use crate::ai::types::{Capability, ProviderId, ValidatedClient};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const RECEIPT_SCHEMA: u8 = 1;
const MAX_RECEIPTS: usize = 24;
const MAX_RECEIPT_FILE_BYTES: u64 = 16 * 1024;
const OFFICIAL_SUBSCRIPTION_ROUTE: &str = "official_subscription_runtime";

/// Evidence that one bounded, explicit operation completed through an
/// isolated official subscription runtime.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityReceipt {
    schema: u8,
    provider: ProviderId,
    runtime_version: String,
    auth_route: String,
    capability: Capability,
    verified_at_unix_seconds: i64,
}

impl CapabilityReceipt {
    fn successful(client: &ValidatedClient, capability: Capability) -> Self {
        Self {
            schema: RECEIPT_SCHEMA,
            provider: client.provider,
            runtime_version: client.version.clone(),
            auth_route: OFFICIAL_SUBSCRIPTION_ROUTE.into(),
            capability,
            verified_at_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .unwrap_or(0),
        }
    }

    fn matches(&self, client: &ValidatedClient, capability: Capability) -> bool {
        self.schema == RECEIPT_SCHEMA
            && self.provider == client.provider
            && self.runtime_version == client.version
            && self.auth_route == OFFICIAL_SUBSCRIPTION_ROUTE
            && self.capability == capability
    }

    fn valid_for_storage(&self) -> bool {
        self.schema == RECEIPT_SCHEMA
            && !self.runtime_version.is_empty()
            && self.runtime_version.len() <= 256
            && self.auth_route == OFFICIAL_SUBSCRIPTION_ROUTE
            && self.verified_at_unix_seconds >= 0
    }
}

/// Bounded capability receipts loaded from the private Omuse configuration
/// directory. Corrupt or oversized state is treated as no qualification,
/// never as verification.
#[derive(Clone, Debug, Default)]
pub struct QualificationReceipts {
    entries: Vec<CapabilityReceipt>,
}

impl QualificationReceipts {
    pub fn load(path: &Path) -> Self {
        let Some(bytes) = fs::metadata(path)
            .ok()
            .filter(|metadata| metadata.is_file() && metadata.len() <= MAX_RECEIPT_FILE_BYTES)
            .and_then(|_| fs::read(path).ok())
        else {
            return Self::default();
        };
        let Ok(entries) = serde_json::from_slice::<Vec<CapabilityReceipt>>(&bytes) else {
            return Self::default();
        };
        let mut entries = entries
            .into_iter()
            .filter(CapabilityReceipt::valid_for_storage)
            .collect::<Vec<_>>();
        entries.sort();
        entries.dedup_by(|newer, older| {
            if newer.provider == older.provider
                && newer.runtime_version == older.runtime_version
                && newer.auth_route == older.auth_route
                && newer.capability == older.capability
            {
                if newer.verified_at_unix_seconds < older.verified_at_unix_seconds {
                    std::mem::swap(newer, older);
                }
                true
            } else {
                false
            }
        });
        if entries.len() > MAX_RECEIPTS {
            entries.drain(..entries.len() - MAX_RECEIPTS);
        }
        Self { entries }
    }

    pub fn contains(&self, client: &ValidatedClient, capability: Capability) -> bool {
        self.entries
            .iter()
            .any(|receipt| receipt.matches(client, capability))
    }

    /// Returns true only when a newly completed operation adds or refreshes
    /// the current provider/version/route/capability receipt.
    pub fn record_success(&mut self, client: &ValidatedClient, capability: Capability) -> bool {
        if !client.operation_profile_qualified {
            return false;
        }
        let receipt = CapabilityReceipt::successful(client, capability);
        self.entries.retain(|existing| {
            !existing.matches(client, capability)
                && !(existing.provider == receipt.provider
                    && existing.auth_route == receipt.auth_route
                    && existing.capability == receipt.capability
                    && existing.runtime_version == receipt.runtime_version)
        });
        self.entries.push(receipt);
        self.entries.sort();
        if self.entries.len() > MAX_RECEIPTS {
            self.entries.drain(..self.entries.len() - MAX_RECEIPTS);
        }
        true
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let Some(parent) = path.parent() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Capability receipt path has no parent directory",
            ));
        };
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(
            ".ai-capability-receipts-{}.json",
            uuid::Uuid::new_v4()
        ));
        let write_result = (|| -> io::Result<()> {
            let bytes = serde_json::to_vec(&self.entries).map_err(io::Error::other)?;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            use std::io::Write;
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }

    #[cfg(test)]
    pub(crate) fn fixture(client: &ValidatedClient, capability: Capability) -> Self {
        Self {
            entries: vec![CapabilityReceipt::successful(client, capability)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(version: &str) -> ValidatedClient {
        let mut client =
            ValidatedClient::fixture(ProviderId::CodexSubscription, "/bin/true".into());
        client.version = version.into();
        client
    }

    #[test]
    fn receipt_matches_only_the_same_provider_version_route_and_capability() {
        let v1 = client("1.2.3");
        let v2 = client("1.2.4");
        let receipts = QualificationReceipts::fixture(&v1, Capability::ImageGeneration);

        assert!(receipts.contains(&v1, Capability::ImageGeneration));
        assert!(!receipts.contains(&v1, Capability::ImageEditing));
        assert!(!receipts.contains(&v2, Capability::ImageGeneration));
    }

    #[test]
    fn persisted_receipts_are_bounded_and_contain_no_request_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipts.json");
        let client = client("1.2.3");
        let mut receipts = QualificationReceipts::default();
        assert!(receipts.record_success(&client, Capability::AssistantStreaming));
        receipts.save(&path).unwrap();

        let bytes = fs::read_to_string(&path).unwrap();
        let stored: serde_json::Value = serde_json::from_str(&bytes).unwrap();
        assert_eq!(stored[0]["authRoute"], "official_subscription_runtime");
        assert!(!bytes.contains("prompt"));
        assert!(!bytes.contains("artwork"));
        assert!(
            QualificationReceipts::load(&path).contains(&client, Capability::AssistantStreaming)
        );
    }
}
