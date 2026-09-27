//! Confirmed deletion is serialized with apply; fallback commits before Intent removal (RFC 0003).

use super::*;
use crate::{
    apply::apply_resolved_profile_unlocked,
    domain::{ActivationId, ProfileIntent},
    runtime::ReloadAdapter,
};
use serde_json::Value;

mod files;
use files::{Directory, Entry};

#[cfg(test)]
mod tests;

/// Read-only confirmation snapshot, not a persisted deletion plan.
pub(crate) struct DeletionRequest {
    pub id: IntentId,
    pub active: bool,
    image: Option<Entry>,
    original: Entry,
    directory: Directory,
    sequence: Option<u64>,
}

impl DeletionRequest {
    pub(crate) fn image_path(&self) -> Option<PathBuf> {
        self.image
            .as_ref()
            .map(|image| self.directory.path(&image.name))
    }
}

pub(crate) struct DeletionOutcome {
    pub fallback: Option<(ActivationId, ReloadOutcome)>,
    pub cleanup: String,
}

impl ProfileWorkflows {
    /// Captures consent scope without resolving Welcome, writing files, or reloading.
    pub(crate) fn prepare_deletion(&self, name: &str) -> Result<DeletionRequest, WorkflowError> {
        let id = IntentId::from_str(name)?;
        let root = self.paths.managed_root();
        let _lock = exclusive_state_lock(&root.join("state.lock"))
            .map_err(|e| WorkflowError::History(e.to_string()))?;
        let dir = Directory::open(&root)?;
        check_registry(&dir, &self.config)?;
        let original = read_profile(&dir, &id)?;
        if id.as_str() == "welcome" {
            return Err(WorkflowError::Invalid(
                "installed Welcome cannot be deleted",
            ));
        }
        parse_named_profile_toml(id.as_str(), &self.config, original.text()?)?;
        let history =
            inspect_history_unlocked(&root).map_err(|e| WorkflowError::History(e.to_string()))?;
        if history.latest().is_some_and(|a| a.profile_id().is_none()) {
            return Err(WorkflowError::Replay(id));
        }
        Ok(DeletionRequest {
            active: history.latest().and_then(|a| a.profile_id()) == Some(&id),
            sequence: history.latest().map(|a| a.sequence()),
            image: exclusive_image(&self.config, &dir, &id, original.text()?, Some(&original)),
            id,
            original,
            directory: dir,
        })
    }

    /// Executes only after consent. The internal resolver stages Welcome outside the lock;
    /// captured Intent and History are checked again before any write. Reload follows unlock.
    pub(crate) fn confirm_deletion<E: std::fmt::Display>(
        &self,
        request: &DeletionRequest,
        activated_at: &str,
        resolve_welcome: impl FnOnce(&IntentId, &ProfileIntent) -> Result<(Value, Option<Vec<u8>>), E>,
        reload: impl ReloadAdapter,
    ) -> Result<DeletionOutcome, WorkflowError> {
        let fallback_error = |reason: String| WorkflowError::Fallback {
            id: request.id.clone(),
            reason,
        };
        let dir = &request.directory;
        let original = request.original.text()?;
        dir.validate()?;
        let prepared = if request.active {
            let welcome_id = IntentId::from_str("welcome")?;
            let welcome =
                read_profile(dir, &welcome_id).map_err(|e| fallback_error(e.to_string()))?;
            let document = welcome.text().map_err(|e| fallback_error(e.to_string()))?;
            let (id, intent) = parse_named_profile_toml("welcome", &self.config, document)
                .map_err(|e| fallback_error(e.to_string()))?;
            let (plan, bytes) =
                resolve_welcome(&id, &intent).map_err(|e| fallback_error(e.to_string()))?;
            if plan.pointer("/profile/id").and_then(Value::as_str) != Some("welcome") {
                return Err(fallback_error("resolver did not return Welcome".into()));
            }
            Some((welcome, plan, bytes))
        } else {
            None
        };

        let root = self.paths.managed_root();
        let lock = exclusive_state_lock(&root.join("state.lock"))
            .map_err(|e| WorkflowError::History(e.to_string()))?;
        dir.validate()?;
        check_registry(dir, &self.config)?;
        if !dir.unchanged(&request.original, MAX_INTENT)? {
            return Err(WorkflowError::DeletionChanged(request.id.clone()));
        }
        let history =
            inspect_history_unlocked(&root).map_err(|e| WorkflowError::History(e.to_string()))?;
        if history.latest().map(|a| a.sequence()) != request.sequence {
            return Err(WorkflowError::DeletionChanged(request.id.clone()));
        }
        let fallback = if let Some((welcome, plan, bytes)) = prepared {
            if !dir.unchanged(&welcome, MAX_INTENT)? {
                return Err(WorkflowError::DeletionChanged(request.id.clone()));
            }
            Some(
                apply_resolved_profile_unlocked(
                    &root,
                    &self.paths.ghostty_root_config(),
                    activated_at,
                    &plan,
                    bytes.as_deref(),
                )
                .map_err(|e| fallback_error(e.to_string()))?,
            )
        } else {
            None
        };

        // Never remove an image before Profile removal is durable. A crash here may leave
        // Welcome active and this Profile present, but cannot leave deleted Intent active.
        let removal = dir.remove(&request.original, MAX_INTENT).and_then(|()| {
            // Once the confirmed target is gone, its name is no longer exempt from the
            // ownership proof: a publisher may have installed a replacement (RFC 0003).
            let image = request.image.as_ref().and_then(|confirmed| {
                exclusive_image(&self.config, dir, &request.id, original, None)
                    .filter(|image| confirmed.same(image).unwrap_or(false))
            });
            dir.require_absent(&request.original.name)?;
            Ok(image)
        });
        let mut cleanup = "Images retained (no confirmed, proven-exclusive owned copy).".to_owned();
        if let Ok(Some(image)) = &removal {
            let path = dir.path(&image.name);
            cleanup = match dir.remove(image, MAX_IMAGE) {
                Ok(()) => format!("Removed owned image {}.", path.display()),
                Err(error) => format!(
                    "Owned image cleanup incomplete at {}: {error}; retained or removal durability uncertain; inspect before retrying.",
                    path.display()
                ),
            };
        }
        drop(lock);
        let fallback = fallback.map(|id| (id, reload.reload()));
        removal.map_err(|source| WorkflowError::DeletionIncomplete {
            id: request.id.clone(),
            fallback: match fallback {
                Some((id, reload)) => {
                    format!("Welcome Activation {id} committed; reload {reload:?}")
                }
                None => "no fallback Activation or reload requested".into(),
            },
            source: Box::new(source),
        })?;
        Ok(DeletionOutcome { fallback, cleanup })
    }
}

fn read_profile(dir: &Directory, id: &IntentId) -> Result<Entry, WorkflowError> {
    match dir.read(&format!("{id}.toml"), MAX_INTENT) {
        Err(WorkflowError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            Err(WorkflowError::Missing(id.clone()))
        }
        other => other,
    }
}

fn check_registry(dir: &Directory, config: &ConfigIntent) -> Result<(), WorkflowError> {
    if parse_config_toml(dir.config()?.text()?)? != *config {
        return Err(WorkflowError::Invalid(
            "Source registry changed; start delete again",
        ));
    }
    Ok(())
}

fn exclusive_image(
    config: &ConfigIntent,
    dir: &Directory,
    id: &IntentId,
    intent: &str,
    confirmed_target: Option<&Entry>,
) -> Option<Entry> {
    let (_, profile) = parse_named_profile_toml(id.as_str(), config, intent).ok()?;
    let Some(WallpaperIntent::Source {
        source,
        selection: WallpaperSelection::Path(candidate),
        owned_image: Some(owned),
        ..
    }) = profile.wallpaper
    else {
        return None;
    };
    if candidate.as_str() != format!("{id}.png") && candidate.as_str() != format!("{id}.jpg") {
        return None;
    }
    if config.sources.iter().any(|(other, intent)| {
        other != &source && matches!(intent, SourceIntent::LocalDirectory { .. })
    }) {
        return None;
    }
    for name in dir.names().ok()? {
        if Path::new(&name).extension() != Some(std::ffi::OsStr::new("toml")) {
            continue;
        }
        let filename = name.to_str()?;
        let name = filename.strip_suffix(".toml")?;
        let entry = dir.read(filename, MAX_INTENT).ok()?;
        if confirmed_target
            .is_some_and(|target| target.name == filename && target.same(&entry).unwrap_or(false))
        {
            continue;
        }
        let (_, other) = parse_named_profile_toml(name, config, entry.text().ok()?).ok()?;
        if let Some(WallpaperIntent::Source {
            source: other_source,
            selection,
            ..
        }) = other.wallpaper
            && other_source == source
            && match selection {
                WallpaperSelection::Random => true,
                WallpaperSelection::Path(path) => path == candidate,
            }
        {
            return None;
        }
    }
    let image = dir.read(candidate.as_str(), MAX_IMAGE).ok()?;
    if !image.single_link().ok()?
        || Sha256Digest::from_bytes(Sha256::digest(&image.bytes).into()) != owned.sha256
    {
        return None;
    }
    Some(image)
}
