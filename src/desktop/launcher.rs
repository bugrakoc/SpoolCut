use std::{
    env, fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use gtk::glib::{self, KeyFile, KeyFileFlags};

use crate::APPLICATION_ID;

pub const APP_ICON: &[u8] = include_bytes!("../../assets/io.github.bugrakoc.SpoolCut.png");
const DESKTOP_ENTRY: &str = include_str!("../../assets/io.github.bugrakoc.SpoolCut.desktop");
const GROUP: &str = "Desktop Entry";
const MANAGED: &str = "X-SpoolCut-AppImage";

pub fn register_appimage() -> Result<()> {
    let (Some(image), Some(bundle)) = (env::var_os("APPIMAGE"), env::var_os("APPDIR")) else {
        return Ok(());
    };
    let (image, bundle) = (PathBuf::from(image), PathBuf::from(bundle));
    if !image.is_absolute() || !image.is_file() || !env::current_exe()?.starts_with(&bundle) {
        return Ok(());
    }
    let host_data_dirs = glib::system_data_dirs()
        .into_iter()
        .filter(|directory| !directory.starts_with(&bundle));
    install(&image, &glib::user_data_dir(), host_data_dirs)
}

fn install(
    image: &Path,
    user_data: &Path,
    host_data_dirs: impl Iterator<Item = PathBuf>,
) -> Result<()> {
    let desktop_file = Path::new("applications").join(format!("{APPLICATION_ID}.desktop"));
    let launcher = user_data.join(&desktop_file);
    let managed = match fs::read_to_string(&launcher) {
        Ok(contents) => {
            let entry = KeyFile::new();
            if entry.load_from_data(&contents, KeyFileFlags::NONE).is_err()
                || entry.boolean(GROUP, MANAGED) != Ok(true)
            {
                return Ok(());
            }
            true
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if host_data_dirs
                .into_iter()
                .any(|directory| directory.join(&desktop_file).is_file())
            {
                return Ok(());
            }
            false
        }
        Err(error) => return Err(error).with_context(|| format!("reading {}", launcher.display())),
    };

    let image = image
        .to_str()
        .context("AppImage path must be UTF-8 for the desktop launcher")?;
    let icon = user_data.join("spoolcut").join("icon.png");
    let icon_path = icon
        .to_str()
        .context("Icon path must be UTF-8 for the desktop launcher")?;
    let entry = KeyFile::new();
    entry.load_from_data(DESKTOP_ENTRY, KeyFileFlags::NONE)?;
    entry.set_string(GROUP, "Exec", &launch_command(image));
    entry.set_string(GROUP, "TryExec", image);
    entry.set_string(GROUP, "Icon", icon_path);
    entry.set_boolean(GROUP, MANAGED, true);

    write_if_changed(&icon, APP_ICON, true)?;
    write_if_changed(&launcher, entry.to_data().as_bytes(), managed)
}

fn launch_command(image: &str) -> String {
    let quoted = image
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    if image.contains('=') {
        // Desktop entries forbid '=' in the executable token. Pass the path as an argument.
        format!(r#"/bin/sh -c "exec \"\$@\"" SpoolCut "{quoted}" %F"#)
    } else {
        format!(r#""{quoted}" %F"#)
    }
}

fn write_if_changed(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let directory = path
        .parent()
        .context("desktop integration path has no parent")?;
    fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(bytes)?;
    if replace {
        temporary.persist(path)?;
    } else if let Err(error) = temporary.persist_noclobber(path)
        && error.error.kind() != ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::PermissionsExt, thread, time::Duration};

    use gtk::{gio, prelude::AppInfoExt};

    use super::*;

    #[test]
    fn launcher_runs_moved_appimage_with_quoted_path() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let launcher = data
            .join("applications")
            .join(format!("{APPLICATION_ID}.desktop"));
        for name in [
            "SpoolCut test.AppImage",
            r#"SpoolCut = "α" %F $tag `q` \.AppImage"#,
        ] {
            let image = directory.path().join(name);
            fs::write(&image, "#!/bin/sh\nprintf '%s' \"$1\" > \"$0.args\"\n").unwrap();
            fs::set_permissions(&image, fs::Permissions::from_mode(0o755)).unwrap();
            install(&image, &data, std::iter::empty()).unwrap();

            let desktop = gio::DesktopAppInfo::from_filename(&launcher).unwrap();
            let project = directory.path().join("example project.json");
            desktop
                .launch(
                    &[gio::File::for_path(&project)],
                    gio::AppLaunchContext::NONE,
                )
                .unwrap();
            let arguments = image.with_extension("AppImage.args");
            let mut actual = String::new();
            for _ in 0..100 {
                actual = fs::read_to_string(&arguments).unwrap_or_default();
                if !actual.is_empty() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(actual, project.to_str().unwrap());
        }
        assert_eq!(fs::read(data.join("spoolcut/icon.png")).unwrap(), APP_ICON);
    }

    #[test]
    fn keeps_existing_launchers() {
        let directory = tempfile::tempdir().unwrap();
        let host_data = directory.path().join("host");
        let user_data = directory.path().join("user");
        let launcher = Path::new("applications").join(format!("{APPLICATION_ID}.desktop"));
        let image = directory.path().join("SpoolCut.AppImage");
        fs::create_dir_all(host_data.join("applications")).unwrap();
        fs::write(host_data.join(&launcher), DESKTOP_ENTRY).unwrap();
        install(&image, &user_data, [host_data].into_iter()).unwrap();
        assert!(!user_data.exists());

        fs::create_dir_all(user_data.join("applications")).unwrap();
        fs::write(user_data.join(&launcher), DESKTOP_ENTRY).unwrap();
        install(&image, &user_data, std::iter::empty()).unwrap();
        assert_eq!(
            fs::read_to_string(user_data.join(launcher)).unwrap(),
            DESKTOP_ENTRY
        );
        assert!(!user_data.join("spoolcut").exists());
    }
}
