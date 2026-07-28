use uv_configuration::ResolutionPolicy;
use uv_pep440::Version;

use crate::resolver::ResolverEnvironment;

impl ResolverEnvironment {
    /// Filter a list of (python_full_version, env) tuples by an optional ResolutionPolicy.
    pub fn filter_by_python_resolution_policy(
        &self,
        mut envs: Vec<(Version, Self)>,
        policy: Option<ResolutionPolicy>,
    ) -> Vec<(Version, Self)> {
        fn allows(version: &Version, policy: &ResolutionPolicy) -> bool {
            match policy {
                ResolutionPolicy::Only(v) => v.parse::<Version>().map(|pv| &pv == version).unwrap_or(false),
                ResolutionPolicy::Range(spec) => spec.parse::<uv_pep440::VersionSpecifiers>().map(|s| s.contains(version)).unwrap_or(true),
            }
        }
        if let Some(policy) = policy {
            envs.retain(|(v, _)| allows(v, &policy));
        }
        envs
    }
}
