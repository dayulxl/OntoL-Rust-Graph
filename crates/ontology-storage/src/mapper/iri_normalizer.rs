//! IRI 压缩 / 解压缩。
//!
//! 将完整的 IRI 通过预定义前缀表转换为简短标识符
//! （如 `ex:Person` ↔ `http://example.org/ontology#Person`），
//! 用于减少网络传输和存储开销。

use std::collections::HashMap;

/// 常用 RDF / OWL 前缀映射表
fn builtin_prefixes() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("rdf",  "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("owl",  "http://www.w3.org/2002/07/owl#"),
        ("xsd",  "http://www.w3.org/2001/XMLSchema#"),
    ])
}

/// 前缀 ↔ 完整 IRI 的注册表
#[derive(Debug, Clone)]
pub struct IriNormalizer {
    prefix_to_ns: HashMap<String, String>,
    ns_to_prefix: HashMap<String, String>,
}

impl IriNormalizer {
    /// 创建带有内置前缀的默认实例
    pub fn new() -> Self {
        let mut n = Self {
            prefix_to_ns: HashMap::new(),
            ns_to_prefix: HashMap::new(),
        };
        for (prefix, ns) in builtin_prefixes() {
            n.register(prefix.to_string(), ns.to_string());
        }
        n
    }

    /// 注册自定义前缀
    pub fn register(&mut self, prefix: String, namespace: String) {
        self.ns_to_prefix.insert(namespace.clone(), prefix.clone());
        self.prefix_to_ns.insert(prefix, namespace);
    }

    /// 压缩：完整 IRI → `prefix:local`
    ///
    /// 若匹配到已注册命名空间则返回压缩形式，否则原样返回完整 IRI。
    pub fn compress(&self, full_iri: &str) -> String {
        for (ns, prefix) in &self.ns_to_prefix {
            if let Some(local) = full_iri.strip_prefix(ns.as_str()) {
                return format!("{}:{}", prefix, local);
            }
        }
        full_iri.to_string()
    }

    /// 解压缩：`prefix:local` → 完整 IRI
    ///
    /// 若未找到前缀或输入不含 `:` 则原样返回。
    pub fn expand(&self, curie: &str) -> String {
        if let Some((prefix, local)) = curie.split_once(':') {
            if let Some(ns) = self.prefix_to_ns.get(prefix) {
                return format!("{}{}", ns, local);
            }
        }
        curie.to_string()
    }

    /// IRI → 内部短名称，仅取 local part
    ///
    /// 例如 `http://www.w3.org/2002/07/owl#Thing` → `"Thing"`.
    pub fn iri_to_name(&self, iri: &str) -> String {
        // 先尝试压缩，然后取 local part
        let compressed = self.compress(iri);
        if let Some((_prefix, local)) = compressed.split_once(':') {
            local.to_string()
        } else if let Some(pos) = iri.rfind('#') {
            iri[pos + 1..].to_string()
        } else if let Some(pos) = iri.rfind('/') {
            iri[pos + 1..].to_string()
        } else {
            iri.to_string()
        }
    }
}

impl Default for IriNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_known_prefix() {
        let n = IriNormalizer::new();
        let compressed = n.compress("http://www.w3.org/2000/01/rdf-schema#label");
        assert_eq!(compressed, "rdfs:label");
    }

    #[test]
    fn expand_known_prefix() {
        let n = IriNormalizer::new();
        let expanded = n.expand("owl:Thing");
        assert_eq!(expanded, "http://www.w3.org/2002/07/owl#Thing");
    }

    #[test]
    fn iri_to_name_strips_namespace() {
        let n = IriNormalizer::new();
        let name = n.iri_to_name("http://www.w3.org/2002/07/owl#Thing");
        assert_eq!(name, "Thing");
    }
}
