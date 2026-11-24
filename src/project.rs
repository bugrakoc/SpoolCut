use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub sources: BTreeMap<String, PathBuf>,
    pub clips: Vec<Clip>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub source: String,
    pub in_pts: i64,
    pub out_pts: i64,
}

impl Project {
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let project: Self = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("reading project {}", path.display()))?;
        Ok(project)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let directory = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut staged = tempfile::NamedTempFile::new_in(directory)
            .with_context(|| format!("creating a temporary project in {}", directory.display()))?;
        {
            let mut writer = BufWriter::new(staged.as_file_mut());
            serde_json::to_writer_pretty(&mut writer, self)?;
            writeln!(writer)?;
            writer.flush()?;
        }
        staged
            .persist(path)
            .with_context(|| format!("saving {}", path.display()))?;
        Ok(())
    }
}
