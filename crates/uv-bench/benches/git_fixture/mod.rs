use std::path::Path;

use anyhow::{Context, Result, anyhow};
use uv_git::GIT;
use uv_redacted::SafeUrl;

pub(super) struct GitFixture {
    pub(super) name: &'static str,
    repository: &'static str,
    pub(super) commit: &'static str,
    pub(super) reference: &'static str,
    pub(super) revisions: &'static [&'static str],
}

impl GitFixture {
    /// Fetch the pinned history before entering a benchmark's timed loop.
    pub(super) fn prepare(&self) -> Result<SafeUrl> {
        let cache = std::path::absolute(Path::new("../../target/bench-git"))?;
        let directory = cache.join(format!("{}-{}.git", self.name, self.commit));
        if !directory.is_dir() {
            fs_err::create_dir_all(&cache)?;
            let staging = tempfile::tempdir_in(&cache)?;
            let repository = staging.path();
            git(
                repository,
                &["-c", "init.templateDir=", "init", "--bare", "--quiet"],
            )?;

            let reference = format!("+{}:{}", self.commit, self.reference);
            git(
                repository,
                &[
                    "fetch",
                    "--no-auto-maintenance",
                    "--quiet",
                    "--no-tags",
                    self.repository,
                    &reference,
                ],
            )?;
            git(
                repository,
                &["update-ref", "--no-deref", "HEAD", self.commit],
            )?;
            // A delta represents one object as changes to a similar object. Git splits the
            // search for similar objects across threads, so scheduling can change which base
            // objects it compares and hence the pack used by the benchmark. Use one thread for
            // the delta search and -F to recompute compressed objects and deltas from scratch.
            git(repository, &["-c", "pack.threads=1", "repack", "-adqF"])?;

            // Local fetches must support the same object filtering as the upstream host.
            git(repository, &["config", "uploadpack.allowFilter", "true"])?;
            git(
                repository,
                &["config", "uploadpack.allowAnySHA1InWant", "true"],
            )?;
            fs_err::rename(repository, &directory)?;
        }
        SafeUrl::from_file_path(directory).map_err(|()| anyhow!("Invalid Git repository URL"))
    }
}

fn git(directory: &Path, arguments: &[&str]) -> Result<()> {
    GIT.as_ref()
        .cloned()?
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .exec_with_output()
        .with_context(|| format!("Failed to run Git in {}", directory.display()))?;
    Ok(())
}

pub(super) const GIT_FIXTURES: &[GitFixture] = &[
    GitFixture {
        name: "sampleproject",
        repository: "https://github.com/pypa/sampleproject",
        commit: "621e4974ca25ce531773def586ba3ed8e736b3fc",
        reference: "refs/heads/main",
        revisions: &[],
    },
    GitFixture {
        name: "flask",
        repository: "https://github.com/pallets/flask",
        commit: "2c1b30d0503cfb064f1cb252e6614a06915a362a",
        reference: "refs/tags/3.1.2",
        revisions: &[],
    },
    GitFixture {
        name: "django",
        repository: "https://github.com/django/django",
        commit: "75c4403f07b8ad25893f7832dbe8fc6814b53b2d",
        reference: "refs/tags/5.2.6",
        revisions: &[
            "759921c8e9ad151932fc913ab429fef0a6112ef8", // 5.2a1
            "9b7944896d9e96c60fe609308d152c358effdd39", // 5.2b1
            "3d3bd04cba71c57298b2b106b5856b1f44a0a7cc", // 5.2rc1
            "9e7cc2b628fe8fd3895986af9b7fc9525034c1b0", // 5.2
            "bc833e8883db4a333a6485d91637b78c85e2b13b", // 5.2.1
            "9e2fe65967e3ad4da276d156c466fe1cf682ca7c", // 5.2.2
            "15883bc669303242742a81f06958175dddbb66de", // 5.2.3
            "c941d0deec0ea08a30670be0fac879f2372f071b", // 5.2.4
            "a3b1107a4955bdd994908efb4c6e1d03c281e69f", // 5.2.5
            "75c4403f07b8ad25893f7832dbe8fc6814b53b2d", // 5.2.6
        ],
    },
];
