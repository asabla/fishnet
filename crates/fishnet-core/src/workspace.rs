use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_EDIT: AtomicU64 = AtomicU64::new(0);
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::control::Cancellation;
use crate::process::{self, CommandSpec, ProcessOutput};
use crate::protocol::{BoundedText, ErrorKind, OperationError, ToolCall, ToolResult};
use crate::provider::fingerprint;

pub const POLICY: &str = "workspace-explicit-v1";
const MAX_FILE_BYTES: u64 = 1_048_576;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSpec {
    pub root: PathBuf,
    pub allowed_writes: Vec<String>,
    pub required_changes: Vec<String>,
    pub protected_paths: Vec<String>,
    pub commands: Vec<CommandSpec>,
    pub checks: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceCheck {
    pub command: CommandSpec,
    pub output: ProcessOutput,
    pub changed_paths: Vec<String>,
    pub protected_unchanged: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceSnapshot {
    pub starting_files: BTreeMap<String, Option<String>>,
    pub protected_files: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug)]
pub struct Workspace {
    spec: WorkspaceSpec,
    starting_files: BTreeMap<String, Option<String>>,
    protected_files: BTreeMap<String, Option<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArguments {
    path: String,
    offset: usize,
    max_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceArguments {
    path: String,
    expected_sha256: Option<String>,
    old: String,
    new: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArguments {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandArguments {
    command_id: String,
}

impl Workspace {
    /// # Errors
    /// Rejects invalid roots, path grants, conflicting protection, or command specifications.
    pub fn open(mut spec: WorkspaceSpec) -> Result<Self, OperationError> {
        spec.root = spec
            .root
            .canonicalize()
            .map_err(|_| denied("workspace root is unavailable"))?;
        if !spec.root.is_dir()
            || spec.allowed_writes.len() > 32
            || spec.protected_paths.len() > 64
            || spec.commands.len() > 16
            || spec.checks.len() > 16
        {
            return Err(denied("workspace configuration exceeds supported bounds"));
        }
        let mut workspace = Self {
            spec,
            starting_files: BTreeMap::new(),
            protected_files: BTreeMap::new(),
        };
        workspace.validate_commands()?;
        for path in &workspace.spec.allowed_writes {
            workspace.path(path, true)?;
            if workspace.spec.protected_paths.contains(path) {
                return Err(denied("write grant overlaps a protected file"));
            }
            workspace
                .starting_files
                .insert(path.clone(), workspace.file_hash(path)?);
        }
        for path in &workspace.spec.required_changes {
            if !workspace.spec.allowed_writes.contains(path) {
                return Err(denied("required change has no write grant"));
            }
        }
        for path in &workspace.spec.protected_paths {
            let hash = workspace.file_hash(path)?;
            if hash.is_none() {
                return Err(denied("protected check source is missing"));
            }
            workspace.protected_files.insert(path.clone(), hash);
        }
        Ok(workspace)
    }

    #[must_use]
    pub const fn spec(&self) -> &WorkspaceSpec {
        &self.spec
    }

    /// # Errors
    /// Rejects missing, excessive, or invalid scoped instruction files.
    pub fn instructions(&self) -> Result<String, OperationError> {
        let mut paths = BTreeSet::from([String::from("AGENTS.md")]);
        for path in &self.spec.allowed_writes {
            let mut parent = Path::new(path).parent();
            while let Some(directory) = parent {
                paths.insert(directory.join("AGENTS.md").to_string_lossy().into_owned());
                parent = directory.parent();
            }
        }
        let mut instructions = String::new();
        for path in paths {
            if self.path(&path, true)?.is_file() {
                let text = self.read_text(&path)?;
                let _ = write!(
                    instructions,
                    "\nInstructions for scope {}:\n{text}\n",
                    Path::new(&path)
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .display()
                );
            }
        }
        if instructions.len() > 32_768 {
            return Err(denied("scoped instructions exceed 32768 bytes"));
        }
        Ok(instructions)
    }

    /// Effective arguments are selected from the immutable host contract, before invocation.
    ///
    /// # Errors
    /// Rejects malformed or unlisted command identifiers.
    pub fn command_for(&self, call: &ToolCall) -> Result<Option<CommandSpec>, OperationError> {
        if call.name != "run_command" {
            return Ok(None);
        }
        let args: CommandArguments = arguments(&call.arguments)?;
        self.spec
            .commands
            .iter()
            .find(|command| command.id == args.command_id)
            .cloned()
            .map(Some)
            .ok_or_else(|| denied("command ID is not granted by the task contract"))
    }

    /// # Errors
    /// Returns file, schema, policy, cancellation, deadline, or execution errors.
    pub async fn execute(
        &self,
        call: &ToolCall,
        cancellation: &Cancellation,
        deadline: Instant,
        limit: usize,
    ) -> Result<ToolResult, OperationError> {
        cancellation.check(deadline)?;
        let data = if call.name == "run_command" {
            let command = self
                .command_for(call)?
                .ok_or_else(|| denied("missing effective command"))?;
            let output = process::execute(
                &command,
                &self.spec.root,
                cancellation,
                deadline,
                limit / 12,
            )
            .await?;
            json!({"command":command,"output":output})
        } else {
            let workspace = self.clone();
            let call = call.clone();
            tokio::task::spawn_blocking(move || match call.name.as_str() {
                "read_file" => workspace.read_file(&arguments(&call.arguments)?, limit),
                "list_files" => workspace.list_files(&arguments(&call.arguments)?, limit),
                "replace_text" => workspace.replace_text(arguments(&call.arguments)?),
                _ => Err(denied("tool is not granted by the workspace policy")),
            })
            .await
            .map_err(|_| uncertain("workspace worker stopped; inspect possible effects"))??
        };
        Ok(ToolResult::Output { data })
    }

    pub(crate) async fn verify_changes_async(&self) -> Result<Vec<String>, OperationError> {
        let workspace = self.clone();
        tokio::task::spawn_blocking(move || workspace.verify_changes())
            .await
            .map_err(|_| uncertain("workspace check worker stopped"))?
    }

    #[must_use]
    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            starting_files: self.starting_files.clone(),
            protected_files: self.protected_files.clone(),
        }
    }

    /// Fingerprint relevant content as well as policy and root. A changed file breaks equivalence.
    ///
    /// # Errors
    /// Returns an error when a granted source cannot be inspected safely.
    pub fn relevant_state(&self, call: &ToolCall) -> Result<String, OperationError> {
        let mut state =
            serde_json::to_vec(&self.spec).map_err(|_| denied("cannot encode workspace policy"))?;
        for path in &self.spec.allowed_writes {
            state.extend_from_slice(self.file_hash(path)?.unwrap_or_default().as_bytes());
        }
        if let Some(path) = call.arguments.get("path").and_then(Value::as_str)
            && let Ok(Some(hash)) = self.file_hash(path)
        {
            state.extend_from_slice(hash.as_bytes());
        }
        Ok(fingerprint(&state))
    }

    /// # Errors
    /// Rejects missing required changes or modifications to protected check sources.
    pub fn verify_changes(&self) -> Result<Vec<String>, OperationError> {
        for (path, starting) in &self.protected_files {
            if &self.file_hash(path)? != starting {
                return Err(denied("protected check source changed"));
            }
        }
        let mut changed = Vec::new();
        for (path, starting) in &self.starting_files {
            if &self.file_hash(path)? != starting {
                changed.push(path.clone());
            }
        }
        if self
            .spec
            .required_changes
            .iter()
            .any(|path| !changed.contains(path))
        {
            return Err(conflict("required source changes are missing"));
        }
        Ok(changed)
    }

    fn validate_commands(&self) -> Result<(), OperationError> {
        let mut ids = BTreeSet::new();
        for command in &self.spec.commands {
            process::validate(command)?;
            if let Ok(relative) = command.program.strip_prefix(&self.spec.root)
                && self
                    .spec
                    .allowed_writes
                    .iter()
                    .any(|path| Path::new(path) == relative)
            {
                return Err(denied(
                    "an authorized executable cannot also be a model-writable file",
                ));
            }
            if !ids.insert(&command.id) {
                return Err(denied("duplicate command ID"));
            }
        }
        if self.spec.checks.iter().any(|id| !ids.contains(id)) {
            return Err(denied("check references an unlisted command"));
        }
        Ok(())
    }

    fn path(&self, path: &str, allow_missing: bool) -> Result<PathBuf, OperationError> {
        if path.len() > 256 || path.is_empty() {
            return Err(denied("invalid workspace path length"));
        }
        let relative = Path::new(path);
        let mut resolved = self.spec.root.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(denied("path must contain only relative normal components"));
            };
            let name_text = name.to_string_lossy();
            if [
                ".git", ".fishnet", ".codex", ".agents", ".aws", ".ssh", "target",
            ]
            .contains(&name_text.as_ref())
                || name_text.starts_with(".env")
                || name_text.starts_with(".fishnet-edit-")
                || name_text.ends_with(".pem")
                || name_text.ends_with(".key")
            {
                return Err(denied(
                    "path is reserved or contains credentials or generated state",
                ));
            }
            resolved.push(name);
            match fs::symlink_metadata(&resolved) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(denied("symlink access is not granted"));
                }
                Ok(_) => (),
                Err(error) if allow_missing && error.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => return Err(denied("workspace path is unavailable")),
            }
        }
        Ok(resolved)
    }

    fn read_text(&self, path: &str) -> Result<String, OperationError> {
        let file =
            fs::File::open(self.path(path, false)?).map_err(|_| denied("file cannot be opened"))?;
        if !file
            .metadata()
            .map_err(|_| denied("file metadata unavailable"))?
            .is_file()
        {
            return Err(denied("path must refer to a regular file"));
        }
        let mut text = String::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|_| denied("file must contain valid UTF-8"))?;
        if text.len() as u64 > MAX_FILE_BYTES {
            return Err(denied("file exceeds 1 MiB limit"));
        }
        Ok(text)
    }

    fn file_hash(&self, path: &str) -> Result<Option<String>, OperationError> {
        if !self.path(path, true)?.exists() {
            return Ok(None);
        }
        Ok(Some(fingerprint(self.read_text(path)?.as_bytes())))
    }

    fn read_file(&self, args: &ReadArguments, limit: usize) -> Result<Value, OperationError> {
        let text = self.read_text(&args.path)?;
        if !text.is_char_boundary(args.offset) {
            return Err(conflict("offset is outside the file or splits UTF-8"));
        }
        let remaining = text
            .get(args.offset..)
            .ok_or_else(|| conflict("offset is outside file"))?;
        let mut page_bytes = args.max_bytes.min(limit.saturating_sub(512));
        loop {
            let prefix = BoundedText::new(remaining, page_bytes);
            let data = json!({"path":args.path,"sha256":fingerprint(text.as_bytes()),"total_bytes":text.len(),
                "offset":args.offset,"next_offset":args.offset+prefix.text.len(),"text":prefix.text,"truncated":prefix.truncated});
            // Bound the complete serialized result, including JSON escaping and its envelope.
            let encoded = serde_json::to_vec(&ToolResult::Output { data: data.clone() })
                .map_err(|_| denied("cannot encode file page"))?;
            if encoded.len() <= limit {
                return Ok(data);
            }
            if page_bytes == 0 {
                return Err(denied("file page metadata exceeds output limit"));
            }
            page_bytes /= 2;
        }
    }

    fn list_files(&self, args: &ListArguments, limit: usize) -> Result<Value, OperationError> {
        let directory = if args.path == "." {
            self.spec.root.clone()
        } else {
            self.path(&args.path, false)?
        };
        let mut names = Vec::new();
        for entry in fs::read_dir(directory)
            .map_err(|_| denied("directory cannot be read"))?
            .take(257)
        {
            let entry = entry.map_err(|_| denied("directory entry unavailable"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if args.path == "." {
                name.clone()
            } else {
                format!("{}/{name}", args.path)
            };
            if self.path(&relative, false).is_ok() {
                names.push(name);
            }
        }
        names.sort();
        let text = names.join("\n");
        Ok(json!({"entries":BoundedText::new(&text,limit/12),"entry_limit":256}))
    }

    fn replace_text(&self, args: ReplaceArguments) -> Result<Value, OperationError> {
        if !self.spec.allowed_writes.contains(&args.path) {
            return Err(denied("file has no write grant"));
        }
        let destination = self.path(&args.path, true)?;
        let existing = self.file_hash(&args.path)?;
        if existing != args.expected_sha256 {
            return Err(conflict(
                "file changed; read the current version before editing",
            ));
        }
        let content = if existing.is_some() {
            self.read_text(&args.path)?
        } else {
            String::new()
        };
        if existing.is_some() && Some(fingerprint(content.as_bytes())) != args.expected_sha256 {
            return Err(conflict("file changed during the edit read"));
        }
        if args.old.is_empty() && !content.is_empty() {
            return Err(conflict(
                "empty old text is only valid for an empty or new file",
            ));
        }
        if !args.old.is_empty() && content.matches(&args.old).count() != 1 {
            return Err(conflict("old text must match exactly once"));
        }
        let updated = if args.old.is_empty() {
            args.new
        } else {
            content.replacen(&args.old, &args.new, 1)
        };
        if updated.len() as u64 > MAX_FILE_BYTES {
            return Err(denied("edit exceeds 1 MiB file bound"));
        }
        atomic_edit(&destination, updated.as_bytes(), || {
            if self.file_hash(&args.path)? != existing {
                return Err(conflict("file changed while preparing the edit"));
            }
            Ok(())
        })?;
        Ok(
            json!({"path":args.path,"before_sha256":existing,"after_sha256":fingerprint(updated.as_bytes()),"bytes":updated.len()}),
        )
    }
}

fn arguments<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, OperationError> {
    serde_json::from_value(value.clone()).map_err(|_| OperationError {
        kind: ErrorKind::InvalidArguments,
        message: "tool arguments do not match the declared schema".into(),
    })
}

fn denied(message: &str) -> OperationError {
    OperationError {
        kind: ErrorKind::PermissionDenied,
        message: message.into(),
    }
}
fn conflict(message: &str) -> OperationError {
    OperationError {
        kind: ErrorKind::Conflict,
        message: message.into(),
    }
}
fn uncertain(message: &str) -> OperationError {
    OperationError {
        kind: ErrorKind::UnknownEffect,
        message: message.into(),
    }
}

/// Provider schemas declare only the capabilities enforced by the executor.
#[must_use]
pub fn tool_schemas(spec: &WorkspaceSpec) -> Vec<Value> {
    let string = json!({"type":"string"});
    let integer = json!({"type":"integer","minimum":0});
    let definitions = [
        (
            "read_file",
            "Read a UTF-8 file page. Use next_offset for the next page. Hash covers the complete file.",
            json!({"path":string,"offset":integer,"max_bytes":integer}),
        ),
        (
            "list_files",
            "List one directory under the workspace root. Use '.' for the root.",
            json!({"path":string}),
        ),
        (
            "replace_text",
            "Replace exactly one matching span after checking the complete file hash. For a new file use null hash and empty old text.",
            json!({"path":{"type":"string","enum":spec.allowed_writes},"expected_sha256":{"type":["string","null"]},"old":string,"new":string}),
        ),
        (
            "run_command",
            "Run one host-authorized argv in the workspace. A nonzero exit is evidence, not a transport failure.",
            json!({"command_id":{"type":"string","enum":spec.commands.iter().map(|command| &command.id).collect::<Vec<_>>()}}),
        ),
    ];
    definitions.into_iter().filter(|(name,_,_)| *name != "run_command" || !spec.commands.is_empty()).map(|(name,description,properties)| {
        let required = properties.as_object().map(|fields| fields.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
        json!({"type":"function","name":name,"description":description,"strict":true,
            "parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
    }).collect()
}

struct PendingEdit {
    path: PathBuf,
}

impl Drop for PendingEdit {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn atomic_edit(
    destination: &Path,
    bytes: &[u8],
    check_current: impl FnOnce() -> Result<(), OperationError>,
) -> Result<(), OperationError> {
    let parent = destination
        .parent()
        .ok_or_else(|| denied("edit has no parent directory"))?;
    let pending = PendingEdit {
        path: parent.join(format!(
            ".fishnet-edit-{}-{}",
            std::process::id(),
            NEXT_EDIT.fetch_add(1, Ordering::Relaxed)
        )),
    };
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&pending.path)
        .map_err(|_| denied("cannot create edit staging file"))?;
    if let Ok(metadata) = fs::metadata(destination) {
        file.set_permissions(metadata.permissions())
            .map_err(|_| denied("cannot preserve file permissions"))?;
    }
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| denied("cannot prepare durable edit"))?;
    check_current()?;
    fs::rename(&pending.path, destination)
        .map_err(|_| uncertain("edit rename outcome is unknown; inspect before retrying"))?;
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|_| uncertain("edit directory durability is unknown"))?;
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskFile {
    pub schema_version: u32,
    pub objective: String,
    pub workspace: WorkspaceSpec,
    #[serde(default = "coding_limits")]
    pub limits: crate::protocol::Limits,
}

/// Load a bounded host task contract. Model tools cannot change the in-memory grants.
///
/// # Errors
/// Rejects malformed contracts, unavailable programs, and edits to the contract file itself.
pub fn load_task(path: &Path, root: Option<&Path>) -> Result<TaskFile, OperationError> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| denied("task file unavailable"))?
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|_| denied("cannot read task file"))?;
    if bytes.len() > 65_536 {
        return Err(denied("task contract exceeds 65536 bytes"));
    }
    let mut task: TaskFile =
        serde_json::from_slice(&bytes).map_err(|_| denied("invalid task contract"))?;
    if task.schema_version != 1 {
        return Err(denied("unsupported task contract schema"));
    }
    task.limits.validate().map_err(denied)?;
    if task.limits.max_output_bytes < 2_048 {
        return Err(denied(
            "coding tasks require an output budget of at least 2048 bytes",
        ));
    }
    if let Some(root) = root {
        task.workspace.root = root.into();
    }
    task.workspace.root = task
        .workspace
        .root
        .canonicalize()
        .map_err(|_| denied("workspace root unavailable"))?;
    if let Ok(relative) = path
        .canonicalize()
        .map_err(|_| denied("task file unavailable"))?
        .strip_prefix(&task.workspace.root)
    {
        let relative = relative.to_string_lossy().into_owned();
        if task.workspace.allowed_writes.contains(&relative) {
            return Err(denied("task contract cannot grant edits to itself"));
        }
        if !task.workspace.protected_paths.contains(&relative) {
            task.workspace.protected_paths.push(relative);
        }
    }
    for command in &mut task.workspace.commands {
        if !command.program.is_absolute() {
            let program = &command.program;
            command.program = if program.components().count() == 1 {
                std::env::var_os("PATH")
                    .and_then(|value| {
                        std::env::split_paths(&value)
                            .map(|directory| directory.join(program))
                            .find(|path| path.is_absolute() && path.is_file())
                    })
                    .ok_or_else(|| denied("authorized program is not available in PATH"))?
            } else {
                if !program
                    .components()
                    .all(|component| matches!(component, Component::Normal(_)))
                {
                    return Err(denied(
                        "relative program must remain under the workspace root",
                    ));
                }
                task.workspace.root.join(program)
            };
        }
        process::validate(command)?;
    }
    Ok(task)
}

impl TaskFile {
    #[must_use]
    pub fn into_contract(self) -> crate::protocol::TaskContract {
        crate::protocol::TaskContract {
            objective: self.objective,
            expected_key: "workspace".into(),
            expected_value: None,
            policy_version: POLICY.into(),
            fixture_revision: "workspace-v1".into(),
            workspace: Some(self.workspace),
        }
    }
}

const fn coding_limits() -> crate::protocol::Limits {
    crate::protocol::Limits {
        deadline_ms: 300_000,
        max_provider_calls: 32,
        max_output_bytes: 8_192,
    }
}
