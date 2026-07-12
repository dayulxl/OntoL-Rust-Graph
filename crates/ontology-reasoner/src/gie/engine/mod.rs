//! InferenceEngine — 四步图推理机
//!
//! Step 1: 复制所有推理关联对象 + RDFS 祖先链
//! Step 2: 创建副本节点之间的对应关系
//! Step 3: 继承属性 + 修改属性
//! Step 4: 逐节点推理 + 叙述输出

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::relationship::Relationship;
use ontology_storage::mapper::unified_mapping::ACTION_TYPE_KEY;
use ontology_storage::repository::graph_store::SharedRepository;

use crate::confidence::policy::ConfidencePolicy;
use crate::error::ReasonerError;
use crate::graph::util;
use crate::graph::util::prop_as_f64;
use crate::swrl::ast::ExecutionStats;
use crate::swrl::engine::SwrlEngine;

#[derive(Debug, Clone)]
pub struct InferenceRequest { pub node_ids: Vec<String>, pub confidence: f64, pub cope_version: String }
impl Default for InferenceRequest {
    fn default() -> Self { Self { node_ids: vec![], confidence: 0.8, cope_version: "default".into() } }
}

#[derive(Debug, Clone)]
pub struct InferenceReport {
    pub cope_version: String, pub cloned_count: usize, pub iterations: usize,
    pub swrl_stats: ExecutionStats, pub confidence_blocks: usize, pub total_ms: u64,
    pub narration: Vec<String>,
}

pub struct InferenceEngine {
    repo: SharedRepository, policy: ConfidencePolicy,
    event_tx: Option<Sender<String>>, buf: Vec<String>,
}

impl InferenceEngine {
    pub fn new(repo: SharedRepository) -> Self {
        Self { repo, policy: ConfidencePolicy::default(), event_tx: None, buf: Vec::new() }
    }
    pub fn with_policy(mut self, p: ConfidencePolicy) -> Self { self.policy = p; self }
    pub fn with_event_channel(mut self, tx: Sender<String>) -> Self { self.event_tx = Some(tx); self }

    fn nid(n: &Node) -> String { n.property("_nid").and_then(|v| v.as_i64()).map(|i| i.to_string()).unwrap_or_else(|| "?".into()) }
    fn nc(n: &Node) -> String { n.property("code").and_then(|v| v.as_str()).unwrap_or("?").to_string() }
    fn nm(n: &Node) -> String { n.property("name").and_then(|v| v.as_str()).unwrap_or("未命名").to_string() }
    fn other(rel: &Relationship, my: &str) -> String { if rel.start_node_id == my { rel.end_node_id.clone() } else { rel.start_node_id.clone() } }

    fn say(&mut self, s: &str) { self.buf.push(s.to_string()); log::info!("{}", s);
        if let Some(ref tx) = self.event_tx { let _ = tx.send(format!("{{\"msg\":\"{}\"}}\n", esc(s))); } }

    fn clone_node(&self, n: &Node, ver: &str) -> Result<String, ReasonerError> {
        let c = Self::nc(n);
        let e = util::inherit_entity_properties(self.repo.as_ref(), n);
        util::ensure_cope_version_with_props(self.repo.as_ref(), &c, ver, &e.labels, &e.properties)
            .map_err(ReasonerError::SwrlExecution)
    }

    // ═══════════════════ 主入口 ═══════════════════
    pub fn reason_on_nodes(&mut self, r: InferenceRequest) -> Result<InferenceReport, ReasonerError> {
        let t0 = Instant::now();
        let ver = &r.cope_version;
        let threshold = self.policy.threshold();

        let mut seeds: Vec<Node> = Vec::new();
        for id in &r.node_ids { if let Ok(Some(n)) = self.repo.get_node(id) { seeds.push(n); } }
        if seeds.is_empty() { return Err(ReasonerError::SwrlExecution("未找到节点".into())); }

        // orig_nid → (orig_node, copy_nid)
        let mut cm: HashMap<String, (Node, String)> = HashMap::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut rq: VecDeque<String> = VecDeque::new();

        // ═══ Step 1: 复制所有推理关联对象 + RDFS 祖先链 ═══
        self.say("═══ Step 1: 复制推理关联对象 ═══");

        for seed in &seeds {
            let snid = Self::nid(seed);
            let copy_id = self.clone_node(seed, ver)?;
            self.say(&format!("种子: {} 副本ID={}", Self::nm(seed), copy_id));
            cm.insert(snid.clone(), (seed.clone(), copy_id));
            visited.insert(snid.clone());
            rq.push_back(snid.clone());

            let all_rels = util::get_all_relationships(self.repo.as_ref(), &snid);

            // 1a. RDFS 祖先链递归爬顶层
            let mut rdfs_list: Vec<String> = Vec::new();
            for rel in &all_rels {
                if !crate::language::is_ontology_relation(&rel.rel_type) { continue; }
                let up = Self::other(rel, &snid);
                if visited.contains(&up) { continue; }
                let mut stack = vec![up];
                while let Some(cur) = stack.pop() {
                    if visited.contains(&cur) || rdfs_list.contains(&cur) { continue; }
                    rdfs_list.push(cur.clone()); visited.insert(cur.clone());
                    if let Ok(Some(n)) = self.repo.get_node(&cur) {
                        match self.clone_node(&n, ver) {
                            Ok(cid) => { cm.insert(cur.clone(), (n, cid)); }
                            Err(e) => log::warn!("RDFS克隆失败: {}", e),
                        }
                    }
                    if let Ok(crs) = self.repo.get_relationships(&cur, None) {
                        for cr in &crs {
                            if crate::language::is_ontology_relation(&cr.rel_type) {
                                let cu = Self::other(cr, &cur);
                                if !visited.contains(&cu) && !rdfs_list.contains(&cu) { stack.push(cu); }
                            }
                        }
                    }
                }
            }
            if !rdfs_list.is_empty() { self.say(&format!("  RDFS链: {} 个祖先", rdfs_list.len())); }

            // 1b. 推理下游递归克隆
            let mut wl: VecDeque<String> = VecDeque::new();
            for rel in &all_rels {
                if rel.properties.get(ACTION_TYPE_KEY).and_then(|v| v.as_str()).unwrap_or("") == "inference" {
                    let nb = Self::other(rel, &snid);
                    if !visited.contains(&nb) {
                        if let Ok(Some(n)) = self.repo.get_node(&nb) {
                            if n.property("cope_version").and_then(|v| v.as_str()).unwrap_or("").is_empty() { wl.push_back(nb); }
                        }
                    }
                }
            }
            let mut dc = 0usize;
            while let Some(cid) = wl.pop_front() {
                if visited.contains(&cid) { continue; }
                visited.insert(cid.clone());
                if let Ok(Some(cn)) = self.repo.get_node(&cid) {
                    match self.clone_node(&cn, ver) {
                        Ok(cc) => { cm.insert(cid.clone(), (cn.clone(), cc)); dc += 1; rq.push_back(cid.clone()); }
                        Err(e) => log::warn!("下游克隆失败: {}", e),
                    }
                    if let Ok(drs) = self.repo.get_relationships(&cid, None) {
                        for dr in &drs {
                            if dr.properties.get(ACTION_TYPE_KEY).and_then(|v| v.as_str()).unwrap_or("") == "inference" {
                                let n2 = Self::other(dr, &cid);
                                if !visited.contains(&n2) {
                                    if let Ok(Some(nn)) = self.repo.get_node(&n2) {
                                        if nn.property("cope_version").and_then(|v| v.as_str()).unwrap_or("").is_empty() { wl.push_back(n2); }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if dc > 0 { self.say(&format!("  下游: {} 个推理节点", dc)); }
        }

        // ═══ Step 2: 创建副本节点之间的对应关系 ═══
        self.say(&format!("═══ Step 2: 创建 {} 个副本节点间的关系 ═══", cm.len()));
        let mut rc = 0usize;
        let keys: Vec<String> = cm.keys().cloned().collect();
        for oid in &keys {
            let out = self.repo.get_relationships(oid, None).unwrap_or_default();
            for r in &out {
                if let (Some((_, from)), Some((_, to))) = (cm.get(&r.start_node_id), cm.get(&r.end_node_id)) {
                    let nr = Relationship { rel_type: r.rel_type.clone(), start_node_id: from.clone(),
                        end_node_id: to.clone(), properties: r.properties.clone() };
                    let _ = self.repo.insert_relationship(&nr);
                    rc += 1;
                }
            }
        }
        self.say(&format!("  共复制 {} 条关系", rc));

        // ═══ Step 3: 继承属性 ═══
        self.say("═══ Step 3: 继承属性 ═══");
        for (_nid, (orig, copy_id)) in &cm {
            let enriched = util::inherit_entity_properties(self.repo.as_ref(), orig);
            let inh = enriched.properties.len().saturating_sub(orig.properties.len());
            if inh > 0 { self.say(&format!("  {} 继承 {} 个属性 (副本ID={})", Self::nm(orig), inh, copy_id)); }
        }

        // ═══ Step 4: 推理 ═══
        self.say(&format!("═══ Step 4: 推理 ({} 个节点, 置信度={:.2}, 阈值={:.2}) ═══", rq.len(), r.confidence, threshold));
        let mut confs: HashMap<String, f64> = HashMap::new();
        for k in cm.keys() { confs.insert(k.clone(), r.confidence.clamp(0.0, 1.0)); }
        let mut reasoned: HashSet<String> = HashSet::new();
        let mut report = InferenceReport { cope_version: r.cope_version.clone(), cloned_count: cm.len(),
            iterations: 0, swrl_stats: ExecutionStats::default(), confidence_blocks: 0, total_ms: 0, narration: vec![] };

        while let Some(nid) = rq.pop_front() {
            if reasoned.contains(&nid) { continue; } reasoned.insert(nid.clone());
            let (node, copy_id) = match cm.get(&nid) { Some(v) => v, None => continue };
            let name = Self::nm(node); let code = Self::nc(node);
            let mut conf = confs.get(&nid).copied().unwrap_or(r.confidence);
            report.iterations += 1;

            let enriched = util::inherit_entity_properties(self.repo.as_ref(), node);
            let inherited = enriched.properties.len().saturating_sub(node.properties.len());
            self.say(&format!("【第{}步】{}（{}）原ID={} 副本ID={}", report.iterations, name, code, nid, copy_id));
            if inherited > 0 { self.say(&format!("  → 继承 {} 个父类型属性", inherited)); }

            let ba = crate::swrl::behavior::parse_behavior(node);
            if ba.is_empty() {
                self.say("  → 无行为字段");
            } else {
                if !ba.precondition_shacl.is_empty() { self.say(&format!("  hasPrecondition: {}（True通过/False阻断）", ba.precondition_shacl)); }
                if !ba.effect_text.is_empty() {
                    let rt = if ba.effect_text.starts_with("swrl:") { "SWRL引擎" } else if ba.effect_text.starts_with("sh:") { "SHACL验证" } else { "SWRL" };
                    self.say(&format!("  hasEffect: {} → {}", ba.effect_text, rt));
                    let mut eng = SwrlEngine::new(Arc::clone(&self.repo)).with_max_iterations(100).with_policy(self.policy.clone());
                    let res = crate::swrl::behavior::execute_behavior(self.repo.as_ref(), &ba, &mut eng, 5, 0);
                    if res.triggered { report.swrl_stats.total_derived += res.derived_count; self.say(&format!("  ✅ 触发成功 {}条推导", res.derived_count)); }
                    else if let Some(r) = &res.blocked_reason { self.say(&format!("  ⛔ 阻断: {}", r)); }
                }
                if !ba.cost_text.is_empty() { self.say(&format!("  hasCost: {}", ba.cost_text)); }
                if ba.duration_secs > 0 { self.say(&format!("  hasDuration: {}（{}秒）", ba.duration_text, ba.duration_secs)); }
                if !ba.priority_text.is_empty() { self.say(&format!("  hasPriority: {}（等级{}）", ba.priority_text, ba.priority_value)); }
                if !ba.composed_of.is_empty() { self.say(&format!("  composedOf: {}", ba.composed_of.join("; "))); }
            }

            let nc = prop_as_f64(node.property("confidence"));
            conf = match nc { Some(v) => conf * v, None => conf };
            confs.insert(nid.clone(), conf);
            if nc.is_some() && conf < threshold {
                report.confidence_blocks += 1;
                self.say(&format!("  ⛔ 置信度{:.2}<{:.2} 阻断", conf, threshold));
                continue;
            }

            // 沿推理边跳下游
            let rels2 = util::get_all_relationships(self.repo.as_ref(), &nid);
            for rel in &rels2 {
                if rel.properties.get(ACTION_TYPE_KEY).and_then(|v| v.as_str()).unwrap_or("") != "inference" { continue; }
                let nb = Self::other(rel, &nid);
                if reasoned.contains(&nb) || !cm.contains_key(&nb) { continue; }
                rq.push_back(nb.clone()); confs.insert(nb.clone(), conf);
                let p = cm.get(&nb);
                let pn = p.map_or("?".to_string(), |(n,_)| Self::nm(n));
                let pc = p.map_or("?".to_string(), |(n,_)| Self::nc(n));
                self.say(&format!("  → 【{}】→ {}（{}）", rel.rel_type, pn, pc));
            }
        }

        report.total_ms = t0.elapsed().as_millis() as u64;
        report.cloned_count = cm.len();
        report.narration = std::mem::take(&mut self.buf);
        Ok(report)
    }
}

fn esc(s: &str) -> String { s.replace('\\',"\\\\").replace('"',"\\\"").replace('\n',"\\n") }
