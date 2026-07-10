//! LLM 响应解析器 — 将原始 JSON 文本解析为强类型 Rust 结构。
//!
//! 防止裸字符串直接流入业务层导致解析失败和流程崩溃。
//! 支持三种解析策略：
//! 1. **Structured Output** — LLM 按 JSON Schema 返回的严格 JSON
//! 2. **Tool Call** — LLM 通过 Function Calling 返回的工具调用
//! 3. **Text Extraction** — 从混合文本中提取 JSON 块（fallback）

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::MappingError;
use crate::mapper::llm::tool::ToolCall;

// ── 解析后的 LLM 响应 ──

/// 解析后的 LLM 响应，包含文本内容、工具调用和结构化输出。
///
/// 类型参数 `T` 为预期的结构化输出类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmResponse<T> {
    /// 文本内容（LLM 的纯文本回复）
    pub content: Option<String>,

    /// 工具调用列表（Function Calling 返回）
    pub tool_calls: Vec<ToolCall>,

    /// 结构化输出（从 JSON Schema 约束解析出的强类型结果）
    pub structured_output: Option<T>,
}

impl<T> LlmResponse<T> {
    /// 创建一个仅含文本内容的响应
    pub fn text(content: String) -> Self {
        LlmResponse {
            content: Some(content),
            tool_calls: Vec::new(),
            structured_output: None,
        }
    }

    /// 创建一个仅含结构化输出的响应
    pub fn structured(data: T) -> Self {
        LlmResponse {
            content: None,
            tool_calls: Vec::new(),
            structured_output: Some(data),
        }
    }

    /// 创建一个含工具调用的响应
    pub fn with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        LlmResponse {
            content,
            tool_calls,
            structured_output: None,
        }
    }

    /// 是否有工具调用
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// 是否有结构化输出
    pub fn has_structured_output(&self) -> bool {
        self.structured_output.is_some()
    }

    /// 获取文本内容（如有）
    pub fn text(&self) -> Option<&str> {
        self.content.as_deref()
    }
}

// ── 解析策略 ──

/// LLM 原始输出的格式类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmOutputFormat {
    /// LLM 直接返回 JSON（structured output 模式）
    Json,
    /// LLM 返回包含 JSON 代码块的混合文本
    MarkdownJsonBlock,
    /// 完整的 OpenAI 兼容 chat completion 响应
    ChatCompletion,
    /// 自动检测格式
    Auto,
}

/// 解析 LLM 原始输出为 `LlmResponse<T>`
///
/// ## 解析流程
///
/// 1. 尝试直接 JSON 解析（structured output）
/// 2. 如果含 tool_calls 字段，提取工具调用
/// 3. Fallback：从 Markdown 代码块中提取 JSON
pub fn parse_response<T: DeserializeOwned>(
    raw: &str,
    format: LlmOutputFormat,
) -> Result<LlmResponse<T>, MappingError> {
    match format {
        LlmOutputFormat::Json => parse_as_json(raw),
        LlmOutputFormat::MarkdownJsonBlock => parse_from_markdown(raw),
        LlmOutputFormat::ChatCompletion => parse_chat_completion(raw),
        LlmOutputFormat::Auto => {
            // 自动检测：先试直接 JSON，再试 Markdown 提取
            parse_as_json(raw).or_else(|_| parse_from_markdown(raw))
        }
    }
}

/// 将原始文本解析为结构化 JSON
fn parse_as_json<T: DeserializeOwned>(raw: &str) -> Result<LlmResponse<T>, MappingError> {
    let trimmed = raw.trim();

    // 尝试反序列化为完整的 ChatCompletion 格式
    if let Ok(chat) = serde_json::from_str::<OpenAiChatCompletion>(trimmed) {
        return convert_chat_completion(chat);
    }

    // 尝试直接反序列化为目标类型 T
    match serde_json::from_str::<T>(trimmed) {
        Ok(data) => Ok(LlmResponse::structured(data)),
        Err(e) => Err(MappingError::LlmParse(format!(
            "JSON 解析失败: {}。原始内容前 200 字符: {}",
            e,
            &trimmed[..trimmed.len().min(200)]
        ))),
    }
}

/// 从 Markdown 代码块中提取 JSON
fn parse_from_markdown<T: DeserializeOwned>(raw: &str) -> Result<LlmResponse<T>, MappingError> {
    // 提取 ```json ... ``` 代码块
    let json_block = extract_json_block(raw).ok_or_else(|| {
        MappingError::LlmParse("未找到 JSON 代码块 (```json ... ```)".to_string())
    })?;

    parse_as_json(&json_block)
}

/// 解析 OpenAI 兼容的 chat completion 响应
fn parse_chat_completion<T: DeserializeOwned>(raw: &str) -> Result<LlmResponse<T>, MappingError> {
    let chat: OpenAiChatCompletion = serde_json::from_str(raw.trim())
        .map_err(|e| MappingError::LlmParse(format!("Chat Completion JSON 解析失败: {}", e)))?;

    convert_chat_completion(chat)
}

// ── OpenAI 兼容结构（内部使用） ──

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletion {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    #[serde(default)]
    content: Option<String>,

    #[serde(default, rename = "tool_calls")]
    tool_calls: Vec<OpenAiToolCall>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String, // JSON 字符串
}

/// 将 OpenAI ChatCompletion 格式转换为统一的 LlmResponse
fn convert_chat_completion<T: DeserializeOwned>(
    chat: OpenAiChatCompletion,
) -> Result<LlmResponse<T>, MappingError> {
    let message = chat
        .choices
        .into_iter()
        .next()
        .map(|c| c.message)
        .ok_or_else(|| MappingError::LlmParse("ChatCompletion 无 choices".to_string()))?;

    let mut tool_calls = Vec::new();

    for tc in message.tool_calls {
        let arguments: serde_json::Value =
            serde_json::from_str(&tc.function.arguments).map_err(|e| {
                MappingError::LlmParse(format!(
                    "工具调用参数 JSON 解析失败 (tool={}): {}",
                    tc.function.name, e
                ))
            })?;

        tool_calls.push(ToolCall {
            id: tc.id,
            name: tc.function.name,
            arguments,
        });
    }

    // 如果 content 中包含 JSON，尝试解析为结构化输出
    let structured = if let Some(ref content) = message.content {
        if let Ok(data) = serde_json::from_str::<T>(content.trim()) {
            Some(data)
        } else {
            None
        }
    } else {
        None
    };

    Ok(LlmResponse {
        content: message.content,
        tool_calls,
        structured_output: structured,
    })
}

// ── JSON 提取工具 ──

/// 从文本中提取 ```json ... ``` 代码块内容
pub fn extract_json_block(text: &str) -> Option<String> {
    let start_marker = "```json";
    let end_marker = "```";

    let start = text.find(start_marker)?;
    let content_start = start + start_marker.len();

    // 跳过开头的换行符
    let content_start = text[content_start..]
        .find(|c: char| !c.is_whitespace())
        .map(|offset| content_start + offset)
        .unwrap_or(content_start);

    let end = text[content_start..].find(end_marker)?;

    Some(text[content_start..content_start + end].trim().to_string())
}

/// 尝试从任意文本中提取第一个有效的 JSON 对象
pub fn extract_json_object(text: &str) -> Option<String> {
    let trimmed = text.trim();

    // 找到第一个 '{'
    let start = trimmed.find('{')?;

    // 括号匹配扫描
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (i, ch) in trimmed[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(trimmed[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

// ── 批量解析 ──

/// 尝试多种策略解析 LLM 响应，返回第一个成功的结果
pub fn parse_best_effort<T: DeserializeOwned>(raw: &str) -> Result<LlmResponse<T>, MappingError> {
    // 策略 1：直接 JSON
    if let Ok(result) = parse_as_json::<T>(raw) {
        return Ok(result);
    }

    // 策略 2：Markdown 代码块
    if let Ok(result) = parse_from_markdown::<T>(raw) {
        return Ok(result);
    }

    // 策略 3：从文本中提取 JSON 对象
    if let Some(json_str) = extract_json_object(raw) {
        if let Ok(result) = parse_as_json::<T>(&json_str) {
            return Ok(result);
        }
    }

    // 全部失败：返回纯文本
    Ok(LlmResponse::text(raw.to_string()))
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct TestOutput {
        class_iri: String,
        label: Option<String>,
    }

    #[test]
    fn parse_direct_json() {
        let raw = r#"{"class_iri": "ex:Person", "label": "人"}"#;
        let result: LlmResponse<TestOutput> = parse_response(raw, LlmOutputFormat::Json).unwrap();

        assert!(result.has_structured_output());
        let data = result.structured_output.unwrap();
        assert_eq!(data.class_iri, "ex:Person");
        assert_eq!(data.label, Some("人".to_string()));
    }

    #[test]
    fn parse_markdown_json_block() {
        let raw = r#"以下是查询结果：
```json
{"class_iri": "ex:Organization", "label": "组织"}
```
查询完成。"#;

        let result: LlmResponse<TestOutput> =
            parse_response(raw, LlmOutputFormat::MarkdownJsonBlock).unwrap();

        let data = result.structured_output.unwrap();
        assert_eq!(data.class_iri, "ex:Organization");
    }

    #[test]
    fn parse_chat_completion_with_tool_calls() {
        let raw = r#"{
            "choices": [{
                "message": {
                    "content": "我来查询 Person 类的个体",
                    "tool_calls": [{
                        "id": "call_001",
                        "function": {
                            "name": "query_individuals_by_class",
                            "arguments": "{\"class_iri\": \"ex:Person\", \"limit\": 10}"
                        }
                    }]
                }
            }]
        }"#;

        let result: LlmResponse<TestOutput> =
            parse_response(raw, LlmOutputFormat::ChatCompletion).unwrap();

        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "query_individuals_by_class");

        let args: serde_json::Value = result.tool_calls[0].arguments.clone();
        assert_eq!(args["class_iri"], "ex:Person");
    }

    #[test]
    fn extract_json_from_text() {
        let text = "前缀文本 {\"key\": \"value\"} 后缀文本";
        let extracted = extract_json_object(text).unwrap();
        assert_eq!(extracted, r#"{"key": "value"}"#);
    }

    #[test]
    fn parse_best_effort_falls_back_to_text() {
        let raw = "这是一段纯文本回复，没有 JSON。";
        let result: LlmResponse<TestOutput> = parse_best_effort(raw).unwrap();
        assert_eq!(result.text(), Some("这是一段纯文本回复，没有 JSON。"));
        assert!(!result.has_structured_output());
    }
}
