//! Canonical marker trees over compatible platform assignments.
//!
//! The exclusion table describes combinations of marker values that cannot occur together.
//! On those assignments, choose a consistent value so that markers that agree on every
//! compatible platform share the same node ID.

use std::cmp::Ordering;

use arcstr::literal;
use rustc_hash::FxHashMap;

use super::{InternerGuard, Node, NodeId, Variable};
use crate::MarkerValueString::{
    ImplementationName, OsName, PlatformPythonImplementation, PlatformSystem, SysPlatform,
};
use crate::marker::lowering::CanonicalMarkerValueString as Key;
use crate::{MarkerExpression, MarkerOperator};

#[derive(Default)]
pub(super) struct Cache {
    /// The disjunction of known incompatible marker pairs.
    exclusions: Option<NodeId>,
    /// Canonical representatives of nodes that test multiple platform keys.
    canonical: FxHashMap<NodeId, NodeId>,
}

// Bit zero in a NodeId is reserved for complemented edges.
const OS_KEY: usize = 1 << 1;
const IMPLEMENTATION_KEY: usize = 1 << 2;
const MIXED_KEYS: usize = OS_KEY | IMPLEMENTATION_KEY;
pub(super) const FLAGS: usize = MIXED_KEYS;

impl NodeId {
    fn tests_platform_key(self) -> bool {
        self.0 & FLAGS != 0
    }

    fn mixes_platform_keys(self) -> bool {
        self.0 & FLAGS == MIXED_KEYS
    }
}

/// Record the first group of related keys, or `MIXED_KEYS` if a path tests
/// two keys from the same group. Each group is contiguous in decision order,
/// so only the first group can interact with an ancestor.
pub(super) fn node_flags(node: &Node) -> usize {
    let Variable::String(key) = &node.var else {
        return 0;
    };
    let group = match key {
        Key::OsName | Key::SysPlatform | Key::PlatformSystem => OS_KEY,
        Key::ImplementationName | Key::PlatformPythonImplementation => IMPLEMENTATION_KEY,
        // This key comes between the groups, so propagate its children's flags.
        Key::PlatformMachine => {
            return node
                .children
                .nodes()
                .fold(0, |flags, child| flags | child.0)
                & FLAGS;
        }
        Key::PlatformRelease | Key::PlatformVersion => return 0,
    };
    // A child flagged as mixed already has both bits set.
    if node.children.nodes().any(|child| child.0 & group != 0) {
        MIXED_KEYS
    } else {
        group
    }
}

impl InternerGuard<'_> {
    /// Returns `true` if there is no environment in which both marker trees can apply,
    /// i.e. their conjunction is always `false`.
    pub(in crate::marker) fn is_disjoint_nontrivial(&mut self, xi: NodeId, yi: NodeId) -> bool {
        debug_assert!(
            xi.is_disjoint_trivial(yi).is_none(),
            "`is_disjoint_nontrivial` requires non-trivial disjointness"
        );

        // The first groups can differ while the trees share a later group.
        if xi.tests_platform_key() && yi.tests_platform_key() {
            let intersection = self.and_nontrivial(xi, yi);
            return self.canonicalize(intersection).is_false();
        }

        self.disjoint_edges(xi, yi)
    }

    /// Choose the same diagram for functions that agree on all compatible platform
    /// assignments. The values on incompatible assignments are don't-cares.
    #[inline]
    pub(in crate::marker) fn canonicalize(&mut self, marker: NodeId) -> NodeId {
        if marker.mixes_platform_keys() {
            self.canonicalize_mixed_platforms(marker)
        } else {
            marker
        }
    }

    #[inline(never)]
    fn canonicalize_mixed_platforms(&mut self, marker: NodeId) -> NodeId {
        if let Some(&result) = self.state.platform.canonical.get(&marker) {
            return result;
        }
        let platforms = self.exclusions().not();
        let result = self.platform_cofactor(marker, platforms, &mut FxHashMap::default());

        // Children are also exposed as standalone marker trees. Complete their
        // platform assignments without assuming the value of their parent.
        let result = if result.mixes_platform_keys() {
            let node = self.shared.node(result);
            let children = node.children.map(result, |child| self.canonicalize(child));
            self.create_node(node.var.clone(), children)
        } else {
            result
        };
        self.state.platform.canonical.insert(marker, result);
        self.state
            .platform
            .canonical
            .insert(marker.not(), result.not());
        result
    }

    /// Extend a marker from the compatible platforms to the whole decision domain.
    /// The first range of each key permits every assignment to the remaining keys
    /// that any other range permits, unless an earlier key already determines this
    /// key's value. Use the first range's child as the default wherever another
    /// range agrees with it on all compatible platforms.
    fn platform_cofactor(
        &mut self,
        value: NodeId,
        domain: NodeId,
        cache: &mut FxHashMap<(NodeId, NodeId), NodeId>,
    ) -> NodeId {
        if domain.is_true() || value.is_true() || value.is_false() {
            return value;
        }
        // Each recursion visits a reachable domain. Excluded edges take the default branch below.
        debug_assert!(!domain.is_false());
        if !value.tests_platform_key() {
            return value;
        }
        let node = self.shared.node(value);
        if let Some(&result) = cache.get(&(value, domain)) {
            return result;
        }
        let platform = self.shared.node(domain);
        let default_domain = platform.children.nodes().next().unwrap().negate(domain);
        if default_domain.is_false() {
            // A known implementation name fixes the final key to a single value.
            // Restricting the tree to that value eliminates the decision entirely.
            debug_assert_eq!(
                platform.var,
                Variable::String(Key::PlatformPythonImplementation)
            );
            return self.restrict_cached(value, domain, cache);
        }
        let result = match node.var.cmp(&platform.var) {
            Ordering::Less => {
                let children = node
                    .children
                    .map(value, |child| self.platform_cofactor(child, domain, cache));
                self.create_node(node.var.clone(), children)
            }
            Ordering::Greater => self.platform_cofactor(value, default_domain, cache),
            Ordering::Equal => {
                let default_value = node.children.nodes().next().unwrap().negate(value);
                let default = self.platform_cofactor(default_value, default_domain, cache);
                let children =
                    node.children
                        .apply(value, &platform.children, domain, |child, enabled| {
                            if enabled.is_false() || child == default_value {
                                return default;
                            }
                            let result = self.platform_cofactor(child, enabled, cache);
                            if enabled != default_domain
                                && result == self.platform_cofactor(default, enabled, cache)
                            {
                                default
                            } else {
                                result
                            }
                        });
                self.create_node(node.var.clone(), children)
            }
        };
        cache.insert((value, domain), result);
        result
    }

    /// The disjunction of known incompatible conditions.
    ///
    /// For example, while the marker specification and grammar do not _forbid_ it, we know that
    /// both `sys_platform == 'win32'` and `platform_system == 'Darwin'` will never be true at the
    /// same time.
    ///
    /// This method thus encodes assumptions about the environment that are not guaranteed by the
    /// PEP 508 specification alone.
    fn exclusions(&mut self) -> NodeId {
        if let Some(exclusions) = self.state.platform.exclusions {
            return exclusions;
        }
        let mut equals = |key, value| {
            self.expression(MarkerExpression::String {
                key,
                operator: MarkerOperator::Equal,
                value,
            })
        };
        let os_name_nt = equals(OsName, literal!("nt"));
        let os_name_posix = equals(OsName, literal!("posix"));
        let sys_platform_linux = equals(SysPlatform, literal!("linux"));
        let sys_platform_darwin = equals(SysPlatform, literal!("darwin"));
        let sys_platform_ios = equals(SysPlatform, literal!("ios"));
        let sys_platform_win32 = equals(SysPlatform, literal!("win32"));
        let platform_system_freebsd = equals(PlatformSystem, literal!("FreeBSD"));
        let platform_system_netbsd = equals(PlatformSystem, literal!("NetBSD"));
        let platform_system_openbsd = equals(PlatformSystem, literal!("OpenBSD"));
        let platform_system_sunos = equals(PlatformSystem, literal!("SunOS"));
        let platform_system_ios = equals(PlatformSystem, literal!("iOS"));
        let platform_system_ipados = equals(PlatformSystem, literal!("iPadOS"));
        let sys_platform_aix = equals(SysPlatform, literal!("aix"));
        let sys_platform_android = equals(SysPlatform, literal!("android"));
        let sys_platform_emscripten = equals(SysPlatform, literal!("emscripten"));
        let sys_platform_cygwin = equals(SysPlatform, literal!("cygwin"));
        let sys_platform_wasi = equals(SysPlatform, literal!("wasi"));

        // Pairs of `os_name` and `sys_platform` that are known to be incompatible.
        //
        // For example: `os_name == 'nt' and sys_platform == 'darwin'`
        let mut pairs = vec![
            (os_name_nt, sys_platform_linux),
            (os_name_nt, sys_platform_darwin),
            (os_name_nt, sys_platform_ios),
            (os_name_posix, sys_platform_win32),
        ];

        // A known `sys.implementation.name` determines `platform.python_implementation()`.
        // The converse is ambiguous. For example, Pyston also reports `CPython`.
        for (name, implementation) in [
            (literal!("cpython"), literal!("CPython")),
            (literal!("pypy"), literal!("PyPy")),
            (literal!("graalpy"), literal!("GraalVM")),
            (literal!("pyston"), literal!("CPython")),
        ] {
            pairs.push((
                equals(ImplementationName, name),
                equals(PlatformPythonImplementation, implementation).not(),
            ));
        }

        // Pairs of `platform_system` and `sys_platform` that are known to be incompatible.
        //
        // For example: `platform_system == 'FreeBSD' and sys_platform == 'aix'`
        for platform_system in [
            platform_system_freebsd,
            platform_system_netbsd,
            platform_system_openbsd,
            platform_system_sunos,
            platform_system_ios,
            platform_system_ipados,
        ] {
            for sys_platform in [
                sys_platform_aix,
                sys_platform_android,
                sys_platform_emscripten,
                sys_platform_ios,
                sys_platform_linux,
                sys_platform_darwin,
                sys_platform_win32,
                sys_platform_cygwin,
                sys_platform_wasi,
            ] {
                // Some of the above pairs are actually compatible.
                if sys_platform == sys_platform_ios
                    && (platform_system == platform_system_ios
                        || platform_system == platform_system_ipados)
                {
                    continue;
                }
                pairs.push((platform_system, sys_platform));
            }
        }

        let mut tree = NodeId::FALSE;
        for (a, b) in pairs {
            let a_and_b = self.and(a, b);
            tree = self.or(tree, a_and_b);
        }

        self.state.platform.exclusions = Some(tree);
        tree
    }
}
