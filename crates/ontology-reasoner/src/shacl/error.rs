//! SHACL 验证错误类型。

use std::fmt;

use ontology_storage::error::StoreError;

/// SHACL 验证过程中的错误。
#[derive(Debug)]
pub enum ShaclError {
    /// 形状未找到（按名称查找时）
    ShapeNotFound(String),

    /// 节点未找到
    NodeNotFound(String),

    /// 形状解析失败
    ShapeParse(String),

    /// 约束不适用（如在不支持的上下文中使用某约束）
    ConstraintNotApplicable { constraint: String, reason: String },

    /// 属性路径无法解析
    PathResolution(String),

    /// 逻辑约束循环引用
    CircularReference(String),

    /// 图存储操作错误（透传）
    Storage(StoreError),

    /// 验证引擎内部错误
    Internal(String),
}

impl fmt::Display for ShaclError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShaclError::ShapeNotFound(name) => write!(f, "SHACL shape not found: '{}'", name),
            ShaclError::NodeNotFound(id) => write!(f, "SHACL target node not found: '{}'", id),
            ShaclError::ShapeParse(msg) => write!(f, "SHACL shape parse error: {}", msg),
            ShaclError::ConstraintNotApplicable { constraint, reason } => write!(
                f,
                "SHACL constraint '{}' not applicable: {}",
                constraint, reason
            ),
            ShaclError::PathResolution(msg) => write!(f, "SHACL path resolution error: {}", msg),
            ShaclError::CircularReference(msg) => {
                write!(f, "SHACL circular reference: {}", msg)
            }
            ShaclError::Storage(e) => write!(f, "SHACL storage error: {}", e),
            ShaclError::Internal(msg) => write!(f, "SHACL internal error: {}", msg),
        }
    }
}

impl std::error::Error for ShaclError {}

impl From<StoreError> for ShaclError {
    fn from(e: StoreError) -> Self {
        ShaclError::Storage(e)
    }
}
