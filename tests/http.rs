use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use adk_rs::{
    AgentBuilder,
    AgentName,
    AppName,
    AuthCredential,
    CredentialService,
    EventActions,
    EventPart,
    HttpMethod,
    HttpTool,
    HttpToolConfig,
    InMemoryCredentialService,
    InMemorySessionStore,
    InvocationId,
    LanguageModel,
    ModelError,
    ModelRequest,
    ModelResponse,
    Runner,
    Session,
    SessionId,
    SessionStore,
    Tool,
    ToolCall,
    UserId,
};
use async_trait::async_trait;
use serde_json::json;

struct HttpThenTextModel {
    calls: Mutex<usize>,
}

#[async_trait]
impl LanguageModel for HttpThenTextModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            return Ok(ModelResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "weather-1".to_owned(),
                    name: "weather_get".to_owned(),
                    args: json!({ "city": "Draper" }),
                }],
                actions: EventActions::default(),
            });
        }

        Ok(ModelResponse {
            text: Some("weather checked".to_owned()),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        })
    }
}

#[tokio::test]
async fn http_tool_calls_local_json_api_normal() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 1024];
        let bytes_read = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        assert!(request.starts_with("GET /weather?city=Draper HTTP/1.1"));
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "content-length: 11\r\n",
            "\r\n",
            "{\"ok\":true}"
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let tool = HttpTool::new(HttpToolConfig {
        name: "weather_get".to_owned(),
        description: "Get weather by city".to_owned(),
        method: HttpMethod::Get,
        url: format!("http://{addr}/weather"),
        query: vec![("city".to_owned(), "{city}".to_owned())],
        ..HttpToolConfig::default()
    })
    .unwrap();

    let result = tool
        .call(&ToolCall {
            id: "weather-1".to_owned(),
            name: "weather_get".to_owned(),
            args: json!({ "city": "Draper" }),
        })
        .await
        .unwrap();

    server.join().unwrap();
    assert_eq!(result.content["status"], 200);
    assert_eq!(result.content["body"]["ok"], true);
}

#[tokio::test]
async fn runner_resolves_http_tool_credential_from_context_normal() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 2048];
        let bytes_read = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        assert!(request.starts_with("GET /weather?city=Draper HTTP/1.1"));
        assert!(request.contains("x-api-key: test-secret"));
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "content-length: 14\r\n",
            "\r\n",
            "{\"rain\":false}"
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    let app_name = AppName::new("trail-app").unwrap();
    let user_id = UserId::new("trail-user").unwrap();
    let session_id = SessionId::new("credential-session").unwrap();
    let store = InMemorySessionStore::default();
    store
        .create(Session::for_user(
            app_name.clone(),
            user_id.clone(),
            session_id.clone(),
        ))
        .unwrap();
    let credentials = InMemoryCredentialService::default();
    credentials
        .put_credential(
            &app_name,
            &user_id,
            "weather",
            AuthCredential::ApiKey("test-secret".to_owned()),
        )
        .unwrap();

    let weather_tool = HttpTool::new(HttpToolConfig {
        name: "weather_get".to_owned(),
        description: "Get weather by city".to_owned(),
        method: HttpMethod::Get,
        url: format!("http://{addr}/weather"),
        query: vec![("city".to_owned(), "{city}".to_owned())],
        credential_key: Some("weather".to_owned()),
        ..HttpToolConfig::default()
    })
    .unwrap();
    let agent = AgentBuilder::new(
        AgentName::new("trail_advisor").unwrap(),
        "Check weather with credentials.",
        Arc::new(HttpThenTextModel {
            calls: Mutex::new(0),
        }),
    )
    .tool(Arc::new(weather_tool))
    .build()
    .unwrap();
    let runner = Runner::new(store, agent).credential_service(Arc::new(credentials));

    let output = runner
        .run(&session_id, InvocationId::new("turn-1").unwrap(), "run?")
        .await
        .unwrap();

    server.join().unwrap();
    assert!(output.events.iter().any(|event| {
        matches!(
            event.parts.first(),
            Some(EventPart::ToolResult(result)) if result.content["body"]["rain"] == false
        )
    }));
}
