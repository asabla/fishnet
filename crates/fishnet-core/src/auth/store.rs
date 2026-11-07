use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{AuthResult, failure, random_value};

#[derive(Deserialize, Serialize)]
pub(super) struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_at: u64,
    pub earliest_refresh_at: u64,
    pub scopes: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Account {
    pub schema_version: u32,
    pub client_id: String,
    pub subject: String,
    pub email: Option<String>,
    pub host_id: String,
    pub tokens: Option<Tokens>,
    pub refresh_uncertain: bool,
}

pub(super) struct Store {
    directory: PathBuf,
    _lock: File,
}

impl Store {
    pub(super) fn open(directory: &Path) -> AuthResult<Self> {
        for ancestor in directory.ancestors() {
            if fs::symlink_metadata(ancestor)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                return Err(failure(
                    "authentication path ancestors must not be symlinks",
                ));
            }
        }
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(directory)
            .map_err(|_| failure("cannot create private authentication directory"))?;
        check_private(directory)?;
        let path = directory.join(".session.lock");
        if private_exists(&path)? {
            check_private(&path)?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options
            .open(path)
            .map_err(|_| failure("cannot open authentication lock"))?;
        lock.try_lock().map_err(|_| {
            failure("another authentication operation owns this store; try again after it finishes")
        })?;
        Ok(Self {
            directory: directory.into(),
            _lock: lock,
        })
    }

    pub(super) fn load(&self, label: &str) -> AuthResult<Option<Account>> {
        let path = self.account_path(label)?;
        if !private_exists(&path)? {
            return Ok(None);
        }
        let account: Account = read_private(&path)?;
        if account.schema_version != 1 || !account.client_id.starts_with("oaiapp_") {
            return Err(failure("unsupported account record"));
        }
        Ok(Some(account))
    }

    pub(super) fn save(&self, label: &str, account: &Account) -> AuthResult<()> {
        write_private(&self.account_path(label)?, account)
    }

    pub(super) fn host_id(&self) -> AuthResult<String> {
        let path = self.directory.join("host.json");
        if private_exists(&path)? {
            return read_private(&path);
        }
        let mut bytes = [0_u8; 16];
        getrandom::getrandom(&mut bytes).map_err(|_| failure("OS random source is unavailable"))?;
        if let Some(byte) = bytes.get_mut(6) {
            *byte = (*byte & 0x0f) | 0x40;
        }
        if let Some(byte) = bytes.get_mut(8) {
            *byte = (*byte & 0x3f) | 0x80;
        }
        let mut id = String::from("urn:uuid:");
        for (index, byte) in bytes.iter().enumerate() {
            use std::fmt::Write as _;
            if [4, 6, 8, 10].contains(&index) {
                id.push('-');
            }
            let _ = write!(id, "{byte:02x}");
        }
        write_private(&path, &id)?;
        Ok(id)
    }

    fn account_path(&self, label: &str) -> AuthResult<PathBuf> {
        if label.is_empty()
            || label.len() > 64
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(failure(
                "account label must contain 1..=64 ASCII letters, digits, or hyphens",
            ));
        }
        Ok(self.directory.join(format!("account-{label}.json")))
    }
}

fn read_private<T: serde::de::DeserializeOwned>(path: &Path) -> AuthResult<T> {
    check_private(path)?;
    let file =
        File::open(path).map_err(|_| failure("cannot read protected authentication record"))?;
    let mut bytes = Vec::new();
    file.take(131_073)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("cannot read authentication record"))?;
    if bytes.len() > 131_072 {
        return Err(failure("authentication record exceeds size bound"));
    }
    serde_json::from_slice(&bytes).map_err(|_| failure("invalid authentication record"))
}

fn check_private(path: &Path) -> AuthResult<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| failure("authentication path unavailable"))?;
    if metadata.file_type().is_symlink() {
        return Err(failure("authentication paths must not be symlinks"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(failure(
                "authentication files and directories must be owner-only",
            ));
        }
    }
    Ok(())
}

fn write_private(path: &Path, value: &impl Serialize) -> AuthResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| failure("authentication path has no parent"))?;
    let pending = parent.join(format!(".pending-{}", random_value()?));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&pending)
            .map_err(|_| failure("cannot prepare authentication record"))?;
        let bytes = serde_json::to_vec(value)
            .map_err(|_| failure("cannot encode authentication record"))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| failure("cannot sync authentication record"))?;
        fs::rename(&pending, path).map_err(|_| failure("cannot replace authentication record"))?;
        #[cfg(unix)]
        File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|_| failure("authentication directory sync failed"))?;
        Ok(())
    })();
    let _ = fs::remove_file(pending);
    result
}

fn private_exists(path: &Path) -> AuthResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            check_private(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(failure("authentication path cannot be inspected")),
    }
}
