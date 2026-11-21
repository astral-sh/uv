use std::sync::LazyLock;

use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};
use uv_platform_tags::LanguageTag;

/// The constraint on `implementation_name` implied by a wheel's [`LanguageTag`].
pub fn wheel_implementation_marker(tag: LanguageTag) -> MarkerTree {
    fn marker(name: &str) -> MarkerTree {
        MarkerTree::expression(MarkerExpression::String {
            key: MarkerValueString::ImplementationName,
            operator: MarkerOperator::Equal,
            value: name.into(),
        })
    }

    static CPYTHON: LazyLock<MarkerTree> = LazyLock::new(|| marker("cpython"));
    static PYPY: LazyLock<MarkerTree> = LazyLock::new(|| marker("pypy"));
    static GRAALPY: LazyLock<MarkerTree> = LazyLock::new(|| marker("graalpy"));
    static PYSTON: LazyLock<MarkerTree> = LazyLock::new(|| marker("pyston"));

    match tag {
        LanguageTag::None | LanguageTag::Python { .. } => MarkerTree::TRUE,
        LanguageTag::CPython { .. } | LanguageTag::CPythonMajor { .. } => *CPYTHON,
        LanguageTag::PyPy { .. } => *PYPY,
        LanguageTag::GraalPy { .. } => *GRAALPY,
        LanguageTag::Pyston { .. } => *PYSTON,
    }
}
