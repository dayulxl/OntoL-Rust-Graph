use std::fmt;

/// 存储层统一错误类型
#[derive(Debug)]
pub enum StoreError {
    Connection(String),
    Query(String),
    Transaction(String),
    Serialization(String),
    Config(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Connection(m) => write!(f, "Connection error: {}", m),
            StoreError::Query(m) => write!(f, "Query execution error: {}", m),
            StoreError::Transaction(m) => write!(f, "Transaction error: {}", m),
            StoreError::Serialization(m) => write!(f, "Serialization error: {}", m),
            StoreError::Config(m) => write!(f, "Configuration error: {}", m),
        }
    }
}

impl std::error::Error for StoreError {}

/// 映射层错误
#[derive(Debug)]
pub enum MappingError {
    IriNormalization(String),
    PropertyConversion(String),
    ModelMapping(String),
    LlmParse(String),
    LlmSchema(String),
}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MappingError::IriNormalization(m) => write!(f, "IRI normalization error: {}", m),
            MappingError::PropertyConversion(m) => write!(f, "Property conversion error: {}", m),
            MappingError::ModelMapping(m) => write!(f, "Model mapping error: {}", m),
            MappingError::LlmParse(m) => write!(f, "LLM response parse error: {}", m),
            MappingError::LlmSchema(m) => write!(f, "LLM JSON Schema generation error: {}", m),
        }
    }
}

impl std::error::Error for MappingError {}

/// 属性图层级错误
#[derive(Debug)]
pub enum GraphError {
    InvalidNode(String),
    InvalidRelationship(String),
    PatternMatching(String),
    IndexError(String),
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::InvalidNode(m) => write!(f, "Invalid node: {}", m),
            GraphError::InvalidRelationship(m) => write!(f, "Invalid relationship: {}", m),
            GraphError::PatternMatching(m) => write!(f, "Pattern matching error: {}", m),
            GraphError::IndexError(m) => write!(f, "Index error: {}", m),
        }
    }
}

impl std::error::Error for GraphError {}
