use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use adk_rs::{
    AgentBuilder,
    AgentName,
    EventActions,
    EventAuthor,
    EventPart,
    InMemorySessionStore,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    Runner,
    SessionId,
    Tool,
    ToolCall,
    ToolError,
    ToolResult,
    ToolSpec,
};
use async_trait::async_trait;
use serde_json::{Value, json};

struct DraftReviewModel {
    calls: AtomicUsize,
}

#[async_trait]
impl LanguageModel for DraftReviewModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let previous_calls = self.calls.fetch_add(1, Ordering::SeqCst);

        if previous_calls == 0 {
            let user_text = latest_text(&request).unwrap_or("empty draft");
            return Ok(ModelResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "count-words-1".to_owned(),
                    name: "word_count".to_owned(),
                    args: json!({ "text": user_text }),
                }],
                actions: EventActions::default(),
            });
        }

        let words = latest_word_count(&request).unwrap_or(0);
        Ok(ModelResponse {
            text: Some(format!(
                "The draft has {words} words. Keep the next revision focused on one outcome."
            )),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

struct WordCountTool;

#[async_trait]
impl Tool for WordCountTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "word_count".to_owned(),
            description: "Count words in a text string.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                },
                "required": ["text"]
            }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        let text = call
            .args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed {
                name: "word_count".to_owned(),
                message: "missing text string".to_owned(),
            })?;

        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({ "words": text.split_whitespace().count() }),
        })
    }
}

fn latest_text(request: &ModelRequest) -> Option<&str> {
    request
        .events
        .iter()
        .rev()
        .flat_map(|event| event.parts.iter())
        .find_map(|part| match part {
            EventPart::Text(text) => Some(text.as_str()),
            EventPart::ToolCall(_) | EventPart::ToolResult(_) => None,
        })
}

fn latest_word_count(request: &ModelRequest) -> Option<u64> {
    request
        .events
        .iter()
        .rev()
        .flat_map(|event| event.parts.iter())
        .find_map(|part| match part {
            EventPart::ToolResult(result) => result.content.get("words").and_then(Value::as_u64),
            EventPart::Text(_) | EventPart::ToolCall(_) => None,
        })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model: Arc<dyn LanguageModel> = Arc::new(DraftReviewModel {
        calls: AtomicUsize::new(0),
    });
    let word_count_tool: Arc<dyn Tool> = Arc::new(WordCountTool);

    let agent = AgentBuilder::new(
        AgentName::new("draft_reviewer")?,
        "Review a draft, call tools when a deterministic check is useful, and answer briefly.",
        model,
    )
    .tool(word_count_tool)
    .build()?;

    let runner = Runner::new(InMemorySessionStore::default(), agent);
    let output = runner
        .run(
            &SessionId::new("tool-demo")?,
            InvocationId::new("turn-1")?,
            "This crate needs clear docs and a few tiny agents.",
        )
        .await?;

    for event in output.events {
        let author = match event.author {
            EventAuthor::User => "user".to_owned(),
            EventAuthor::Agent(name) => format!("agent:{}", name.as_str()),
            EventAuthor::Tool(name) => format!("tool:{name}"),
        };

        for part in event.parts {
            match part {
                EventPart::Text(text) => println!("{author}: {text}"),
                EventPart::ToolResult(result) => println!("{author}: {}", result.content),
                EventPart::ToolCall(_) => {}
            }
        }
    }

    Ok(())
}
