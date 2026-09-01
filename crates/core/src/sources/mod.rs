//! Per-agent log parsers. All vendor knowledge stops here.

pub mod claude;

use std::path::{Path, PathBuf};

use crate::common::model::Session;

pub struct Fragment {
    pub session: Session,
    /// End of the last **complete** line. A file not ending in a newline still has
    /// a line being written.
    pub consumed: u64,
    /// Rewound to the first line of the last response. A boundary cutting through a
    /// response would give each fragment a different fold state (seen / tool_slot);
    /// re-reading it whole is safe because failures use idempotent keys.
    pub resume_at: u64,
}

pub trait Source {
    fn name(&self) -> &'static str;

    fn discover(&self) -> Vec<PathBuf>;

    fn load(&self, path: &Path) -> std::io::Result<Session> {
        self.load_from(path, 0).map(|f| f.session)
    }

    fn load_from(&self, path: &Path, offset: u64) -> std::io::Result<Fragment>;
}

pub fn all() -> Vec<Box<dyn Source>> {
    vec![Box::new(claude::ClaudeCode)]
}

pub(crate) fn home() -> Option<PathBuf> {
    // Windows has no HOME; std falls back to USERPROFILE.
    std::env::home_dir()
}

/// From the filename: a subagent's `sessionId` field points at the parent.
pub(crate) fn session_id(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

pub(crate) fn walk(root: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        // file_type() does not follow symlinks; following them would cycle.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, ext, out);
        } else if kind.is_file() && path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

#[cfg(all(test, unix))]
mod walk_tests;
