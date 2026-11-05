//! Git inputs for the build-time version, including linked worktrees.
//!
//! Cargo recompiles PHPantom whenever the build script reruns, so only inputs
//! that can change the embedded version are watched.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Tracked paths compiled into the binary. Only their uncommitted changes add
/// `-dirty`, so editing tests, documentation or examples never rebuilds it.
const BINARY_INPUTS: [&str; 6] = [
    "src",
    "build.rs",
    "build_support",
    "Cargo.toml",
    "Cargo.lock",
    "stubs.lock",
];

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        // Read-only Git commands must not rewrite the index.
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\r', '\n'])
            .to_string()
    })
}

fn git_path(root: &Path, name: &str) -> Option<PathBuf> {
    // Relative to `root` in a normal checkout, absolute in a linked worktree.
    git(root, &["rev-parse", "--git-path", name]).map(|path| root.join(path))
}

/// Describe this checkout and register its existing version inputs with Cargo.
pub fn describe(root: &Path) -> Option<String> {
    // Source packages may be built inside another checkout's target directory.
    // That enclosing repository does not describe the package being built.
    let top = git(root, &["rev-parse", "--show-toplevel"])?;
    if fs::canonicalize(top).ok()? != fs::canonicalize(root).ok()? {
        return None;
    }

    let mut paths = BTreeSet::new();
    for name in ["HEAD", "packed-refs"] {
        if let Some(path) = git_path(root, name)
            && path.exists()
        {
            paths.insert(path);
        }
    }
    if let Some(refs) = git_path(root, "refs") {
        if let Some(tags) = git_path(root, "refs/tags") {
            watch_refs(&tags, &refs, &mut paths);
        }
        if let Some(branch) = git(root, &["symbolic-ref", "-q", "HEAD"])
            && let Some(path) = git_path(root, &branch)
        {
            if path.is_file() {
                paths.insert(path);
            } else if let Some(log) = git_path(root, "logs/HEAD")
                && log.is_file()
            {
                // A packed branch has no file of its own. This worktree's HEAD
                // log records its commits; a refs directory would also record
                // commits made on other branches and in other worktrees.
                paths.insert(log);
            } else {
                watch_refs(&path, &refs, &mut paths);
            }
        }
    }

    // The index is not watched: staging leaves the binary unchanged, and a
    // commit moves HEAD or the branch watched above.
    let tracked = git(
        root,
        &[&["ls-files", "-z", "--"], &BINARY_INPUTS[..]].concat(),
    )?;
    for name in tracked.split('\0').filter(|name| !name.is_empty()) {
        // Watch a deleted source through its nearest directory, so it is not
        // a missing file that reruns forever. Never climb to the project root:
        // Cargo scans watched directories recursively, including target/.
        let mut path = root.join(name);
        while !path.exists() && path.parent().is_some_and(|parent| parent != root) {
            path.pop();
        }
        paths.insert(path);
    }
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let mut version = git(root, &["describe", "--tags", "--always"])?;
    // Unlike `describe --dirty` and `diff`, status respects optional locking
    // when refreshing stat information after identical content is restored.
    let status = git(
        root,
        &[
            &[
                "status",
                "--porcelain=v1",
                "--untracked-files=no",
                "--ignore-submodules=untracked",
                "--",
            ],
            &BINARY_INPUTS[..],
        ]
        .concat(),
    )?;
    if !status.is_empty() {
        version.push_str("-dirty");
    }
    Some(version)
}

fn watch_refs(path: &Path, refs: &Path, paths: &mut BTreeSet<PathBuf>) {
    if !path.exists() {
        let mut parent = path.to_path_buf();
        while !parent.exists() && parent.starts_with(refs) {
            parent.pop();
        }
        if parent.starts_with(refs) && parent.exists() {
            paths.insert(parent);
        }
        return;
    }
    paths.insert(path.to_path_buf());
    // Existing files give immediate deletion detection in addition to Cargo's
    // recursive directory watch, including nested tag names.
    if path.is_dir()
        && let Ok(entries) = fs::read_dir(path)
    {
        for entry in entries.flatten() {
            watch_refs(&entry.path(), refs, paths);
        }
    }
}
