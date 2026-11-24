use std::{
    fs::File,
    io::{Read, Seek},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};

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
        .with_context(|| format!("starting {name}. Install it and ensure it is on PATH"))?;

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
