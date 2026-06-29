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

struct SocialPostModel {
    step: AtomicUsize,
}

#[async_trait]
impl LanguageModel for SocialPostModel {
    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let step = self.step.fetch_add(1, Ordering::SeqCst);
        match step {
            0 => Ok(ModelResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "search-1".to_owned(),
                    name: "search_news".to_owned(),
                    args: json!({ "topic": latest_text(&request).unwrap_or("AI agents") }),
                }],
                actions: EventActions::default(),
            }),
            1 => Ok(ModelResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "critique-1".to_owned(),
                    name: "critique_post".to_owned(),
                    args: json!({
                        "draft": draft_from_articles(&request),
                    }),
                }],
                actions: EventActions::default(),
            }),
            _ => Ok(ModelResponse {
                text: Some(final_post(&request)),
                tool_calls: Vec::new(),
                actions: EventActions::default(),
            }),
        }
    }
}

struct SearchNewsTool;

#[async_trait]
impl Tool for SearchNewsTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "search_news".to_owned(),
            description: "Return a tiny set of source notes for a topic.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "topic": { "type": "string" }
                },
                "required": ["topic"]
            }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        let topic = call
            .args
            .get("topic")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed {
                name: "search_news".to_owned(),
                message: "missing topic string".to_owned(),
            })?;

        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({
                "articles": [
                    format!("{topic}: agents choose tools instead of following a fixed path"),
                    format!("{topic}: tool results become observations for the next model call")
                ]
            }),
        })
    }
}

struct CritiquePostTool;

#[async_trait]
impl Tool for CritiquePostTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "critique_post".to_owned(),
            description: "Check whether a draft is specific enough to publish.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "draft": { "type": "string" }
                },
                "required": ["draft"]
            }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        let draft = call
            .args
            .get("draft")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed {
                name: "critique_post".to_owned(),
                message: "missing draft string".to_owned(),
            })?;

        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({
                "approved": draft.contains("Observation"),
                "note": "Keep the post concrete and mention the tool loop."
            }),
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

fn draft_from_articles(request: &ModelRequest) -> String {
    let articles = request
        .events
        .iter()
        .rev()
        .flat_map(|event| event.parts.iter())
        .find_map(|part| match part {
            EventPart::ToolResult(result) => result.content.get("articles").cloned(),
            EventPart::Text(_) | EventPart::ToolCall(_) => None,
        })
        .unwrap_or_else(|| json!([]));

    format!("Observation: {articles}. Draft: agents pick tools, observe results, and iterate.")
}

fn final_post(request: &ModelRequest) -> String {
    let critique = request
        .events
        .iter()
        .rev()
        .flat_map(|event| event.parts.iter())
        .find_map(|part| match part {
            EventPart::ToolResult(result) => result.content.get("note").and_then(Value::as_str),
            EventPart::Text(_) | EventPart::ToolCall(_) => None,
        })
        .unwrap_or("Ship the clearest version.");

    format!(
        "AI agents are workflows where the model makes the next-step decision. It can search, observe, critique, and revise. Critique note: {critique}"
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model: Arc<dyn LanguageModel> = Arc::new(SocialPostModel {
        step: AtomicUsize::new(0),
    });
    let search_tool: Arc<dyn Tool> = Arc::new(SearchNewsTool);
    let critique_tool: Arc<dyn Tool> = Arc::new(CritiquePostTool);

    let agent = AgentBuilder::new(
        AgentName::new("social_post_agent")?,
        "Create a short social post by choosing tools, observing results, and iterating.",
        model,
    )
    .tool(search_tool)
    .tool(critique_tool)
    .build()?;

    let runner = Runner::new(InMemorySessionStore::default(), agent);
    let output = runner
        .run(
            &SessionId::new("react-demo")?,
            InvocationId::new("turn-1")?,
            "Explain AI agents to beginners",
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
