//! Guards for the single owners of plan 06: each decision is made in one
//! place, and these rules fail when a second place appears.
//!
//! Each rule reads the files under `src/` with plain string matching and is
//! one `#[test]`; its failure message names the rule and every offending
//! file.  Where the compiler can enforce an owner (a private field, a
//! required parameter) it does, and no rule is needed here.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under `src/`, as (path relative to the crate root,
/// contents).
fn source_files() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pending: Vec<PathBuf> = vec![root.join("src")];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read a source directory") {
            let path = entry.expect("read a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let relative = path
                    .strip_prefix(root)
                    .expect("a path under the crate root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let contents = fs::read_to_string(&path).expect("read a source file");
                files.push((relative, contents));
            }
        }
    }
    files.sort();
    files
}

/// The files under `src/` that contain any of `needles`.
fn files_containing(needles: &[&str]) -> Vec<String> {
    source_files()
        .into_iter()
        .filter(|(_, contents)| needles.iter().any(|needle| contents.contains(needle)))
        .map(|(path, _)| path)
        .collect()
}

/// Whether a declaration's PHPDoc type was written is a stored fact
/// (`doc_type` / `doc_return_type`, plan 06 step 1.2), never guessed from
/// `effective == native`.  The two guessing helpers that inheritance used
/// must not come back.
#[test]
fn no_doc_type_guessing() {
    let offenders = files_containing(&["lacks_docblock_override", "ancestor_has_richer_type"]);
    assert!(
        offenders.is_empty(),
        "rule no_doc_type_guessing: read `doc_type` / `doc_return_type` instead of \
         guessing from the effective and native types; found in: {offenders:?}"
    );
}
