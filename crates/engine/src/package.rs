//! A `.screencraft` package: a folder holding `screen.mp4`, `input.json` and `project.json`.

use screencraft_core::input::InputLog;
use screencraft_core::project::Project;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Package {
    pub dir: PathBuf,
}

impl Package {
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Folder for a new recording inside `root`, named from the current time.
    pub fn create_in(root: &Path, stamp: &str) -> std::io::Result<Self> {
        let mut dir = root.join(format!("Recording {stamp}.screencraft"));
        let mut n = 2;
        while dir.exists() {
            dir = root.join(format!("Recording {stamp} {n}.screencraft"));
            n += 1;
        }
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn project_path(&self) -> PathBuf {
        self.dir.join("project.json")
    }

    pub fn load_project(&self) -> Result<Project, String> {
        let bytes = std::fs::read(self.project_path()).map_err(|e| format!("cannot read project: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid project: {e}"))
    }

    pub fn save_project(&self, project: &Project) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
        // Write beside, then rename: a crash mid-save never leaves a half-written project.
        let tmp = self.dir.join("project.json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, self.project_path()).map_err(|e| e.to_string())
    }

    pub fn load_input(&self, project: &Project) -> InputLog {
        let mut log: InputLog = std::fs::read(self.dir.join(&project.media.input))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        log.refine_carets();
        log
    }

    pub fn screen_path(&self, project: &Project) -> PathBuf {
        self.dir.join(&project.media.screen)
    }
}

/// Recordings in `root`, newest first: (package, modified seconds since epoch).
pub fn list_packages(root: &Path) -> Vec<(Package, u64)> {
    let mut out: Vec<(Package, u64)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "screencraft").unwrap_or(false) && e.path().join("project.json").exists())
        .map(|e| {
            let m = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (Package::open(e.path()), m)
        })
        .collect();
    out.sort_by_key(|(_, m)| std::cmp::Reverse(*m));
    out
}
