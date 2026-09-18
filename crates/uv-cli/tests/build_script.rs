use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, anyhow, bail, ensure};
use fs_err as fs;

const BUILD_SCRIPT: &str = include_str!("../build.rs");
const ENVIRONMENT_VARIABLES: &str = r#"struct EnvVars;

impl EnvVars {
    const CARGO_MANIFEST_DIR: &'static str = "CARGO_MANIFEST_DIR";
    const TARGET: &'static str = "TARGET";
    const UV_COMMIT_HASH: &'static str = "UV_COMMIT_HASH";
    const UV_COMMIT_SHORT_HASH: &'static str = "UV_COMMIT_SHORT_HASH";
    const UV_COMMIT_DATE: &'static str = "UV_COMMIT_DATE";
    const UV_LAST_TAG: &'static str = "UV_LAST_TAG";
    const UV_LAST_TAG_DISTANCE: &'static str = "UV_LAST_TAG_DISTANCE";
}"#;

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum BuildState {
    Compiled,
    Dirty,
    Fresh,
}

#[derive(Debug)]
struct Build {
    state: BuildState,
    commit: String,
    tag: String,
    distance: String,
}

struct Fixture {
    directory: PathBuf,
    repository: PathBuf,
    target: PathBuf,
    git_configuration: PathBuf,
}

impl Fixture {
    fn new(name: &str, reftable: bool) -> Result<Option<Self>> {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "uv-cli-build-script-{name}-{}-{sequence}",
            std::process::id()
        ));
        let repository = directory.join("repository");
        fs::create_dir_all(&repository)?;
        let git_configuration = directory.join("gitconfig");
        fs::write(&git_configuration, "")?;
        let fixture = Self {
            target: directory.join("target"),
            directory,
            repository,
            git_configuration,
        };

        let mut arguments = vec!["init", "--quiet", "--initial-branch=main"];
        if reftable {
            arguments.push("--ref-format=reftable");
        }
        let output = fixture
            .git_command(&fixture.repository, &arguments)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if reftable
                && (stderr.contains("unknown option")
                    || stderr.contains("unknown ref storage format")
                    || stderr.contains("unknown ref format")
                    || stderr.contains("not supported"))
            {
                return Ok(None);
            }
            bail!("failed to initialize Git repository: {stderr}");
        }

        let package = fixture.repository.join("crates").join("probe");
        fs::create_dir_all(package.join("src"))?;
        fs::write(
            package.join("Cargo.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        fs::write(package.join("build.rs"), Self::build_script()?)?;
        fs::write(
            package.join("src").join("main.rs"),
            r#"fn main() {
    println!(
        "{}\n{}\n{}",
        option_env!("UV_COMMIT_HASH").unwrap_or("none"),
        option_env!("UV_LAST_TAG").unwrap_or("none"),
        option_env!("UV_LAST_TAG_DISTANCE").unwrap_or("none"),
    );
}
"#,
        )?;
        fs::write(fixture.repository.join("tracked.txt"), "initial\n")?;
        fixture.git(&fixture.repository, &["add", "."])?;
        fixture.git(
            &fixture.repository,
            &["commit", "--quiet", "--message=Initial commit"],
        )?;
        Ok(Some(fixture))
    }

    fn build_script() -> Result<String> {
        ensure!(
            BUILD_SCRIPT.matches("use fs_err as fs;").count() == 1,
            "the build script must contain exactly one fs_err import"
        );
        ensure!(
            BUILD_SCRIPT.matches("use uv_static::EnvVars;").count() == 1,
            "the build script must contain exactly one EnvVars import"
        );
        Ok(BUILD_SCRIPT
            .replace("use fs_err as fs;", "use std::fs;")
            .replace("use uv_static::EnvVars;", ENVIRONMENT_VARIABLES))
    }

    fn git_command(&self, directory: &Path, arguments: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .current_dir(directory)
            .env("GIT_CONFIG_GLOBAL", &self.git_configuration)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .args(["-c", "user.name=Build Script Test"])
            .args(["-c", "user.email=build-script@example.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .args(["-c", "maintenance.auto=false"])
            .args(["-c", "gc.auto=0"])
            .arg("-c")
            .arg(format!(
                "core.hooksPath={}",
                self.directory.join("no-hooks").display()
            ))
            .args(arguments);
        command
    }

    fn git(&self, directory: &Path, arguments: &[&str]) -> Result<String> {
        let mut command = self.git_command(directory, arguments);
        Self::run(&mut command)
    }

    fn commit(&self, directory: &Path, message: &str) -> Result<String> {
        self.git(
            directory,
            &["commit", "--allow-empty", "--quiet", "--message", message],
        )?;
        self.head(directory)
    }

    fn head(&self, directory: &Path) -> Result<String> {
        Ok(self
            .git(directory, &["rev-parse", "HEAD"])?
            .trim()
            .to_string())
    }

    fn add_worktree(&self, name: &str, branch: Option<&str>) -> Result<PathBuf> {
        let worktree = self.directory.join(name);
        let worktree_argument = worktree
            .to_str()
            .context("the worktree path must be valid UTF-8")?;
        if let Some(branch) = branch {
            self.git(
                &self.repository,
                &[
                    "worktree",
                    "add",
                    "--quiet",
                    "-b",
                    branch,
                    worktree_argument,
                ],
            )?;
        } else {
            self.git(
                &self.repository,
                &["worktree", "add", "--quiet", "--detach", worktree_argument],
            )?;
        }
        Ok(worktree)
    }

    fn build(&self, repository: &Path) -> Result<Build> {
        let manifest = repository.join("crates").join("probe").join("Cargo.toml");
        let mut command = Command::new(env!("CARGO"));
        command
            .current_dir(repository)
            .env("CARGO_NET_OFFLINE", "true")
            .env("CARGO_TARGET_DIR", &self.target)
            .env("CARGO_TERM_COLOR", "never")
            .env("GIT_CONFIG_GLOBAL", &self.git_configuration)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("CARGO_BUILD_TARGET")
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("CARGO_MAKEFLAGS")
            .env_remove("MAKEFLAGS")
            .env_remove("MFLAGS")
            .args(["build", "--offline", "--verbose", "--manifest-path"])
            .arg(manifest)
            .arg("--target-dir")
            .arg(&self.target);
        let output = command.output()?;
        Self::ensure_success(&command, &output)?;
        let stderr = String::from_utf8(output.stderr)?;
        let mut state = None;
        for line in stderr.lines() {
            let mut words = line.split_whitespace();
            match (words.next(), words.next()) {
                (Some("Dirty"), Some("probe")) => state = Some(BuildState::Dirty),
                (Some("Fresh"), Some("probe")) => state = Some(BuildState::Fresh),
                (Some("Compiling"), Some("probe")) if state.is_none() => {
                    state = Some(BuildState::Compiled);
                }
                _ => {}
            }
        }
        let state = state.ok_or_else(|| anyhow!("missing Cargo build event in:\n{stderr}"))?;
        let executable = self
            .target
            .join("debug")
            .join(format!("probe{}", std::env::consts::EXE_SUFFIX));
        let mut executable_command = Command::new(executable);
        executable_command.current_dir(repository);
        let stdout = Self::run(&mut executable_command)?;
        let mut lines = stdout.lines();
        let commit = lines.next().context("missing embedded commit")?.to_string();
        let tag = lines.next().context("missing embedded tag")?.to_string();
        let distance = lines
            .next()
            .context("missing embedded tag distance")?
            .to_string();
        ensure!(lines.next().is_none(), "unexpected embedded version output");
        Ok(Build {
            state,
            commit,
            tag,
            distance,
        })
    }

    fn run(command: &mut Command) -> Result<String> {
        let output = command.output()?;
        Self::ensure_success(command, &output)?;
        String::from_utf8(output.stdout).context("command output was not valid UTF-8")
    }

    fn ensure_success(command: &Command, output: &Output) -> Result<()> {
        if output.status.success() {
            Ok(())
        } else {
            bail!(
                "command `{}` failed: {}",
                Self::display_command(command),
                String::from_utf8_lossy(&output.stderr)
            )
        }
    }

    fn display_command(command: &Command) -> String {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(OsStr::to_string_lossy)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn packed_branches_track_tag_changes_without_rebuilding_for_git_add() -> Result<()> {
    let fixture = Fixture::new("packed-tags", false)?
        .context("the standard Git reference format should be available")?;
    fixture.git(&fixture.repository, &["tag", "v0.1.0"])?;
    let head = fixture.commit(&fixture.repository, "Advance past the first tag")?;
    fixture.git(&fixture.repository, &["pack-refs", "--all", "--prune"])?;

    let first = fixture.build(&fixture.repository)?;
    assert_eq!(first.state, BuildState::Compiled);
    assert_eq!(first.commit, head);
    assert_eq!(first.tag, "v0.1.0");
    assert_eq!(first.distance, "1");
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Fresh);

    fixture.git(&fixture.repository, &["tag", "v0.2.0"])?;
    let updated = fixture.build(&fixture.repository)?;
    assert_eq!(updated.state, BuildState::Dirty);
    assert_eq!(updated.commit, head);
    assert_eq!(updated.tag, "v0.2.0");
    assert_eq!(updated.distance, "0");

    fixture.git(&fixture.repository, &["pack-refs", "--all", "--prune"])?;
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Dirty);
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Fresh);
    fixture.git(&fixture.repository, &["tag", "--delete", "v0.2.0"])?;
    let untagged = fixture.build(&fixture.repository)?;
    assert_eq!(untagged.state, BuildState::Dirty);
    assert_eq!(untagged.commit, head);
    assert_eq!(untagged.tag, "v0.1.0");
    assert_eq!(untagged.distance, "1");
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Fresh);

    fs::write(fixture.repository.join("tracked.txt"), "staged\n")?;
    fixture.git(&fixture.repository, &["add", "tracked.txt"])?;
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Fresh);
    Ok(())
}

#[test]
fn detached_worktrees_track_shared_tag_changes() -> Result<()> {
    let fixture = Fixture::new("detached-tags", false)?
        .context("the standard Git reference format should be available")?;
    fixture.git(&fixture.repository, &["tag", "v0.1.0"])?;
    let head = fixture.commit(&fixture.repository, "Advance past the first tag")?;
    let worktree = fixture.add_worktree("worktree", None)?;

    let first = fixture.build(&worktree)?;
    assert_eq!(first.commit, head);
    assert_eq!(first.tag, "v0.1.0");
    assert_eq!(first.distance, "1");
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);

    fixture.git(&fixture.repository, &["tag", "v0.2.0"])?;
    let updated = fixture.build(&worktree)?;
    assert_eq!(updated.state, BuildState::Dirty);
    assert_eq!(updated.commit, head);
    assert_eq!(updated.tag, "v0.2.0");
    assert_eq!(updated.distance, "0");
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);
    Ok(())
}

#[test]
fn linked_worktrees_track_private_references() -> Result<()> {
    let fixture = Fixture::new("private-reference", false)?
        .context("the standard Git reference format should be available")?;
    let worktree = fixture.add_worktree("worktree", None)?;
    let initial = fixture.head(&worktree)?;
    fixture.git(
        &worktree,
        &["update-ref", "refs/worktree/current", &initial],
    )?;
    fixture.git(
        &worktree,
        &["symbolic-ref", "HEAD", "refs/worktree/current"],
    )?;

    assert_eq!(fixture.build(&worktree)?.commit, initial);
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);
    let head = fixture.commit(&worktree, "Advance the worktree-private reference")?;
    assert_ne!(head, initial);
    let updated = fixture.build(&worktree)?;
    assert_eq!(updated.state, BuildState::Dirty);
    assert_eq!(updated.commit, head);
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);
    Ok(())
}

#[test]
fn linked_worktrees_track_chained_symbolic_references() -> Result<()> {
    let fixture = Fixture::new("symbolic-reference", false)?
        .context("the standard Git reference format should be available")?;
    let worktree = fixture.add_worktree("worktree", None)?;
    let initial = fixture.head(&worktree)?;
    fixture.git(&worktree, &["update-ref", "refs/heads/target", &initial])?;
    fixture.git(
        &worktree,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/target"],
    )?;
    fixture.git(&worktree, &["symbolic-ref", "HEAD", "refs/heads/alias"])?;

    assert_eq!(fixture.build(&worktree)?.commit, initial);
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);
    let head = fixture.commit(&worktree, "Advance the symbolic reference target")?;
    assert_ne!(head, initial);
    let updated = fixture.build(&worktree)?;
    assert_eq!(updated.state, BuildState::Dirty);
    assert_eq!(updated.commit, head);
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);
    Ok(())
}

#[test]
fn reftable_repositories_track_commits_and_branch_changes() -> Result<()> {
    let Some(fixture) = Fixture::new("reftable", true)? else {
        return Ok(());
    };
    let initial = fixture.head(&fixture.repository)?;
    assert_eq!(fixture.build(&fixture.repository)?.commit, initial);
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Fresh);

    let head = fixture.commit(&fixture.repository, "Advance the reftable branch")?;
    assert_ne!(head, initial);
    let updated = fixture.build(&fixture.repository)?;
    assert_eq!(updated.state, BuildState::Dirty);
    assert_eq!(updated.commit, head);

    fixture.git(&fixture.repository, &["tag", "v0.1.0"])?;
    let tagged = fixture.build(&fixture.repository)?;
    assert_eq!(tagged.state, BuildState::Dirty);
    assert_eq!(tagged.commit, head);
    assert_eq!(tagged.tag, "v0.1.0");
    assert_eq!(tagged.distance, "0");
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Fresh);

    fixture.git(&fixture.repository, &["branch", "previous", &initial])?;
    fixture.git(&fixture.repository, &["switch", "--quiet", "previous"])?;
    let switched = fixture.build(&fixture.repository)?;
    assert_eq!(switched.state, BuildState::Dirty);
    assert_eq!(switched.commit, initial);
    assert_eq!(fixture.build(&fixture.repository)?.state, BuildState::Fresh);
    Ok(())
}

#[test]
fn reftable_worktrees_track_commits_and_detached_heads() -> Result<()> {
    let Some(fixture) = Fixture::new("reftable-worktree", true)? else {
        return Ok(());
    };
    let worktree = fixture.add_worktree("worktree", Some("linked"))?;
    let initial = fixture.head(&worktree)?;
    assert_eq!(fixture.build(&worktree)?.commit, initial);
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);

    let head = fixture.commit(&worktree, "Advance the worktree reftable branch")?;
    assert_ne!(head, initial);
    let updated = fixture.build(&worktree)?;
    assert_eq!(updated.state, BuildState::Dirty);
    assert_eq!(updated.commit, head);

    fixture.git(&worktree, &["switch", "--quiet", "--detach", &initial])?;
    let detached = fixture.build(&worktree)?;
    assert_eq!(detached.state, BuildState::Dirty);
    assert_eq!(detached.commit, initial);
    assert_eq!(fixture.build(&worktree)?.state, BuildState::Fresh);
    Ok(())
}
