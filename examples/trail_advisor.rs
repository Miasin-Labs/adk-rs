use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use adk_rs::{
    AgentBuilder,
    AgentName,
    AgentPrompt,
    AppName,
    AuthCredential,
    CredentialService,
    EventActions,
    EventAuthor,
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
    RunConfig,
    Runner,
    Session,
    SessionId,
    SessionStore,
    Tool,
    ToolCall,
    ToolError,
    ToolResult,
    ToolSpec,
    UserId,
};
use async_trait::async_trait;
use serde_json::{Value, json};

struct TrailAdvisorModel {
    step: AtomicUsize,
}

#[async_trait]
impl LanguageModel for TrailAdvisorModel {
    async fn generate(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let step = self.step.fetch_add(1, Ordering::SeqCst);
        let (name, args) = match step {
            0 => ("calendar_read", json!({ "date": "tomorrow" })),
            1 => ("weather_get", json!({ "city": "Draper" })),
            2 => ("air_quality_get", json!({ "zip": "84020" })),
            3 => ("trail_list", json!({ "max_minutes": 120 })),
            4 => ("message_preview", json!({ "approved": false })),
            _ => {
                return Ok(ModelResponse {
                    text: Some("Recommendation: run Corner Canyon before 8 AM. Weather is mild, air quality is good, and the route fits the two-hour window. Message preview was generated only; no email was sent.".to_owned()),
                    tool_calls: Vec::new(),
                    actions: EventActions::default(),
                });
            }
        };

        Ok(ModelResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: format!("{name}-{step}"),
                name: name.to_owned(),
                args,
            }],
            actions: EventActions::default(),
        })
    }
}

struct StaticJsonTool {
    name: &'static str,
    description: &'static str,
    content: Value,
}

#[async_trait]
impl Tool for StaticJsonTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name.to_owned(),
            description: self.description.to_owned(),
            input_schema: json!({ "type": "object" }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            content: self.content.clone(),
        })
    }
}

fn start_local_api() -> io::Result<(String, thread::JoinHandle<io::Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let handle = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept()?;
            let mut buffer = [0_u8; 2048];
            let bytes_read = stream.read(&mut buffer)?;
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            if !request.contains("x-api-key: demo-weather-key") {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "missing demo API key",
                ));
            }
            let body = if request.starts_with("GET /weather") {
                r#"{"temp_f":52,"condition":"clear"}"#
            } else {
                r#"{"aqi":31,"category":"good"}"#
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes())?;
        }
        Ok(())
    });
    Ok((format!("http://{addr}"), handle))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (api_base, api_thread) = start_local_api()?;
    let app_name = AppName::new("trail-advisor")?;
    let user_id = UserId::new("demo-user")?;
    let session_id = SessionId::new("morning-run")?;
    let store = InMemorySessionStore::default();
    store.create(Session::for_user(
        app_name.clone(),
        user_id.clone(),
        session_id.clone(),
    ))?;

    let credentials = InMemoryCredentialService::default();
    credentials.put_credential(
        &app_name,
        &user_id,
        "weather-api",
        AuthCredential::ApiKey("demo-weather-key".to_owned()),
    )?;

    let prompt = AgentPrompt::new("Trail run advisor")
        .task("Recommend one trail when the calendar includes a run.")
        .input("Calendar, weather, air quality, saved trails, and approval state.")
        .tools([
            "calendar_read",
            "weather_get",
            "air_quality_get",
            "trail_list",
            "message_preview",
        ])
        .constraints([
            "Do not send a message unless approval is true.",
            "Prefer good air quality and routes inside the available time.",
        ])
        .output("One recommendation and a message preview.");

    let weather_tool = HttpTool::new(HttpToolConfig {
        name: "weather_get".to_owned(),
        description: "Fetch local weather by city.".to_owned(),
        method: HttpMethod::Get,
        url: format!("{api_base}/weather"),
        query: vec![("city".to_owned(), "{city}".to_owned())],
        credential_key: Some("weather-api".to_owned()),
        ..HttpToolConfig::default()
    })?;
    let air_tool = HttpTool::new(HttpToolConfig {
        name: "air_quality_get".to_owned(),
        description: "Fetch local air quality by zip code.".to_owned(),
        method: HttpMethod::Get,
        url: format!("{api_base}/air"),
        query: vec![("zip".to_owned(), "{zip}".to_owned())],
        credential_key: Some("weather-api".to_owned()),
        ..HttpToolConfig::default()
    })?;

    let agent = AgentBuilder::new(
        AgentName::new("trail_advisor")?,
        prompt,
        Arc::new(TrailAdvisorModel {
            step: AtomicUsize::new(0),
        }),
    )
    .tool(Arc::new(StaticJsonTool {
        name: "calendar_read",
        description: "Read today's calendar.",
        content: json!({ "has_run": true, "available_minutes": 120 }),
    }))
    .tool(Arc::new(weather_tool))
    .tool(Arc::new(air_tool))
    .tool(Arc::new(StaticJsonTool {
        name: "trail_list",
        description: "List saved trails.",
        content: json!({
            "trails": [
                { "name": "Corner Canyon", "minutes": 95, "shade": "partial" },
                { "name": "Bonneville Shoreline", "minutes": 140, "shade": "low" }
            ]
        }),
    }))
    .tool(Arc::new(StaticJsonTool {
        name: "message_preview",
        description: "Preview the outbound message without sending it.",
        content: json!({ "sent": false, "channel": "email-preview" }),
    }))
    .build()?;

    let output = Runner::new(store, agent)
        .credential_service(Arc::new(credentials))
        .with_run_config(RunConfig {
            memory_window_events: Some(8),
            max_iterations: Some(8),
            ..RunConfig::default()
        })
        .run(
            &session_id,
            InvocationId::new("turn-1")?,
            "Plan tomorrow's run",
        )
        .await?;

    api_thread
        .join()
        .map_err(|_| io::Error::other("local demo API panicked"))??;
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
