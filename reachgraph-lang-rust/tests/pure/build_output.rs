//! `locate_out_dir` — ADR-0009's reading of cargo's own record.
//!
//! Plain directories in the system temporary directory, laid out the way cargo
//! lays out a target directory. No cargo, no build.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use reachgraph_lang_rust::build_output::locate_out_dir;

struct Target(PathBuf);

impl Target {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "reachgraph-build-output-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp is writable");
        Self(path)
    }

    /// One build-script run: `<profile>/build/<dir>/` with a `root-output`
    /// naming an `out/` that holds `file`, when `file` is given.
    fn run(&self, profile: &str, dir: &str, file: Option<&str>) -> PathBuf {
        let run = self.0.join(profile).join("build").join(dir);
        let out = run.join("out");
        fs::create_dir_all(&out).expect("out is creatable");
        if let Some(file) = file {
            fs::write(out.join(file), "pub fn f() {}\n").expect("generated file");
        }
        fs::write(run.join("root-output"), out.display().to_string()).expect("record");
        out
    }

    fn touch(&self, profile: &str, dir: &str, when: SystemTime) {
        let record = self
            .0
            .join(profile)
            .join("build")
            .join(dir)
            .join("root-output");
        let file = fs::File::options()
            .write(true)
            .open(&record)
            .expect("record exists");
        file.set_modified(when).expect("mtime is settable");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_recorded_out_dir_is_found_under_the_underscored_name() {
    let target = Target::new("found");
    let out = target.run("debug", "yadgar_task-0123abcd", Some("yadgar.task.v1.rs"));

    assert_eq!(locate_out_dir(target.path(), "yadgar-task"), Some(out));
}

/// `yadgar-task-db-<hash>` starts with `yadgar-task-`. Without the hash check
/// the task package would read the task-db package's generated code.
#[test]
fn a_package_whose_name_extends_another_is_not_read_for_it() {
    let target = Target::new("prefix");
    target.run("debug", "yadgar-task-db-0123abcd", Some("db.rs"));

    assert_eq!(locate_out_dir(target.path(), "yadgar-task"), None);
}

/// A run that wrote no Rust source has nothing the crate graph could load.
#[test]
fn an_out_dir_with_no_rust_source_is_not_an_output() {
    let target = Target::new("empty");
    target.run("debug", "m-0123abcd", None);

    assert_eq!(locate_out_dir(target.path(), "m"), None);
}

/// Several builds of one package: the most recently recorded one is the build
/// the caller ran last.
#[test]
fn the_most_recent_record_wins() {
    let target = Target::new("newest");
    let old = target.run("debug", "m-00000001", Some("gen.rs"));
    let new = target.run("release", "m-00000002", Some("gen.rs"));
    let now = SystemTime::now();
    target.touch("debug", "m-00000001", now - Duration::from_secs(3600));
    target.touch("release", "m-00000002", now);

    assert_eq!(locate_out_dir(target.path(), "m"), Some(new.clone()));

    target.touch("debug", "m-00000001", now + Duration::from_secs(3600));
    assert_eq!(locate_out_dir(target.path(), "m"), Some(old));
}

#[test]
fn a_missing_target_finds_nothing() {
    let target = Target::new("missing");
    assert_eq!(locate_out_dir(&target.path().join("nope"), "m"), None);
}
