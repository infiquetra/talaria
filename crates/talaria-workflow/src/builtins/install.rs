use super::{BUILTINS, Builtin, get};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub enum InstallMode {
    MissingOnly,
    Overwrite,
}
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallAction {
    Created,
    Unchanged,
    KeptModified,
    Overwritten,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallReport {
    pub name: String,
    pub path: PathBuf,
    pub action: InstallAction,
    pub backup: Option<PathBuf>,
}
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstalledState {
    NotInstalled,
    Installed,
    Modified,
}
fn at(path: &Path, e: io::Error) -> io::Error {
    io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}
fn atomic_write(dir: &Path, name: &str, destination: &Path, bytes: &[u8]) -> io::Result<()> {
    for n in 0..100 {
        let temp = dir.join(format!(".{name}.js.tmp-{}-{n}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temp) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(at(&temp, e)),
        };
        let result = file
            .write_all(bytes)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all())
            .and_then(|_| fs::rename(&temp, destination));
        if let Err(e) = result {
            let _ = fs::remove_file(&temp);
            return Err(at(destination, e));
        }
        return Ok(());
    }
    Err(at(
        destination,
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "temporary file names exhausted",
        ),
    ))
}
pub fn install(dir: &Path, mode: InstallMode) -> io::Result<Vec<InstallReport>> {
    match fs::metadata(dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(io::Error::other(format!("{}: not a folder", dir.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(dir).map_err(|e| at(dir, e))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
                    .map_err(|e| at(dir, e))?;
            }
        }
        Err(e) => return Err(at(dir, e)),
    }
    let mut reports = Vec::new();
    for builtin in BUILTINS {
        let path = dir.join(format!("{}.js", builtin.name));
        let mut backup = None;
        let action = match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                atomic_write(dir, builtin.name, &path, builtin.source.as_bytes())?;
                InstallAction::Created
            }
            Err(e) => return Err(at(&path, e)),
            Ok(m) if !m.file_type().is_file() => InstallAction::KeptModified,
            Ok(_) => {
                let old = fs::read(&path).map_err(|e| at(&path, e))?;
                if old == builtin.source.as_bytes() {
                    InstallAction::Unchanged
                } else if matches!(mode, InstallMode::MissingOnly) {
                    InstallAction::KeptModified
                } else {
                    let bak = path.with_extension("js.bak");
                    atomic_write(dir, builtin.name, &bak, &old)?;
                    atomic_write(dir, builtin.name, &path, builtin.source.as_bytes())?;
                    backup = Some(bak);
                    InstallAction::Overwritten
                }
            }
        };
        reports.push(InstallReport {
            name: builtin.name.into(),
            path,
            action,
            backup,
        });
    }
    Ok(reports)
}
pub fn installed_state(dir: &Path, builtin: &Builtin) -> InstalledState {
    let path = dir.join(format!("{}.js", builtin.name));
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => InstalledState::NotInstalled,
        Ok(m) if m.is_file() && fs::read(&path).is_ok_and(|s| s == builtin.source.as_bytes()) => {
            InstalledState::Installed
        }
        _ => InstalledState::Modified,
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ScriptArg {
    Path(PathBuf),
    Named {
        name: String,
        path: PathBuf,
        builtin: bool,
    },
    MissingBuiltin {
        name: String,
        path: PathBuf,
    },
}
pub fn resolve_script_arg(arg: &str, workflows_dir: &Path) -> Result<ScriptArg, String> {
    if arg.contains(['/', '\\']) || arg.ends_with(".js") || Path::new(arg).is_file() {
        return Ok(ScriptArg::Path(arg.into()));
    }
    if arg.is_empty()
        || arg.len() > 64
        || arg.starts_with('.')
        || !arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "'{arg}' is neither a script path nor a workflow name"
        ));
    }
    let path = workflows_dir.join(format!("{arg}.js"));
    if path.is_file() {
        return Ok(ScriptArg::Named {
            name: arg.into(),
            path,
            builtin: get(arg).is_some(),
        });
    }
    if get(arg).is_some() {
        return Ok(ScriptArg::MissingBuiltin {
            name: arg.into(),
            path,
        });
    }
    Err(format!(
        "no workflow named '{arg}' in {}; built-ins: fix-failing-tests, research, review-branch, ultracode",
        workflows_dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_failure_preserves_the_modified_target_and_removes_own_temporary_file() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("fix-failing-tests.js");
        fs::write(&target, "operator edits").unwrap();
        let backup = target.with_extension("js.bak");
        fs::create_dir(&backup).unwrap();
        let error = install(temp.path(), InstallMode::Overwrite).unwrap_err();
        assert!(error.to_string().contains(&backup.display().to_string()));
        assert_eq!(fs::read_to_string(target).unwrap(), "operator edits");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    }

    #[test]
    fn non_regular_target_is_kept_with_force() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("research.js")).unwrap();
        let reports = install(temp.path(), InstallMode::Overwrite).unwrap();
        assert_eq!(
            reports
                .iter()
                .find(|r| r.name == "research")
                .unwrap()
                .action,
            InstallAction::KeptModified
        );
        assert_eq!(
            installed_state(temp.path(), get("research").unwrap()),
            InstalledState::Modified
        );
    }
}
