//! Fetch and reuse real, pinned Git source trees through the local Git transport.

// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

mod git_fixture;

use std::cell::LazyCell;
use std::env;
use std::hint::black_box;
use std::path::Path;
use std::time::Duration;

use criterion::{
    BatchSize, BenchmarkId, Criterion, SamplingMode, criterion_group, criterion_main,
    measurement::WallTime,
};
use uv_git::{Fetch, GitHttpSettings, GitResolver};
use uv_git_types::{GitLfs, GitOid, GitReference, GitUrl};
use uv_redacted::DisplaySafeUrl;

use git_fixture::GIT_FIXTURES;

fn is_codspeed_simulation() -> bool {
    // CodSpeed reports Simulation as `instrumentation` in current versions.
    matches!(
        env::var("CODSPEED_RUNNER_MODE").as_deref(),
        Ok("instrumentation" | "simulation")
    )
}

fn fetch(
    runtime: &tokio::runtime::Runtime,
    git: &GitUrl,
    cache: &Path,
    settings: GitHttpSettings,
) -> Fetch {
    let fetched = runtime
        .block_on(GitResolver::default().fetch(git, settings, cache.to_path_buf(), None))
        .expect("Failed to fetch pinned Git repository");
    if let Some(commit) = git.precise() {
        assert_eq!(fetched.git().precise(), Some(commit));
    }
    fetched
}

fn git_url(repository: &DisplaySafeUrl, reference: &str, precise: Option<GitOid>) -> GitUrl {
    GitUrl::from_fields(
        repository.clone(),
        GitReference::from_rev(reference.to_owned()),
        precise,
        GitLfs::Disabled,
    )
    .expect("Invalid Git URL")
}

fn git_fetch(criterion: &mut Criterion<WallTime>) {
    if is_codspeed_simulation() {
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");
    let settings = GitHttpSettings::default().with_offline(true);
    let mut group = criterion.benchmark_group("git_fetch");
    group.sampling_mode(SamplingMode::Flat);
    for fixture in GIT_FIXTURES {
        // Criterion only prepares repositories selected by the benchmark filter.
        let repository =
            LazyCell::new(|| fixture.prepare().expect("Failed to prepare Git repository"));
        let commit: GitOid = fixture.commit.parse().expect("Invalid Git commit");
        group.bench_function(
            BenchmarkId::new("git_fetch_cold_precise", fixture.name),
            |bencher| {
                let precise = git_url(&repository, fixture.commit, Some(commit));
                bencher.iter_batched(
                    || tempfile::tempdir().expect("Failed to create Git cache"),
                    |cache| {
                        let fetched = fetch(&runtime, &precise, cache.path(), settings);
                        black_box((cache, fetched))
                    },
                    BatchSize::PerIteration,
                );
            },
        );

        let named_reference = fixture
            .reference
            .trim_start_matches("refs/heads/")
            .trim_start_matches("refs/tags/");
        for (name, reference, locked) in [
            ("git_fetch_warm_precise", fixture.commit, Some(commit)),
            ("git_fetch_warm_commit", fixture.commit, None),
            ("git_fetch_warm_reference", named_reference, None),
        ] {
            group.bench_function(BenchmarkId::new(name, fixture.name), |bencher| {
                let precise = git_url(&repository, fixture.commit, Some(commit));
                let git = git_url(&repository, reference, locked);
                let cache = tempfile::tempdir().expect("Failed to create Git cache");
                fetch(&runtime, &precise, cache.path(), settings);
                bencher.iter(|| black_box(fetch(&runtime, &git, cache.path(), settings)));
            });
        }

        // A long-lived CI cache accumulates checkouts as Git dependencies are upgraded.
        for count in [1, 4, 10] {
            let Some(selected) = fixture.revisions.get(..count) else {
                continue;
            };
            group.bench_function(
                BenchmarkId::new(
                    "git_fetch_revision_history",
                    format!("{}/{count}", fixture.name),
                ),
                |bencher| {
                    let revisions: Vec<_> = selected
                        .iter()
                        .map(|revision| {
                            git_url(
                                &repository,
                                revision,
                                Some(revision.parse().expect("Invalid Git commit")),
                            )
                        })
                        .collect();
                    bencher.iter_batched(
                        || tempfile::tempdir().expect("Failed to create Git cache"),
                        |cache| {
                            for git in &revisions {
                                black_box(fetch(&runtime, git, cache.path(), settings));
                            }
                            black_box(cache)
                        },
                        BatchSize::PerIteration,
                    );
                },
            );
        }
    }
    group.finish();
}

criterion_group! {
    name = repositories;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = git_fetch
}
criterion_main!(repositories);
