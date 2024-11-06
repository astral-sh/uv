use std::sync::Arc;

use uv_pep508::{MarkerEnvironment, MarkerTree};
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::requires_python::RequiresPythonRange;
use crate::resolver::ForkState;
use crate::PythonRequirement;

/// Represents one or more marker environments for a resolution.
///
/// Dependencies outside of the marker environments represented by this value
/// are ignored for that particular resolution.
///
/// In normal "pip"-style resolution, one resolver environment corresponds to
/// precisely one marker environment. In universal resolution, multiple marker
/// environments may be specified via a PEP 508 marker expression. In either
/// case, as mentioned above, dependencies not in these marker environments are
/// ignored for the corresponding resolution.
///
/// Callers must provide this to the resolver to indicate, broadly, what kind
/// of resolution it will produce. Generally speaking, callers should provide
/// a specific marker environment for `uv pip`-style resolutions and ask for a
/// universal resolution for uv's project based commands like `uv lock`.
///
/// Callers can rely on this type being reasonably cheap to clone.
///
/// # Internals
///
/// Inside the resolver, when doing a universal resolution, it may create
/// many "forking" states to deal with the fact that there may be multiple
/// incompatible dependency specifications. Specifically, in the Python world,
/// the main constraint is that for any one *specific* marker environment,
/// there must be only one version of a package in a corresponding resolution.
/// But when doing a universal resolution, we want to support many marker
/// environments, and in this context, the "universal" resolution may contain
/// multiple versions of the same package. This is allowed so long as, for
/// any marker environment supported by this resolution, an installation will
/// select at most one version of any given package.
///
/// During resolution, a `ResolverEnvironment` is attached to each internal
/// fork. For non-universal or "specific" resolution, there is only ever one
/// fork because a `ResolverEnvironment` corresponds to one and exactly one
/// marker environment. For universal resolution, the resolver may choose
/// to split its execution into multiple branches. Each of those branches
/// (also called "forks" or "splits") will get its own marker expression that
/// represents a set of marker environments that is guaranteed to be disjoint
/// with the marker environments described by the marker expressions of all
/// other branches.
///
/// Whether it's universal resolution or not, and whether it's one of many
/// forks or one fork, this type represents the set of possible dependency
/// specifications allowed in the resolution produced by a single fork.
///
/// An exception to this is `requires-python`. That is handled separately and
/// explicitly by the resolver. (Perhaps a future refactor can incorporate
/// `requires-python` into this type as well, but it's not totally clear at
/// time of writing if that's a good idea or not.)
#[derive(Clone, Debug)]
pub struct ResolverEnvironment {
    kind: Kind,
}

/// The specific kind of resolver environment.
///
/// Note that it is explicitly intended that this type remain unexported from
/// this module. The motivation for this design is to discourage repeated case
/// analysis on this type, and instead try to encapsulate the case analysis via
/// higher level routines on `ResolverEnvironment` itself. (This goal may prove
/// intractable, so don't treat it like gospel.)
#[derive(Clone, Debug)]
enum Kind {
    /// We're solving for one specific marker environment only.
    ///
    /// Generally, this is what's done for `uv pip`. For the project based
    /// commands, like `uv lock`, we do universal resolution.
    Specific {
        /// The marker environment being resolved for.
        ///
        /// Any dependency specification that isn't satisfied by this marker
        /// environment is ignored.
        marker_env: ResolverMarkerEnvironment,
    },
    /// We're solving for all possible marker environments.
    Universal {
        /// The initial set of "fork preferences." These will come from the
        /// lock file when available, or the list of supported environments
        /// explicitly written into the `pyproject.toml`.
        ///
        /// Note that this may be empty, which means resolution should begin
        /// with no forks. Or equivalently, a single fork whose marker
        /// expression matches all marker environments.
        initial_forks: Arc<[MarkerTree]>,
        /// The markers associated with this resolver fork.
        markers: MarkerTree,
    },
}

impl ResolverEnvironment {
    /// Create a resolver environment that is fixed to one and only one marker
    /// environment.
    ///
    /// This enables `uv pip`-style resolutions. That is, the resolution
    /// returned is only guaranteed to be installable for this specific marker
    /// environment.
    pub fn specific(marker_env: ResolverMarkerEnvironment) -> ResolverEnvironment {
        let kind = Kind::Specific { marker_env };
        ResolverEnvironment { kind }
    }

    /// Create a resolver environment for producing a multi-platform
    /// resolution.
    ///
    /// The set of marker expressions given corresponds to an initial
    /// seeded set of resolver branches. This might come from a lock file
    /// corresponding to the set of forks produced by a previous resolution, or
    /// it might come from a human crafted set of marker expressions.
    ///
    /// The "normal" case is that the initial forks are empty. When empty,
    /// resolution will create forks as needed to deal with potentially
    /// conflicting dependency specifications across distinct marker
    /// environments.
    ///
    /// The order of the initial forks is significant, although we don't
    /// guarantee any specific treatment (similar to, at time of writing, how
    /// the order of dependencies specified is also significant but has no
    /// specific guarantees around it). Changing the ordering can help when our
    /// custom fork prioritization fails.
    pub fn universal(initial_forks: Vec<MarkerTree>) -> ResolverEnvironment {
        let kind = Kind::Universal {
            initial_forks: initial_forks.into(),
            markers: MarkerTree::TRUE,
        };
        ResolverEnvironment { kind }
    }

    /// Returns the marker environment corresponding to this resolver
    /// environment.
    ///
    /// This only returns a marker environment when resolving for a specific
    /// marker environment. i.e., A non-universal or "pip"-style resolution.
    pub fn marker_environment(&self) -> Option<&MarkerEnvironment> {
        match self.kind {
            Kind::Specific { ref marker_env } => Some(marker_env),
            Kind::Universal { .. } => None,
        }
    }

    /// Returns `false` only when this environment is a fork and it is disjoint
    /// with the given marker.
    pub(crate) fn included(&self, marker: &MarkerTree) -> bool {
        match self.kind {
            Kind::Specific { .. } => true,
            Kind::Universal { ref markers, .. } => !markers.is_disjoint(marker),
        }
    }

    /// Narrow this environment given the forking markers.
    ///
    /// This should be used when generating forking states in the resolver. In
    /// effect, this "forks" this environment (which itself may be a fork) by
    /// intersecting it with the markers given.
    ///
    /// This may return `None` when the marker intersection results in a marker
    /// that can never be true for the given Python requirement. In this case,
    /// the corresponding fork should be dropped.
    ///
    /// # Panics
    ///
    /// This panics if the resolver environment corresponds to one and only one
    /// specific marker environment. i.e., "pip"-style resolution.
    pub(crate) fn narrow_environment(
        &self,
        python_requirement: &PythonRequirement,
        rhs: &MarkerTree,
    ) -> Option<ResolverEnvironment> {
        match self.kind {
            Kind::Specific { .. } => {
                unreachable!("environment narrowing only happens in universal resolution")
            }
            Kind::Universal {
                ref initial_forks,
                markers: ref lhs,
            } => {
                let mut lhs = lhs.clone();
                lhs.and(rhs.clone());
                let python_marker = python_requirement.to_marker_tree();
                // If the new combined marker is disjoint with the given
                // Python requirement, then this fork shouldn't exist.
                if lhs.is_disjoint(&python_marker) {
                    tracing::debug!(
                        "Skipping split {lhs:?} \
                         because of Python requirement {python_marker:?}",
                    );
                    return None;
                }
                let kind = Kind::Universal {
                    initial_forks: initial_forks.clone(),
                    markers: lhs,
                };

                Some(ResolverEnvironment { kind })
            }
        }
    }

    /// Create an initial set of forked states based on this resolver
    /// environment configuration.
    ///
    /// In the "clean" universal case, this just returns a singleton `Vec` with
    /// the given fork state. But when the resolver is configured to start
    /// with an initial set of forked resolver states (e.g., those present in
    /// a lock file), then this creates the initial set of forks from that
    /// configuration.
    pub(crate) fn initial_forked_states(&self, init: ForkState) -> Vec<ForkState> {
        let Kind::Universal {
            ref initial_forks, ..
        } = self.kind
        else {
            return vec![init];
        };
        if initial_forks.is_empty() {
            return vec![init];
        }
        initial_forks
            .iter()
            .rev()
            .filter_map(|initial_fork| init.clone().with_env(initial_fork))
            .collect()
    }

    /// Narrow the [`PythonRequirement`] if this resolver environment
    /// corresponds to a more constraining fork.
    ///
    /// For example, if this is a fork where `python_version >= '3.12'` is
    /// always true, and if the given python requirement (perhaps derived from
    /// `Requires-Python`) is `>=3.10`, then this will "narrow" the requirement
    /// to `>=3.12`, corresponding to the marker expression describing this
    /// fork.
    ///
    /// If this environment is not a fork, then this returns `None`.
    pub(crate) fn narrow_python_requirement(
        &self,
        python_requirement: &PythonRequirement,
    ) -> Option<PythonRequirement> {
        python_requirement.narrow(&self.requires_python_range()?)
    }

    /// Returns a message formatted for end users representing a fork in the
    /// resolver.
    ///
    /// If this resolver environment does not correspond to a particular fork,
    /// then `None` is returned.
    ///
    /// This is useful in contexts where one wants to display a message
    /// relating to a particular fork, but either no message or an entirely
    /// different message when this isn't a fork.
    pub(crate) fn end_user_fork_display(&self) -> Option<impl std::fmt::Display + '_> {
        match self.kind {
            Kind::Specific { .. } => None,
            Kind::Universal { ref markers, .. } => {
                if markers.is_true() {
                    None
                } else {
                    Some(format!("split ({markers:?})"))
                }
            }
        }
    }

    /// Returns the marker expression corresponding to the fork that is
    /// represented by this resolver environment.
    ///
    /// This returns `None` when this does not correspond to a fork.
    pub(crate) fn try_markers(&self) -> Option<&MarkerTree> {
        match self.kind {
            Kind::Specific { .. } => None,
            Kind::Universal { ref markers, .. } => {
                if markers.is_true() {
                    None
                } else {
                    Some(markers)
                }
            }
        }
    }

    /// Returns a requires-python version range derived from the marker
    /// expression describing this resolver environment.
    ///
    /// When this isn't a fork, then there is nothing to constrain and thus
    /// `None` is returned.
    fn requires_python_range(&self) -> Option<RequiresPythonRange> {
        crate::marker::requires_python(self.try_markers()?)
    }
}

/// A user visible representation of a resolver environment.
///
/// This is most useful in error and log messages.
impl std::fmt::Display for ResolverEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.kind {
            Kind::Specific { .. } => write!(f, "marker environment"),
            Kind::Universal { ref markers, .. } => {
                if markers.is_true() {
                    write!(f, "all marker environments")
                } else {
                    write!(f, "split `{markers:?}`")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;
    use std::sync::LazyLock;

    use uv_pep440::Version;
    use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder};

    use crate::requires_python::{LowerBound, RequiresPython, RequiresPythonRange, UpperBound};

    use super::*;

    /// A dummy marker environment used in tests below.
    ///
    /// It doesn't matter too much what we use here, and indeed, this one was
    /// copied from our uv microbenchmarks.
    static MARKER_ENV: LazyLock<MarkerEnvironment> = LazyLock::new(|| {
        MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "cpython",
            implementation_version: "3.11.5",
            os_name: "posix",
            platform_machine: "arm64",
            platform_python_implementation: "CPython",
            platform_release: "21.6.0",
            platform_system: "Darwin",
            platform_version: "Darwin Kernel Version 21.6.0: Mon Aug 22 20:19:52 PDT 2022; root:xnu-8020.140.49~2/RELEASE_ARM64_T6000",
            python_full_version: "3.11.5",
            python_version: "3.11",
            sys_platform: "darwin",
        }).unwrap()
    });

    fn requires_python_lower(lower_version_bound: &str) -> RequiresPython {
        RequiresPython::greater_than_equal_version(&version(lower_version_bound))
    }

    fn requires_python_range_lower(lower_version_bound: &str) -> RequiresPythonRange {
        let lower = LowerBound::new(Bound::Included(version(lower_version_bound)));
        RequiresPythonRange::new(lower, UpperBound::default())
    }

    fn marker(marker: &str) -> MarkerTree {
        marker
            .parse::<MarkerTree>()
            .expect("valid pep508 marker expression")
    }

    fn version(v: &str) -> Version {
        v.parse().expect("valid pep440 version string")
    }

    fn python_requirement(python_version_greater_than_equal: &str) -> PythonRequirement {
        let requires_python = requires_python_lower(python_version_greater_than_equal);
        PythonRequirement::from_marker_environment(&MARKER_ENV, requires_python)
    }

    /// Tests that narrowing a Python requirement when resolving for a
    /// specific marker environment never produces a more constrained Python
    /// requirement.
    #[test]
    fn narrow_python_requirement_specific() {
        let resolver_marker_env = ResolverMarkerEnvironment::from(MARKER_ENV.clone());
        let resolver_env = ResolverEnvironment::specific(resolver_marker_env);

        let pyreq = python_requirement("3.10");
        assert_eq!(resolver_env.narrow_python_requirement(&pyreq), None);

        let pyreq = python_requirement("3.11");
        assert_eq!(resolver_env.narrow_python_requirement(&pyreq), None);

        let pyreq = python_requirement("3.12");
        assert_eq!(resolver_env.narrow_python_requirement(&pyreq), None);
    }

    /// Tests that narrowing a Python requirement during a universal resolution
    /// *without* any forks will never produce a more constrained Python
    /// requirement.
    #[test]
    fn narrow_python_requirement_universal() {
        let resolver_env = ResolverEnvironment::universal(vec![]);

        let pyreq = python_requirement("3.10");
        assert_eq!(resolver_env.narrow_python_requirement(&pyreq), None);

        let pyreq = python_requirement("3.11");
        assert_eq!(resolver_env.narrow_python_requirement(&pyreq), None);

        let pyreq = python_requirement("3.12");
        assert_eq!(resolver_env.narrow_python_requirement(&pyreq), None);
    }

    /// Inside a fork whose marker's Python requirement is equal
    /// to our Requires-Python means that narrowing produces a
    /// result, but is unchanged from what we started with.
    #[test]
    fn narrow_python_requirement_forking_no_op() {
        let pyreq = python_requirement("3.10");
        let resolver_env = ResolverEnvironment::universal(vec![])
            .narrow_environment(&pyreq, &marker("python_version >= '3.10'"))
            .unwrap();
        assert_eq!(
            resolver_env.narrow_python_requirement(&pyreq),
            Some(python_requirement("3.10")),
        );
    }

    /// In this test, we narrow a more relaxed requirement compared to the
    /// marker for the current fork. This in turn results in a stricter
    /// requirement corresponding to what's specified in the fork.
    #[test]
    fn narrow_python_requirement_forking_stricter() {
        let pyreq = python_requirement("3.10");
        let resolver_env = ResolverEnvironment::universal(vec![])
            .narrow_environment(&pyreq, &marker("python_version >= '3.11'"))
            .unwrap();
        let expected = {
            let range = requires_python_range_lower("3.11");
            let requires_python = requires_python_lower("3.10").narrow(&range).unwrap();
            PythonRequirement::from_marker_environment(&MARKER_ENV, requires_python)
        };
        assert_eq!(
            resolver_env.narrow_python_requirement(&pyreq),
            Some(expected)
        );
    }

    /// In this test, we narrow a stricter requirement compared to the marker
    /// for the current fork. This in turn results in a requirement that
    /// remains unchanged.
    #[test]
    fn narrow_python_requirement_forking_relaxed() {
        let pyreq = python_requirement("3.11");
        let resolver_env = ResolverEnvironment::universal(vec![])
            .narrow_environment(&pyreq, &marker("python_version >= '3.10'"))
            .unwrap();
        assert_eq!(
            resolver_env.narrow_python_requirement(&pyreq),
            Some(python_requirement("3.11")),
        );
    }
}
