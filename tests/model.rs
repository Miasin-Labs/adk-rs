use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use adk_rs::{
    AuthCredential,
    Event,
    EventAuthor,
    EventId,
    EventPart,
    InvocationId,
    LanguageModel,
    ModelResponse,
    OpenAiCompatibleConfig,
    OpenAiCompatibleModel,
    ToolCall,
    ToolSpec,
};
use serde_json::json;

#[tokio::test]
async fn openai_compatible_model_posts_tools_and_parses_tool_calls_normal() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 8192];
        let bytes_read = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request.contains("authorization: Bearer test-key"));
        assert!(request.contains("\"model\":\"gpt-test\""));
        assert!(request.contains("\"role\":\"system\""));
        assert!(request.contains("\"name\":\"lookup\""));

        let body = json!({
            "choices": [{
                "message": {
                    "content": "I will look that up.",
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{\"query\":\"rust\"}"
                        }
                    }]
                }
            }]
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    let model = OpenAiCompatibleModel::new(OpenAiCompatibleConfig {
        base_url: format!("http://{addr}/v1"),
        model: "gpt-test".to_owned(),
        credential: AuthCredential::BearerToken("test-key".to_owned()),
    })
    .unwrap();
    let response = model
        .generate(adk_rs::ModelRequest {
            instruction: "Use tools when needed.".to_owned(),
            events: vec![Event {
                id: EventId::for_index(1),
                invocation_id: InvocationId::new("turn-1").unwrap(),
                author: EventAuthor::User,
                parts: vec![EventPart::Text("find rust".to_owned())],
                actions: adk_rs::EventActions::default(),
                timestamp_seconds: 0,
            }],
            tools: vec![ToolSpec {
                name: "lookup".to_owned(),
                description: "Lookup a topic".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"]
                }),
            }],
        })
        .await
        .unwrap();

    server.join().unwrap();
    assert_eq!(
        response,
        ModelResponse {
            text: Some("I will look that up.".to_owned()),
            tool_calls: vec![ToolCall {
                id: "call-1".to_owned(),
                name: "lookup".to_owned(),
                args: json!({ "query": "rust" }),
            }],
            actions: adk_rs::EventActions::default(),
        }
    );
}
