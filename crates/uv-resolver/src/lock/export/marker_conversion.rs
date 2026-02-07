use uv_normalize::PackageName;
use uv_pep508::{MarkerExpression, MarkerTree};

use crate::universal_marker::encoded_extra_to_pep751;

pub(crate) fn to_pep751_marker(marker: MarkerTree, root_name: Option<&PackageName>) -> MarkerTree {
    if marker.is_true() {
        return MarkerTree::TRUE;
    }
    if marker.is_false() {
        return MarkerTree::FALSE;
    }

    let dnf = marker.to_dnf();
    if dnf.is_empty() {
        return MarkerTree::TRUE;
    }

    let mut result = MarkerTree::FALSE;
    for clause in dnf {
        let mut clause_result = MarkerTree::TRUE;
        for expr in clause {
            if let Some(converted_expr) = convert_expression(expr, root_name) {
                clause_result.and(MarkerTree::expression(converted_expr));
            }
        }
        result.or(clause_result);
    }

    result
}

fn convert_expression(
    expr: MarkerExpression,
    root_name: Option<&PackageName>,
) -> Option<MarkerExpression> {
    match &expr {
        MarkerExpression::Extra { name, operator } => {
            if let Some(extra_name) = name.as_extra() {
                if let Some(pep751_expr) = encoded_extra_to_pep751(extra_name, operator, root_name)
                {
                    Some(pep751_expr)
                } else if is_encoded_extra(extra_name.as_ref()) {
                    // Internal fork markers (extra-*, group-*, project-*) are not meant for PEP 751 export.
                    None
                } else {
                    Some(expr)
                }
            } else {
                Some(expr)
            }
        }
        _ => Some(expr),
    }
}

fn is_encoded_extra(extra_name: &str) -> bool {
    extra_name.starts_with("extra-")
        || extra_name.starts_with("group-")
        || extra_name.starts_with("project-")
}
