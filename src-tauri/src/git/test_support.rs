//! Test-only helpers for spinning up disposable git repositories.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

pub(crate) struct TestRepo {
    _dir: TempDir,
    path: PathBuf,
}

impl TestRepo {
    pub(crate) fn new(label: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("uclaw-git-test-{label}-"))
            .tempdir()
            .expect("create temp dir for git test repo");
        let path = dir.path().to_path_buf();

        run(&path, "git", &["init", "--initial-branch=main"])
            .or_else(|_| {
                run(&path, "git", &["init"])?;
                run(&path, "git", &["branch", "-m", "main"])
            })
            .expect("git init must succeed");

        run(&path, "git", &["config", "user.name", "uClaw Tests"]).expect("git config user.name");
        run(&path, "git", &["config", "user.email", "tests@uclaw.local"])
            .expect("git config user.email");
        run(&path, "git", &["config", "commit.gpgsign", "false"]).expect("disable gpg sign");

        std::fs::write(path.join("README.md"), "seed\n").expect("write seed file");
        run(&path, "git", &["add", "README.md"]).expect("git add seed");
        run(&path, "git", &["commit", "-m", "chore: seed"]).expect("git commit seed");

        Self { _dir: dir, path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

fn run(cwd: &Path, program: &str, args: &[&str]) -> std::io::Result<()> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
