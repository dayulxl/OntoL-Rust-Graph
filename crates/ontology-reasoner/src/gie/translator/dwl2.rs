//! # DWL2 翻译器 — OWL2 ClassExpression → QueryPlan

use ontology_storage::mapper::query_plan::QueryPlan;
use crate::dwl2::ast::ClassExpression;

/// 将 ClassExpression 翻译为 QueryPlan 列表。
pub fn translate_class_expression(expr: &ClassExpression) -> Result<Vec<QueryPlan>, String> {
    let mut plans = Vec::new();
    flatten_exprs(expr, &mut plans);
    Ok(plans)
}

/// 扁平化 ClassExpression 树为标签名列表（用于 GetByLabel）。
fn flatten_exprs(expr: &ClassExpression, plans: &mut Vec<QueryPlan>) {
    match expr {
        ClassExpression::Top | ClassExpression::Bottom => {}
        ClassExpression::ClassName(name) => {
            plans.push(QueryPlan::GetByLabel(name.clone()));
        }
        ClassExpression::Intersection(a, b) | ClassExpression::Union(a, b) => {
            flatten_exprs(a, plans);
            flatten_exprs(b, plans);
        }
        ClassExpression::Complement(inner) => {
            flatten_exprs(inner, plans);
        }
        ClassExpression::SomeValuesFrom { property, filler: _ } => {
            plans.push(QueryPlan::GetRelationships {
                node_code: String::new(), rel_type: Some(property.clone()),
            });
        }
        ClassExpression::AllValuesFrom { property, filler: _ } => {
            plans.push(QueryPlan::GetRelationships {
                node_code: String::new(), rel_type: Some(property.clone()),
            });
        }
        ClassExpression::MinCardinality { property, .. }
        | ClassExpression::MaxCardinality { property, .. }
        | ClassExpression::ExactCardinality { property, .. } => {
            plans.push(QueryPlan::GetRelationships {
                node_code: String::new(), rel_type: Some(property.clone()),
            });
        }
        ClassExpression::OneOf(_) => {}
        ClassExpression::SelfRestriction(prop) => {
            plans.push(QueryPlan::GetRelationships {
                node_code: String::new(), rel_type: Some(prop.clone()),
            });
        }
    }
}
