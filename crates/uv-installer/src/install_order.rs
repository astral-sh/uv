use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Result, bail};
use rayon::Scope;
use rustc_hash::FxHashMap;

use uv_distribution_types::{CachedDist, Name, Resolution};

/// The dependency groups for a set of prepared wheels.
///
/// Wheel indices refer to the input passed to [`Self::new`]. Groups retain dependencies through
/// distributions that are already installed or were filtered out of the resolution.
pub(crate) struct InstallOrder {
    groups: Vec<InstallGroup>,
}

struct InstallGroup {
    wheels: Vec<usize>,
    dependencies: usize,
    dependents: Vec<usize>,
}

impl InstallOrder {
    pub(crate) fn new(wheels: &[CachedDist], resolution: &Resolution) -> Result<Self> {
        if wheels.len() <= 1 {
            return Ok(Self {
                groups: vec![InstallGroup {
                    wheels: (0..wheels.len()).collect(),
                    dependencies: 0,
                    dependents: vec![],
                }],
            });
        }

        let mut wheel_by_name = FxHashMap::default();
        for (index, wheel) in wheels.iter().enumerate() {
            if wheel_by_name.insert(wheel.name(), index).is_some() {
                bail!("More than one wheel was prepared for `{}`", wheel.name());
            }
        }

        let mut groups: Vec<InstallGroup> = Vec::new();
        for group in resolution.dependency_groups() {
            let group_index = groups.len();
            for dependency in &group.dependencies {
                groups[*dependency].dependents.push(group_index);
            }
            groups.push(InstallGroup {
                wheels: group
                    .distributions
                    .into_iter()
                    .filter_map(|dist| wheel_by_name.remove(dist.name()))
                    .collect(),
                dependencies: group.dependencies.len(),
                dependents: vec![],
            });
        }

        if let Some(name) = wheel_by_name.keys().min() {
            bail!("Prepared wheel for `{name}` is missing from the resolution");
        }

        Ok(Self { groups })
    }

    /// Run independent groups concurrently, releasing dependents as their dependencies finish.
    pub(crate) fn install<E, F>(&self, install: F) -> Result<(), E>
    where
        E: Send + Sync,
        F: Fn(usize) -> Result<(), E> + Sync,
    {
        if let [group] = self.groups.as_slice() {
            for wheel in &group.wheels {
                install(*wheel)?;
            }
            return Ok(());
        }

        let scheduler = Scheduler {
            groups: &self.groups,
            remaining: self
                .groups
                .iter()
                .map(|group| AtomicUsize::new(group.dependencies))
                .collect(),
            install: &install,
            error: OnceLock::new(),
        };

        rayon::scope(|scope| {
            let scheduler = &scheduler;
            for (index, group) in self.groups.iter().enumerate() {
                if group.dependencies == 0 {
                    scope.spawn(move |scope| scheduler.install_group(scope, index));
                }
            }
        });

        match scheduler.error.into_inner() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

struct Scheduler<'a, F, E> {
    groups: &'a [InstallGroup],
    remaining: Vec<AtomicUsize>,
    install: &'a F,
    error: OnceLock<E>,
}

impl<F, E> Scheduler<'_, F, E>
where
    E: Send + Sync,
    F: Fn(usize) -> Result<(), E> + Sync,
{
    fn install_group<'scope>(&'scope self, scope: &Scope<'scope>, index: usize) {
        for wheel in &self.groups[index].wheels {
            if self.error.get().is_some() {
                return;
            }
            if let Err(error) = (self.install)(*wheel) {
                // Other independent groups may already be running. Retain the first failure and
                // leave this group's dependents unscheduled.
                let _ = self.error.set(error);
                return;
            }
        }

        for dependent in &self.groups[index].dependents {
            if self.remaining[*dependent].fetch_sub(1, Ordering::AcqRel) == 1 {
                scope.spawn(move |scope| self.install_group(scope, *dependent));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use rayon::ThreadPoolBuilder;

    use super::{InstallGroup, InstallOrder};

    #[test]
    fn ready_dependents_do_not_wait_for_unrelated_groups() {
        // Wheel 2 depends on wheel 0. Wheel 1 is unrelated and remains busy until wheel 2 runs.
        let order = InstallOrder {
            groups: vec![
                InstallGroup {
                    wheels: vec![0],
                    dependencies: 0,
                    dependents: vec![2],
                },
                InstallGroup {
                    wheels: vec![1],
                    dependencies: 0,
                    dependents: vec![],
                },
                InstallGroup {
                    wheels: vec![2],
                    dependencies: 1,
                    dependents: vec![],
                },
            ],
        };
        let (sender, receiver) = mpsc::channel();
        let receiver = Mutex::new(receiver);
        let pool = ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("thread pool should be available");

        pool.install(|| {
            order.install(|wheel| {
                if wheel == 1 {
                    receiver
                        .lock()
                        .expect("receiver lock should not be poisoned")
                        .recv_timeout(Duration::from_secs(10))?;
                } else if wheel == 2 {
                    sender.send(()).expect("receiver should be available");
                }
                Ok::<(), mpsc::RecvTimeoutError>(())
            })
        })
        .expect("independent groups should not block each other");
    }

    #[test]
    fn cycle_members_run_in_order() {
        let order = InstallOrder {
            groups: vec![InstallGroup {
                wheels: vec![2, 0, 1],
                dependencies: 0,
                dependents: vec![],
            }],
        };
        let installed = Mutex::new(Vec::new());

        order
            .install(|wheel| {
                installed
                    .lock()
                    .expect("installed lock should not be poisoned")
                    .push(wheel);
                Ok::<(), std::convert::Infallible>(())
            })
            .expect("installation should succeed");

        assert_eq!(
            installed.into_inner().expect("lock should not be poisoned"),
            [2, 0, 1]
        );
    }

    #[test]
    fn failure_stops_the_cycle_and_its_dependents() {
        let order = InstallOrder {
            groups: vec![
                InstallGroup {
                    wheels: vec![0, 1],
                    dependencies: 0,
                    dependents: vec![1],
                },
                InstallGroup {
                    wheels: vec![2],
                    dependencies: 1,
                    dependents: vec![],
                },
            ],
        };
        let attempted = AtomicUsize::new(0);

        let result = order.install(|wheel| {
            attempted.fetch_or(1 << wheel, Ordering::Relaxed);
            Err::<(), _>("failed")
        });

        assert_eq!(result, Err("failed"));
        assert_eq!(attempted.load(Ordering::Relaxed), 1);
    }
}
