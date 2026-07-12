//! # SWRL 翻译器 — SWRL 规则 → QueryPlan / AtomPattern

use ontology_storage::mapper::query_plan::AtomPattern;
use crate::swrl::ast::{Atom, Rule};

/// 将 SWRL 规则的前提原子提取为 AtomPattern 列表。
pub fn extract_atom_patterns(rule: &Rule) -> Vec<AtomPattern> {
    rule.antecedent
        .iter()
        .filter_map(|atom| match atom {
            Atom::ClassAtom { class_iri, variable } => {
                Some(AtomPattern::ClassAtom {
                    class_name: class_iri.clone(),
                    variable: variable.clone(),
                })
            }
            Atom::ObjectPropertyAtom { property_iri, subject, object } => {
                Some(AtomPattern::PropertyAtom {
                    property: property_iri.clone(),
                    subject_var: subject.clone(),
                    object_var: object.clone(),
                })
            }
            Atom::SameAs(a, b) => Some(AtomPattern::SameAs {
                var_a: a.clone(), var_b: b.clone(),
            }),
            Atom::DifferentFrom(a, b) => Some(AtomPattern::DifferentFrom {
                var_a: a.clone(), var_b: b.clone(),
            }),
            Atom::Builtin { builtin_iri, arguments } => {
                Some(AtomPattern::Builtin {
                    builtin_iri: builtin_iri.clone(),
                    args: arguments.clone(),
                })
            }
            _ => {
                log::debug!("SWRL 原子暂不支持翻译: {:?}", atom);
                None
            }
        })
        .collect()
}
