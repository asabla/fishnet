use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use crate::contract::{ApiError, Phase, Result, Submission, validate_key};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema_version: u32,
    pub decision: Option<bool>,
    pub approval_wait_ms: Option<u64>,
    pub task_id: String,
    pub submission: Submission,
    pub fingerprint: String,
    pub phase: Phase,
    pub message: Option<String>,
    pub prepared: Option<fishnet_core::workspace::TaskFile>,
    pub provider: crate::ProviderAccess,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub schema_version: u32,
    pub endpoint: String,
    pub token: String,
}
impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

pub(super) struct Store {
    root: PathBuf,
    _lock: File,
    token: String,
}
impl Store {
    pub(super) fn open(root: PathBuf) -> Result<Self> {
        for ancestor in root.ancestors() {
            if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(ApiError::new(
                    "unsafe_credentials",
                    "service path ancestors must not be symlinks",
                ));
            }
        }
        private_directory(&root)?;
        private_directory(&root.join("requests"))?;
        let lock_path = root.join("service.lock");
        let lock = create_private(&lock_path, false)?;
        lock.try_lock()
            .map_err(|_| ApiError::new("store_locked", "another service owns this store"))?;
        recover_pending(&root)?;
        let path = root.join("token");
        let token = if fs::symlink_metadata(&path).is_ok() {
            String::from_utf8(read_private(&path, 128)?).map_err(|_| ApiError::storage())?
        } else {
            let mut bytes = [0; 32];
            getrandom::getrandom(&mut bytes).map_err(|_| ApiError::storage())?;
            let token = URL_SAFE_NO_PAD.encode(bytes);
            atomic_write(&path, token.as_bytes())?;
            token
        };
        if token.len() != 43
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(ApiError::new(
                "unsafe_credentials",
                "invalid service credential file",
            ));
        }
        Ok(Self {
            root,
            _lock: lock,
            token,
        })
    }

    pub(super) fn token(&self) -> &str {
        &self.token
    }

    pub(super) fn save(&self, receipt: &Receipt) -> Result<()> {
        validate_key(&receipt.submission.request_id)?;
        let bytes = serde_json::to_vec(receipt).map_err(|_| ApiError::storage())?;
        if bytes.len() > 131_072 {
            return Err(ApiError::invalid("submission exceeds persistence bound"));
        }
        atomic_write(
            &self
                .root
                .join("requests")
                .join(format!("{}.json", receipt.submission.request_id)),
            &bytes,
        )
    }

    pub(super) fn load(&self) -> Result<Vec<Receipt>> {
        let mut receipts = Vec::new();
        for (count, entry) in fs::read_dir(self.root.join("requests"))
            .map_err(|_| ApiError::storage())?
            .enumerate()
        {
            if count >= 256 {
                return Err(ApiError::storage());
            }
            if receipts.len() == 128 {
                return Err(ApiError::new("capacity", "service receipt store is full"));
            }
            let path = entry.map_err(|_| ApiError::storage())?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "pending")
            {
                continue;
            }
            let bytes = read_private(&path, 131_072)?;
            let receipt: Receipt =
                serde_json::from_slice(&bytes).map_err(|_| ApiError::storage())?;
            validate_key(&receipt.submission.request_id)?;
            validate_key(&receipt.task_id)?;
            if receipt.schema_version != 1
                || receipt.fingerprint
                    != fishnet_core::provider::fingerprint(
                        &serde_json::to_vec(&receipt.submission)
                            .map_err(|_| ApiError::storage())?,
                    )
            {
                return Err(ApiError::storage());
            }
            if path.file_name()
                != Some(std::ffi::OsStr::new(&format!(
                    "{}.json",
                    receipt.submission.request_id
                )))
            {
                return Err(ApiError::storage());
            }
            receipts.push(receipt);
        }
        Ok(receipts)
    }

    pub(super) fn publish(&self, endpoint: String) -> Result<PathBuf> {
        let path = self.root.join("connection.json");
        let bytes = serde_json::to_vec(&Connection {
            schema_version: 1,
            endpoint,
            token: self.token.clone(),
        })
        .map_err(|_| ApiError::storage())?;
        atomic_write(&path, &bytes)?;
        Ok(path)
    }
}

/// Read a service capability file. Do not print or forward its token.
///
/// # Errors
/// Rejects unsafe permissions, links, oversized files, and incompatible records.
pub fn read_connection(path: &Path) -> Result<Connection> {
    let value: Connection = serde_json::from_slice(&read_private(path, 4_096)?)
        .map_err(|_| ApiError::invalid("invalid connection file"))?;
    if value.schema_version != 1 || value.token.len() != 43 {
        return Err(ApiError::invalid("unsupported or invalid connection file"));
    }
    Ok(value)
}

fn check_path(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ApiError::storage())?;
    if metadata.file_type().is_symlink()
        || metadata.is_dir() != directory
        || (!directory && !metadata.is_file())
    {
        return Err(ApiError::new(
            "unsafe_credentials",
            "service paths must not be symlinks or special files",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(ApiError::new(
                "unsafe_credentials",
                "service files require private permissions",
            ));
        }
    }
    Ok(())
}

fn private_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .recursive(true)
            .create(path)
            .map_err(|_| ApiError::storage())?;
    }
    check_path(path, true)
}

pub(super) fn launch_log(directory: &Path) -> Result<File> {
    for ancestor in directory.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(ApiError::new(
                "unsafe_credentials",
                "service path must not contain symlinks",
            ));
        }
    }
    private_directory(directory)?;
    let path = directory.join("daemon.log");
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.len() > 65_536) {
        check_path(&path, false)?;
        fs::rename(&path, directory.join("daemon.previous.log"))
            .map_err(|_| ApiError::storage())?;
    }
    let mut log = create_private(&path, false)?;
    // Each startup writes only a banner or an error, never task or account content.
    log.seek(std::io::SeekFrom::End(0))
        .map_err(|_| ApiError::storage())?;
    Ok(log)
}

fn create_private(path: &Path, exclusive: bool) -> Result<File> {
    if fs::symlink_metadata(path).is_ok() {
        check_path(path, false)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).read(true);
    if exclusive {
        options.create_new(true);
    } else {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|_| ApiError::storage())
}

fn read_private(path: &Path, limit: u64) -> Result<Vec<u8>> {
    check_path(path, false)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| ApiError::storage())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ApiError::storage())?;
    if bytes.len() as u64 > limit {
        return Err(ApiError::storage());
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension("pending");
    let mut file = create_private(&temporary, true)?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temporary, path));
    let _ = fs::remove_file(&temporary);
    result.map_err(|_| ApiError::storage())?;
    #[cfg(unix)]
    File::open(path.parent().ok_or_else(ApiError::storage)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| ApiError::storage())?;
    Ok(())
}

fn recover_pending(root: &Path) -> Result<()> {
    let incidents = root.join("incidents");
    private_directory(&incidents)?;
    for directory in [root.to_path_buf(), root.join("requests")] {
        for (count, entry) in fs::read_dir(&directory)
            .map_err(|_| ApiError::storage())?
            .enumerate()
        {
            if count >= 256 {
                return Err(ApiError::storage());
            }
            let path = entry.map_err(|_| ApiError::storage())?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "pending")
            {
                check_path(&path, false)?;
                let name = path
                    .file_name()
                    .ok_or_else(ApiError::storage)?
                    .to_string_lossy();
                let archived = incidents.join(format!("{name}.{}", crate::new_id()?));
                // Preserve an incomplete local metadata write. Never apply it or replay task work.
                fs::rename(&path, archived).map_err(|_| ApiError::storage())?;
            }
        }
        #[cfg(unix)]
        File::open(&directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| ApiError::storage())?;
    }
    #[cfg(unix)]
    File::open(incidents)
        .and_then(|f| f.sync_all())
        .map_err(|_| ApiError::storage())?;
    Ok(())
}
