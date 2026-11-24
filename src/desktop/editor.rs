use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use anyhow::{Context, Result, ensure};
use spoolcut::{Clip, IndexedSource, MediaInfo, Project};

pub struct IndexedMedia {
    pub index: IndexedSource,
    pub info: MediaInfo,
}

impl IndexedMedia {
    pub fn load(path: &Path, cancel: &AtomicBool) -> Result<Self> {
        let index = IndexedSource::open(path, cancel)?;
        let info = index.info()?;
        Ok(Self { index, info })
    }

    pub fn initial_clip(&self, source: &str) -> Result<Clip> {
        let in_pts = self.info.safe_in_pts[0];
        for out_pts in std::iter::once(self.info.video_end_pts)
            .chain(self.info.safe_in_pts.iter().skip(1).rev().copied())
        {
            let clip = Clip {
                source: source.into(),
                in_pts,
                out_pts,
            };
            if self.index.plan_clip(&clip).is_ok() {
                return Ok(clip);
            }
        }
        anyhow::bail!("No complete stream-copy range was found from the first safe cut")
    }
}

#[derive(Clone)]
pub struct Segment {
    pub path: PathBuf,
    pub start_us: i64,
    pub end_us: i64,
    pub source_start: f64,
    pub source_end: f64,
}

#[derive(Default)]
pub struct Editor {
    pub project: Project,
    pub path: Option<PathBuf>,
    pub media: HashMap<PathBuf, Result<IndexedMedia, String>>,
    saved: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,
}

impl Editor {
    pub fn open(&mut self, mut project: Project, path: PathBuf) {
        let root = path.parent().unwrap_or(Path::new("."));
        for source in project.sources.values_mut() {
            *source = root.join(&*source);
        }
        self.saved = project.clone();
        self.project = project;
        self.path = Some(path);
        self.undo.clear();
        self.redo.clear();
    }

    pub fn save(&mut self, path: PathBuf) -> Result<()> {
        let root = path.parent().context("Project needs a parent directory")?;
        let mut portable = self.project.clone();
        for source in portable.sources.values_mut() {
            if let Ok(relative) = source.strip_prefix(root) {
                *source = relative.to_owned();
            }
        }
        portable.save(&path)?;
        self.saved = self.project.clone();
        self.path = Some(path);
        Ok(())
    }

    pub fn dirty(&self) -> bool {
        self.project != self.saved
    }

    pub fn change(&mut self, edit: impl FnOnce(&mut Project)) {
        let previous = self.project.clone();
        edit(&mut self.project);
        if previous != self.project {
            self.undo.push(previous);
            self.redo.clear();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.redo
                .push(std::mem::replace(&mut self.project, previous));
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, next));
        }
    }

    pub fn import(&mut self, paths: &[PathBuf]) {
        self.change(|project| {
            for path in paths {
                if project.sources.values().any(|other| other == path) {
                    continue;
                }
                let id = (1..)
                    .map(|n| format!("source-{n}"))
                    .find(|id| !project.sources.contains_key(id))
                    .unwrap();
                project.sources.insert(id, path.clone());
            }
        });
    }

    pub fn source(&self, id: &str) -> Result<(&Path, &IndexedMedia)> {
        let path = self
            .project
            .sources
            .get(id)
            .with_context(|| format!("Unknown source {id}"))?;
        match self.media.get(path) {
            Some(Ok(media)) => Ok((path, media)),
            Some(Err(error)) => anyhow::bail!("{}: {error}", source_name(path)),
            None => anyhow::bail!("{} is being inspected", source_name(path)),
        }
    }

    pub fn segments(&self) -> Result<Vec<Segment>> {
        let mut cursor = 0i64;
        self.project
            .clips
            .iter()
            .enumerate()
            .map(|(index, clip)| {
                let (path, media) = self.source(&clip.source)?;
                let plan = media
                    .index
                    .plan_clip(clip)
                    .with_context(|| format!("Clip {}", index + 1))?;
                let start_us = cursor;
                cursor = cursor
                    .checked_add(plan.duration_us)
                    .context("Timeline is too long")?;
                Ok(Segment {
                    path: path.into(),
                    start_us,
                    end_us: cursor,
                    source_start: media.info.video_time_base.seconds(clip.in_pts),
                    source_end: media.info.video_time_base.seconds(clip.out_pts),
                })
            })
            .collect()
    }

    pub fn set_boundary(&mut self, index: usize, trim_in: bool, pts: i64) -> Result<()> {
        let mut clip = self
            .project
            .clips
            .get(index)
            .context("Select a clip")?
            .clone();
        if trim_in {
            clip.in_pts = pts;
        } else {
            clip.out_pts = pts;
        }
        self.source(&clip.source)?.1.index.plan_clip(&clip)?;
        self.change(|project| project.clips[index] = clip);
        Ok(())
    }

    pub fn cut_points(&self, index: usize, trim_in: bool) -> Result<Vec<i64>> {
        let clip = self.project.clips.get(index).context("Select a clip")?;
        let (_, media) = self.source(&clip.source)?;
        let mut candidates = media.info.safe_in_pts.clone();
        if !trim_in {
            candidates.push(media.info.video_end_pts);
        }
        Ok(candidates
            .into_iter()
            .filter(|&pts| {
                if trim_in {
                    pts < clip.out_pts
                } else {
                    pts > clip.in_pts
                }
            })
            .collect())
    }

    pub fn split(&mut self, index: usize, seconds: f64) -> Result<()> {
        let clip = self
            .project
            .clips
            .get(index)
            .context("Place the playhead on a clip")?;
        let (_, media) = self.source(&clip.source)?;
        let mut left = clip.clone();
        let mut right = clip.clone();
        let base = media.info.video_time_base;
        let mut candidates: Vec<_> = media
            .info
            .safe_in_pts
            .iter()
            .copied()
            .filter(|&pts| clip.in_pts < pts && pts < clip.out_pts)
            .collect();
        candidates.sort_by(|&a, &b| {
            (base.seconds(a) - seconds)
                .abs()
                .total_cmp(&(base.seconds(b) - seconds).abs())
        });
        let split = candidates.into_iter().find(|&pts| {
            left.out_pts = pts;
            right.in_pts = pts;
            media.index.plan_clip(&left).is_ok() && media.index.plan_clip(&right).is_ok()
        });
        ensure!(
            split.is_some(),
            "This clip has no safe split point inside its range"
        );
        self.change(|project| {
            project.clips[index] = left;
            project.clips.insert(index + 1, right);
        });
        Ok(())
    }
}

pub fn source_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub fn timecode(seconds: f64) -> String {
    let milliseconds = (seconds.abs() * 1000.0).round() as u64;
    let (whole, fraction) = (milliseconds / 1000, milliseconds % 1000);
    let sign = if seconds < 0.0 { "−" } else { "" };
    format!(
        "{sign}{:02}:{:02}:{:02}.{fraction:03}",
        whole / 3600,
        whole / 60 % 60,
        whole % 60
    )
}
