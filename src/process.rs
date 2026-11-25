use std::{
    env,
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};

pub(crate) fn tool(name: &str) -> Result<Command> {
    let setting = format!("SPOOLCUT_{}", name.to_ascii_uppercase());
    let executable = format!("{name}{}", env::consts::EXE_SUFFIX);
    let configured = env::var_os(&setting).map(PathBuf::from);
    let bundle = env::current_exe().ok();
    let path = env::var_os("PATH");
    let path_dirs = path.iter().flat_map(env::split_paths);
    let program = resolve_tool(
        &executable,
        &setting,
        configured,
        bundle.as_deref(),
        path_dirs,
    )?;
    let command = Command::new(program);
    #[cfg(windows)]
    let mut command = command;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(command)
}

fn resolve_tool(
    executable: &str,
    setting: &str,
    configured: Option<PathBuf>,
    application: Option<&Path>,
    directories: impl Iterator<Item = PathBuf>,
) -> Result<PathBuf> {
    if let Some(path) = configured {
        ensure!(
            is_executable(&path),
            "{setting} does not point to an executable file: {}",
            path.display()
        );
        return Ok(std::path::absolute(path)?);
    }

    let candidates = application
        .and_then(Path::parent)
        .into_iter()
        .flat_map(|dir| [dir.join(executable), dir.join("bin").join(executable)])
        .chain(
            directories
                .filter(|dir| !dir.as_os_str().is_empty())
                .map(|dir| dir.join(executable)),
        );
    for path in candidates {
        if is_executable(&path) {
            return Ok(std::path::absolute(path)?);
        }
    }
    bail!(
        "could not find {executable}; set {setting}, install it alongside SpoolCut, or add it to PATH"
    )
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    metadata.is_file()
}

pub(crate) fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    Ok(())
}

pub(crate) fn run(command: &mut Command, output: Option<&File>, cancel: &AtomicBool) -> Result<()> {
    check(cancel)?;
    let name = command.get_program().to_string_lossy().into_owned();
    let mut stderr = tempfile::tempfile()?;
    let stdout = match output {
        Some(file) => Stdio::from(file.try_clone()?),
        None => Stdio::null(),
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr.try_clone()?)
        .spawn()
        .with_context(|| format!("starting {name}"))?;

    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("cancelled");
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error).with_context(|| format!("waiting for {name}"));
            }
        }
    };
    check(cancel)?;
    stderr.rewind()?;
    let mut errors = Vec::new();
    stderr.take(16_384).read_to_end(&mut errors)?;
    let errors = String::from_utf8_lossy(&errors);
    if !status.success() || !errors.trim().is_empty() {
        bail!("{name} ({status}): {}", errors.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn executable_discovery_prefers_override_then_bundle_then_path() {
        let root = tempfile::tempdir().unwrap();
        let executable = format!("ffmpeg{}", env::consts::EXE_SUFFIX);
        let bundled = root.path().join(&executable);
        let path_dir = root.path().join("on PATH");
        let path_tool = path_dir.join(&executable);
        fs::create_dir(&path_dir).unwrap();
        // Use the running test binary to get a real executable with native permissions.
        fs::copy(env::current_exe().unwrap(), &bundled).unwrap();
        fs::copy(&bundled, &path_tool).unwrap();
        let application = root.path().join("SpoolCut");
        let resolve = |configured| {
            resolve_tool(
                &executable,
                "SPOOLCUT_FFMPEG",
                configured,
                Some(&application),
                std::iter::once(path_dir.clone()),
            )
        };
        assert_eq!(resolve(Some(path_tool.clone())).unwrap(), path_tool);
        assert_eq!(resolve(None).unwrap(), bundled);
        assert!(resolve(Some(root.path().join("missing"))).is_err());
        fs::remove_file(bundled).unwrap();
        assert_eq!(resolve(None).unwrap(), path_tool);
    }
}
