//! # ontology-reasoner
//!
//! 本体推理引擎 — DWL2 DL 查询 + SWRL 规则推理 + 置信度熔断。
//!
//! ## 架构
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │                      Reasoner                              │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐ │
//! │  │ DWL2 Query   │  │ SWRL Engine  │  │ Confidence Fuse  │ │
//! │  │   Engine     │  │  (fixpoint)  │  │  (< 0.3 → stop)  │ │
//! │  └──────┬───────┘  └──────┬───────┘  └────────┬─────────┘ │
//! │         │                 │                    │           │
//! │         └─────────────────┼────────────────────┘           │
//! │                           │                                │
//! │  ┌──────────────┐         │                                │
//! │  │ SHACL Engine │         │                                │
//! │  │  (validate)  │         │                                │
//! │  └──────┬───────┘         │                                │
//! │         │                 │                                │
//! │         └─────────────────┼────────────────────┘           │
//! │                           │                                │
//! │                    ┌──────▼──────┐                         │
//! │                    │  GraphRepo  │  (property graph)       │
//! │                    └─────────────┘                         │
//! └────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 快速开始
//!
//! ```rust,ignore
//! use std::rc::Rc;
//! use ontology_storage::adapters::in_memory::executor::InMemoryAdapter;
//! use ontology_reasoner::{ReasonerBuilder, ReasonerConfig};
//!
//! let repo = Rc::new(InMemoryAdapter::new());
//! let reasoner = ReasonerBuilder::new(repo)
//!     .with_max_iterations(50)
//!     .with_fuse_threshold(0.3)
//!     .build();
//!
//! // 加载 SWRL 规则
//! reasoner.load_swrl_rules(r#"
//!     [parentChild: hasParent(?x, ?y) ^ hasBrother(?y, ?z) -> hasUncle(?x, ?z)]
//!     [adult: :Person(?x) ^ hasAge(?x, ?age) ^ swrlb:greaterThan(?age, 18) -> :Adult(?x)]
//! "#)?;
//!
//! // 执行推理
//! let report = reasoner.reason()?;
//! println!("{}", report);
//! ```
//!
//! ## 置信度熔断
//!
//! 每次推理步骤前，引擎计算当前置信度。一旦 < 0.3，
//! 立即中止该规则链路，返回 `ConfidenceFuse` 错误。
//! 这确保低质量推理不会污染知识图谱。

pub mod confidence;
pub mod dwl2;
pub mod error;
pub mod gie;
pub mod graph;
pub mod language;
pub mod logger;
pub mod shacl;
pub mod spatial;
pub mod swrl;
pub mod timeline;

mod reasoner;

pub use confidence::calculator::{ConfidenceCalculator, ConfidenceInput, ConfidenceWeights};
pub use confidence::fuse::{CONFIDENCE_THRESHOLD, ConfidenceFuse, FuseState};
pub use confidence::policy::{ConfidencePolicy, InferenceMode, SourceCategory};
pub use dwl2::ast::{
    Cardinality, ClassExpression, Dwl2Query, Dwl2Result, PropertyRestriction, Quantifier, QueryType,
};
pub use dwl2::query::Dwl2QueryEngine;
pub use error::ReasonerError;
pub use language::{
    GroupedExpressions, LanguagePrefix, ParsedExpression, classify_inference_prefix,
    classify_strings_by_prefix, group_expressions, is_inference_prefix, is_inference_relation,
    is_ontology_relation, parse_language_expression,
};
pub use ontology_storage::mapper::query_plan::{AtomPattern, ConstraintSpec, QueryPlan, QueryResult};
pub use reasoner::{
    ReasonOnNodesReport, ReasonOnNodesRequest, Reasoner, ReasonerBuilder, ReasonerConfig,
    ReasonerReport,
};
// GIE types — re-exported for external consumers (e.g. ontology-server)
pub use gie::context::InferenceContext;
pub use gie::engine::{InferenceEngine, InferenceReport, InferenceRequest};
pub use spatial::haversine_m;
pub use swrl::ast::{Atom, ExecutionStats, InferenceResult, Rule, VariableBinding};
pub use swrl::behavior::{
    BehaviorAction, BehaviorResult, evaluate_shacl_precondition, execute_behaviors_batch,
    execute_effect, parse_behavior,
};
pub use swrl::builtins::{BuiltinRegistry, BuiltinResult, BuiltinValue};
pub use swrl::engine::SwrlEngine;
pub use swrl::parser::SwrlParser;
pub use timeline::engine::TimelineEngine;
pub use timeline::model::{Segment, TimelineInput, TimelineResult, WaypointInput};

pub use graph::detector::{DefaultStateChangeDetector, StateChangeDetector};
pub use graph::explorer::{Direction, ExploreConfig, ExploreHop, ExploreResult, GraphExplorer};
pub use graph::util::{
    RelCount, RuleMatch, clone_all_for_version, clone_nodes_selective, collect_parent_properties,
    delete_by_cope_version, ensure_cope_version, ensure_cope_version_with_props, find_entity_any,
    find_incoming_relationships, find_matching_rules, get_all_relationships, get_type_ancestors,
    inherit_entity_properties, predict_next_steps, prop_as_f64, summarize_relations, truncate_str,
    update_entity_properties,
};

pub use shacl::ast::{
    Constraint, NodeKind, NodeShape, PropertyPath, PropertyShape, Severity, Shape, ShapesGraph,
    Target,
};
pub use shacl::engine::ShaclEngine;
pub use shacl::error::ShaclError;
pub use shacl::result::{ValidationReport, ValidationResult};
