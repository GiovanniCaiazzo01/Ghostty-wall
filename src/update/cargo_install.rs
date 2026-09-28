use super::{REPOSITORY, UpdateError};
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub(super) struct Record {
    toml: toml::Value,
    json: serde_json::Value,
    key: String,
}

impl Record {
    pub(super) fn parse(toml: &[u8], json: &[u8], version: &str) -> Result<Self, UpdateError> {
        let invalid = || {
            UpdateError::Release("Cargo ownership metadata is missing, inconsistent, or ambiguous; repair this installation with Cargo".into())
        };
        let toml: toml::Value = std::str::from_utf8(toml)
            .ok()
            .and_then(|s| toml::from_str(s).ok())
            .ok_or_else(invalid)?;
        let json: serde_json::Value = serde_json::from_slice(json).map_err(|_| invalid())?;
        let entries = toml
            .get("v1")
            .and_then(|v| v.as_table())
            .ok_or_else(invalid)?;
        let installs = json
            .get("installs")
            .and_then(|v| v.as_object())
            .ok_or_else(invalid)?;
        let owns = |v: &toml::Value| {
            v.as_array()
                .is_some_and(|bins| bins.iter().any(|b| b.as_str() == Some("ghostty-wall")))
        };
        let keys: Vec<_> = entries
            .iter()
            .filter(|(_, v)| owns(v))
            .map(|(k, _)| k.clone())
            .collect();
        if keys.len() != 1 {
            return Err(invalid());
        }
        let key = keys[0].clone();
        if !key.starts_with(&format!("ghostty-wall {version} ("))
            || !key.ends_with(')')
            || entries[&key].as_array().map(|a| a.len()) != Some(1)
            || installs.get(&key).and_then(|v| v.get("bins"))
                != Some(&serde_json::json!(["ghostty-wall"]))
            || installs
                .values()
                .filter(|v| {
                    v.get("bins")
                        .and_then(|v| v.as_array())
                        .is_some_and(|bins| bins.iter().any(|b| b == "ghostty-wall"))
                })
                .count()
                != 1
        {
            return Err(invalid());
        }
        Ok(Self { toml, json, key })
    }

    pub(super) fn updated(
        mut self,
        built: Self,
        tag: &str,
    ) -> Result<(Vec<u8>, Vec<u8>), UpdateError> {
        let prefix = format!(
            "ghostty-wall {} (git+https://github.com/{REPOSITORY}?tag={tag}#",
            &tag[1..]
        );
        let revision = built
            .key
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(')'));
        if !revision.is_some_and(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())) {
            return Err(UpdateError::Release(
                "source build did not record the requested official release tag and commit".into(),
            ));
        }
        let entries = self.toml.get_mut("v1").unwrap().as_table_mut().unwrap();
        entries.remove(&self.key);
        entries.insert(built.key.clone(), built.toml["v1"][&built.key].clone());
        let installs = self.json["installs"].as_object_mut().unwrap();
        installs.remove(&self.key);
        installs.insert(
            built.key.clone(),
            built.json["installs"][&built.key].clone(),
        );
        Ok((
            toml::to_string(&self.toml)
                .map_err(|e| UpdateError::Release(e.to_string()))?
                .into_bytes(),
            serde_json::to_vec(&self.json).map_err(|e| UpdateError::Release(e.to_string()))?,
        ))
    }
}

pub(super) fn build(tag: &str, root: &Path) -> Result<(), UpdateError> {
    let log = tempfile::tempfile()?;
    let status = Command::new("cargo")
        .args([
            "install",
            "--git",
            &format!("https://github.com/{REPOSITORY}"),
            "--tag",
            tag,
            "--locked",
            "--root",
        ])
        .arg(root)
        .args(["--bin", "ghostty-wall", "ghostty-wall"])
        .env("CARGO_TARGET_DIR", root.join("target"))
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()
        .map_err(|e| {
            UpdateError::Build(format!(
                "could not run cargo: {e}. Install Cargo/Rust and a native linker, then retry"
            ))
        })?;
    if !status.success() {
        return Err(UpdateError::Build("cargo install failed; check Rust (minimum 1.88), native linker, GitHub access and disk space. Previous installation was not changed".into()));
    }
    Ok(())
}
