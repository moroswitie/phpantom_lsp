//! Exercise the production build script through Cargo, rather than a watch mock.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

use serde_json::Value;
use tempfile::TempDir;

struct Build {
    _temp: TempDir,
    root: PathBuf,
    target: PathBuf,
    stamp: Option<SystemTime>,
    external: Option<PathBuf>,
    version: Option<String>,
}

struct Result {
    script_ran: bool,
    compiled: bool,
    text: String,
    out_dir: PathBuf,
    diagnostics: String,
}

impl Result {
    #[track_caller]
    fn reused(&self) {
        assert!(!self.script_ran, "build script reran: {}", self.diagnostics);
        assert!(!self.compiled, "binary recompiled: {}", self.diagnostics);
    }

    #[track_caller]
    fn changed(&self) {
        assert!(self.script_ran, "build script did not rerun");
        assert!(self.compiled, "binary did not recompile");
    }
}

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    successful(
        Command::new("git")
            .arg("--no-optional-locks")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .output()
            .unwrap(),
    )
}

fn write(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

impl Build {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        let target = temp.path().join("target");
        for (name, contents) in [
            ("build.rs", include_str!("../build.rs")),
            (
                "build_support/git_version.rs",
                include_str!("../build_support/git_version.rs"),
            ),
            ("src/stub_map.rs", include_str!("../src/stub_map.rs")),
            ("stubs/README.md", include_str!("../stubs/README.md")),
            ("stubs.lock", include_str!("../stubs.lock")),
            ("Cargo.lock", include_str!("../Cargo.lock")),
            (
                "Cargo.toml",
                r#"[package]
name = "build-tracking-probe"
version = "1.2.3"
edition = "2024"

[features]
default = ["offline-stubs"]
offline-stubs = []

[build-dependencies]
ureq = "3"
tar = "0.4"
flate2 = "1"
"#,
            ),
            (
                "src/main.rs",
                r#"include!(concat!(env!("OUT_DIR"), "/stub_map_generated.rs"));
fn main() {
    println!("{}|{}|{}|{}|{}|{}",
        env!("PHPANTOM_GIT_VERSION"), env!("PHPANTOM_STUBS_VERSION"),
        STUB_CLASS_MAP.len(), STUB_FUNCTION_MAP.len(), STUB_CONSTANT_MAP.len(),
        STUB_FILES.join(";"));
}
"#,
            ),
            (".gitignore", "/stubs/*\n!/stubs/README.md\n"),
            (
                "src/unused.rs",
                "// Tracked source outside the module tree.\n",
            ),
            ("docs/guide.md", "Tracked documentation\n"),
            ("tests/data.php", "<?php // Tracked test data\n"),
            ("VERSION-NOTES.md", "Tracked root file\n"),
        ] {
            write(root.join(name), contents);
        }
        Self {
            _temp: temp,
            root,
            target,
            stamp: None,
            external: None,
            version: None,
        }
    }

    fn git(&self, args: &[&str]) -> String {
        git_in(&self.root, args)
    }

    fn init_git(&self) {
        self.git(&["init", "-b", "main"]);
        self.git(&["config", "user.name", "Build test"]);
        self.git(&["config", "user.email", "build@example.invalid"]);
        self.git(&["config", "core.autocrlf", "false"]);
        self.git(&["add", "."]);
        self.git(&["commit", "-m", "Initial fixture"]);
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO"));
        command
            .args(["build", "--offline", "--message-format=json"])
            .current_dir(&self.root)
            .env("CARGO_TARGET_DIR", &self.target)
            .env("CARGO_LOG", "cargo::compiler::fingerprint=info")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_BUILD_TARGET")
            .env_remove("RUSTC_WORKSPACE_WRAPPER")
            .env_remove("PHPANTOM_STUBS_DIR")
            .env_remove("PHPANTOM_GIT_VERSION");
        if let Some(path) = &self.external {
            command.env("PHPANTOM_STUBS_DIR", path);
        }
        if let Some(version) = &self.version {
            command.env("PHPANTOM_GIT_VERSION", version);
        }
        command
    }

    fn build(&mut self) -> Result {
        let output = self.command().output().unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr).into_owned();
        let output = successful(output);
        let messages: Vec<Value> = output
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let out_dir = messages
            .iter()
            .find(|message| {
                message["reason"] == "build-script-executed"
                    && message["package_id"]
                        .as_str()
                        .unwrap()
                        .contains("build-tracking-probe")
            })
            .unwrap()["out_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap();
        let stamp = fs::metadata(out_dir.parent().unwrap().join("output"))
            .unwrap()
            .modified()
            .unwrap();
        let binary = messages
            .iter()
            .find(|message| {
                message["reason"] == "compiler-artifact" && message["target"]["kind"][0] == "bin"
            })
            .unwrap();
        let text = successful(
            Command::new(binary["executable"].as_str().unwrap())
                .output()
                .unwrap(),
        );
        let result = Result {
            script_ran: self.stamp != Some(stamp),
            compiled: binary["fresh"] == false,
            text,
            out_dir,
            diagnostics,
        };
        self.stamp = Some(stamp);
        result
    }

    fn local(&self) -> PathBuf {
        self.root.join("stubs/jetbrains/phpstorm-stubs")
    }
}

fn stub_tree(path: &Path, value: &str) {
    write(
        path.join("PhpStormStubsMap.php"),
        "<?php\nconst CLASSES = array (\n'Probe' => 'probe.php',\n'Later' => 'later/new.php',\n);\n",
    );
    write(path.join("probe.php"), value);
    let lock: toml::Value = toml::from_str(include_str!("../stubs.lock")).unwrap();
    write(path.join(".commit"), lock["commit"].as_str().unwrap());
}

#[test]
fn build_script_tracks_stub_transitions_without_rebuilding_unchanged_inputs() {
    let mut fixture = Build::new();
    let first = fixture.build();
    assert!(first.text.starts_with("1.2.3|none|0|0|0|"));
    first.changed();
    fixture.build().reused();
    write(fixture.target.join("tmp/test-output"), "unrelated");
    write(fixture.root.join("untracked-output"), "unrelated");
    write(first.out_dir.join("unrelated-output"), "unrelated");
    fixture.build().reused();

    stub_tree(&fixture.local(), "local one");
    let local = fixture.build();
    local.changed();
    assert!(local.text.contains("|1|0|0|local one"));
    fixture.build().reused();
    write(fixture.local().join("probe.php"), "local two");
    let edited = fixture.build();
    edited.changed();
    assert!(edited.text.contains("local two"));
    fixture.build().reused();

    write(fixture.local().join("later/new.php"), "new declaration");
    let added = fixture.build();
    added.changed();
    assert!(added.text.contains("|2|0|0|new declaration;local two"));
    fixture.build().reused();
    fs::remove_file(fixture.local().join("later/new.php")).unwrap();
    let removed = fixture.build();
    removed.changed();
    assert!(removed.text.contains("|1|0|0|local two"));
    fixture.build().reused();
    write(
        fixture.local().join("later/new.php"),
        "restored declaration",
    );
    fixture.build().changed();
    fixture.build().reused();

    fs::remove_file(fixture.local().join("PhpStormStubsMap.php")).unwrap();
    let missing_map = fixture.build();
    missing_map.changed();
    assert!(missing_map.text.contains("|none|0|0|0|"));
    fixture.build().reused();
    stub_tree(&fixture.local(), "restored map");
    fixture.build().changed();
    fs::remove_file(fixture.local().join(".commit")).unwrap();
    let missing_marker = fixture.build();
    missing_marker.changed();
    assert!(missing_marker.text.contains("|none|0|0|0|"));
    fixture.build().reused();
    write(fixture.local().join(".commit"), "stale");
    fixture.build().changed();
    fixture.build().reused();

    // A current download cache takes over from stale local declarations.
    let cached = first.out_dir.join("stubs/jetbrains/phpstorm-stubs");
    stub_tree(&cached, "cached");
    write(fixture.local().join(".commit"), "another stale pin");
    let cache = fixture.build();
    cache.changed();
    assert!(cache.text.contains("|1|0|0|cached"));
    fixture.build().reused();
    write(cached.join("probe.php"), "edited cache");
    let cache_edit = fixture.build();
    cache_edit.changed();
    assert!(cache_edit.text.contains("edited cache"));
    fixture.build().reused();

    let external = fixture._temp.path().join("external");
    stub_tree(&external, "external");
    fs::remove_file(external.join(".commit")).unwrap();
    fixture.external = Some(external.clone());
    let supplied = fixture.build();
    supplied.changed();
    assert!(supplied.text.contains("|external|1|0|0|external"));
    fixture.build().reused();
    write(external.join("probe.php"), "external edit");
    let external_edit = fixture.build();
    external_edit.changed();
    assert!(external_edit.text.contains("external edit"));
    fixture.build().reused();
    write(external.join("later/new.php"), "external addition");
    fixture.build().changed();
    fixture.build().reused();
    let map = external.join("PhpStormStubsMap.php");
    write(
        &map,
        &format!(
            "{}\nconst FUNCTIONS = array (\n'probe' => 'probe.php',\n);\nconst CONSTANTS = array (\n'PROBE' => 'probe.php',\n);\n",
            fs::read_to_string(&map).unwrap()
        ),
    );
    let map_edit = fixture.build();
    map_edit.changed();
    assert!(map_edit.text.contains("|external|2|1|1|"));
    fixture.build().reused();

    // These builds must not change any supplied declaration input.
    assert_eq!(
        fs::read_to_string(external.join("probe.php")).unwrap(),
        "external edit"
    );
    assert!(!external.join(".commit").exists());
    assert_eq!(
        fs::read_to_string(fixture.local().join(".commit")).unwrap(),
        "another stale pin"
    );

    fs::remove_dir_all(fixture.root.join("stubs")).unwrap();
    let failed = fixture.command().output().unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("git restore stubs/README.md"));
}

#[test]
fn build_script_tracks_git_versions_in_checkouts_and_linked_worktrees() {
    let mut fixture = Build::new();
    // Resolve the fixture lockfile before committing it.
    fixture.build();
    fixture.init_git();
    // Creating a repository is accompanied by a source change here, just as a
    // fresh checkout has no cached build-script result from before git init.
    write(
        fixture.root.join("build.rs"),
        &format!(
            "{}\n",
            fs::read_to_string(fixture.root.join("build.rs")).unwrap()
        ),
    );
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "Source change"]);
    let clean = fixture.build();
    assert_eq!(
        clean.text.split('|').next().unwrap(),
        fixture.git(&["describe", "--tags", "--always"]).trim()
    );
    fixture.build().reused();

    // Tracked files that are not compiled into the binary never rebuild it,
    // whether they are edited, deleted, staged or restored.
    write(fixture.root.join("docs/guide.md"), "Edited documentation\n");
    write(
        fixture.root.join("tests/data.php"),
        "<?php // Edited test data\n",
    );
    fs::remove_file(fixture.root.join("VERSION-NOTES.md")).unwrap();
    let unrelated = fixture.build();
    unrelated.reused();
    assert_eq!(unrelated.text, clean.text);
    fixture.git(&["add", "--all"]);
    fixture.build().reused();
    fixture.git(&["restore", "--source=HEAD", "--staged", "--worktree", "."]);
    fixture.build().reused();

    // Missing optional Git paths must not turn into permanent rebuild triggers.
    fs::remove_dir(fixture.root.join(".git/refs/tags")).unwrap();
    fixture.build().changed();
    fixture.build().reused();
    fixture.git(&["tag", "nested/tag"]);
    fixture.build().changed();
    fixture.build().reused();
    fixture.git(&["tag", "-d", "nested/tag"]);
    fixture.build().changed();
    fixture.build().reused();
    let source = fixture.root.join("src/main.rs");
    let original = fs::read_to_string(&source).unwrap();
    write(&source, &format!("{original}\n// dirty\n"));
    let dirty = fixture.build();
    dirty.changed();
    assert!(dirty.text.split('|').next().unwrap().ends_with("-dirty"));
    fixture.build().reused();
    // Staging keeps the binary and its version unchanged.
    fixture.git(&["add", "src/main.rs"]);
    fixture.build().reused();
    fixture.git(&["restore", "--staged", "src/main.rs"]);
    fixture.git(&["restore", "src/main.rs"]);
    let restored = fixture.build();
    restored.changed();
    assert_eq!(restored.text, clean.text);
    fixture.build().reused();
    // A deleted source is watched through its directory, not rerun forever.
    fs::remove_file(fixture.root.join("src/unused.rs")).unwrap();
    let removed_source = fixture.build();
    removed_source.changed();
    assert!(
        removed_source
            .text
            .split('|')
            .next()
            .unwrap()
            .ends_with("-dirty")
    );
    fixture.build().reused();
    fixture.git(&["restore", "src/unused.rs"]);
    let restored_source = fixture.build();
    restored_source.changed();
    assert_eq!(restored_source.text, clean.text);
    fixture.build().reused();

    fixture.git(&["commit", "--allow-empty", "-m", "Next revision"]);
    let committed = fixture.build();
    committed.changed();
    assert_ne!(committed.text, clean.text);
    fixture.build().reused();
    fixture.git(&["tag", "v1"]);
    let tagged = fixture.build();
    tagged.changed();
    assert!(tagged.text.starts_with("v1|"));
    fixture.build().reused();
    fixture.git(&["pack-refs", "--all"]);
    let packed = fixture.build();
    packed.changed();
    assert_eq!(packed.text, tagged.text);
    fixture.build().reused();
    fixture.git(&["tag", "-d", "v1"]);
    let untagged = fixture.build();
    untagged.changed();
    assert_eq!(untagged.text, committed.text);
    fixture.build().reused();
    fixture.git(&["switch", "--detach", "HEAD~1"]);
    let detached = fixture.build();
    detached.changed();
    assert_eq!(detached.text, clean.text);
    fixture.build().reused();
    fixture.git(&["switch", "main"]);
    fixture.build().changed();
    fixture.build().reused();

    let original = fixture.root.clone();
    let worktree = fixture._temp.path().join("linked");
    fixture.git(&[
        "worktree",
        "add",
        "-b",
        "linked",
        worktree.to_str().unwrap(),
    ]);
    fixture.root = worktree;
    fixture.stamp = None;
    let linked = fixture.build();
    assert_eq!(linked.text, committed.text);
    fixture.build().reused();
    // Commits in another checkout of the same repository leave this build
    // alone, also once this worktree's branch is packed and has no file.
    git_in(
        &original,
        &["commit", "--allow-empty", "-m", "Other checkout"],
    );
    fixture.build().reused();
    fixture.git(&["pack-refs", "--all"]);
    fixture.build().changed();
    fixture.build().reused();
    git_in(&original, &["commit", "--allow-empty", "-m", "Other again"]);
    fixture.build().reused();
    fixture.git(&["commit", "--allow-empty", "-m", "Linked revision"]);
    let linked_commit = fixture.build();
    linked_commit.changed();
    assert_ne!(linked_commit.text, linked.text);
    fixture.build().reused();
    fixture.git(&["tag", "linked-tag"]);
    let linked_tag = fixture.build();
    linked_tag.changed();
    assert!(linked_tag.text.starts_with("linked-tag|"));
    fixture.build().reused();

    fixture.version = Some("release-override".to_string());
    let overridden = fixture.build();
    overridden.changed();
    assert!(overridden.text.starts_with("release-override|"));
    fixture.build().reused();
    fixture.git(&["commit", "--allow-empty", "-m", "Override stays explicit"]);
    fixture.build().reused();
    fixture.version = None;
    fixture.build().changed();
    fixture.build().reused();
}
