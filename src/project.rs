use std::{collections::BTreeMap, fs::File, io::BufReader, path::Path, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub sources: BTreeMap<String, PathBuf>,
    pub clips: Vec<Clip>,
}

#[derive(Debug, Serialize, Deserialize)]
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
}
