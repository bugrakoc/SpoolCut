use std::{
    env,
    io::{self, Write},
    path::Path,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use spoolcut::{Project, export, inspect, plan};

const USAGE: &str = "SpoolCut stream-copy prototype
  spoolcut inspect SOURCE
  spoolcut plan PROJECT.json
  spoolcut export PROJECT.json OUTPUT.mkv";

fn main() -> ExitCode {
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancel);
    let result = ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
        .context("installing Ctrl+C handler")
        .and_then(|()| run(&cancel));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("spoolcut: {error:#}");
            ExitCode::from(if cancel.load(Ordering::Relaxed) {
                130
            } else {
                1
            })
        }
    }
}

fn run(cancel: &AtomicBool) -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match (args.first().and_then(|arg| arg.to_str()), args.as_slice()) {
        (Some("inspect"), [_, source]) => print_json(&inspect(Path::new(source), cancel)?),
        (Some("plan"), [_, path]) => {
            let path = Path::new(path);
            let project = Project::load(path)?;
            print_json(&plan(
                &project,
                path.parent().context("invalid project path")?,
                cancel,
            )?)
        }
        (Some("export"), [_, path, output]) => {
            let path = Path::new(path);
            let project = Project::load(path)?;
            export(
                &project,
                path.parent().context("invalid project path")?,
                Path::new(output),
                cancel,
            )
        }
        (Some("--help" | "-h"), [_]) | (None, []) => {
            println!("{USAGE}");
            Ok(())
        }
        _ => bail!("invalid arguments\n{USAGE}"),
    }
}

fn print_json(value: &impl Serialize) -> Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, value)?;
    writeln!(output)?;
    Ok(())
}
