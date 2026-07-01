//! 应用状态 — 持有推理器和图仓库的共享句柄。

use std::sync::Arc;

use ontology_reasoner::Reasoner;
use ontology_storage::repository::graph_store::SharedRepository;

/// 全局应用状态，在线程间共享
pub struct AppState {
    pub reasoner: Reasoner,
    pub repo: SharedRepository,
}

impl AppState {
    pub fn new(reasoner: Reasoner) -> Self {
        let repo = Arc::clone(reasoner.repo());
        Self { reasoner, repo }
    }
}
