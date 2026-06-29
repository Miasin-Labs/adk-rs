use std::collections::BTreeMap;

use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::auth::AuthCredential;
use crate::tool::{Tool, ToolCall, ToolError, ToolResult, ToolSpec};
use crate::tool_context::ToolContext;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpToolConfig {
    pub name: String,
    pub description: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
    pub allowed_domains: Vec<String>,
    pub credential: Option<AuthCredential>,
    pub credential_key: Option<String>,
}

impl Default for HttpToolConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            method: HttpMethod::Get,
            url: String::new(),
            headers: Vec::new(),
            query: Vec::new(),
            body: None,
            allowed_domains: Vec::new(),
            credential: None,
            credential_key: None,
        }
    }
}

#[derive(Clone)]
pub struct HttpTool {
    config: HttpToolConfig,
    client: reqwest::Client,
}

impl HttpTool {
    pub fn new(config: HttpToolConfig) -> Result<Self, ToolError> {
        validate_config(&config)?;
        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.config.name.clone(),
            description: self.config.description.clone(),
            input_schema: json!({ "type": "object" }),
        }
    }

    async fn call(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        self.execute(call, None).await
    }

    async fn call_with_context(
        &self,
        call: &ToolCall,
        context: &mut ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let credential = match &self.config.credential_key {
            Some(key) => context
                .credential(key)
                .map_err(|source| http_failed(&self.config.name, source.to_string()))?,
            None => None,
        };
        self.execute(call, credential.as_ref()).await
    }
}

impl HttpTool {
    async fn execute(
        &self,
        call: &ToolCall,
        context_credential: Option<&AuthCredential>,
    ) -> Result<ToolResult, ToolError> {
        let url = build_url(&self.config, &call.args)?;
        let mut request = match self.config.method {
            HttpMethod::Get => self.client.get(url),
            HttpMethod::Post => self.client.post(url),
        };

        for (name, value) in &self.config.headers {
            request = request.header(name, render_template(value, &call.args)?);
        }
        request = apply_credential(
            request,
            context_credential.or(self.config.credential.as_ref()),
        );
        if let Some(body) = &self.config.body {
            request = request.json(&render_json_templates(body, &call.args)?);
        }

        let response = request
            .send()
            .await
            .map_err(|source| http_failed(&self.config.name, source.to_string()))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|source| http_failed(&self.config.name, source.to_string()))?;
        let body = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));

        Ok(ToolResult {
            call_id: call.id.clone(),
            content: json!({
                "status": status,
                "body": body,
            }),
        })
    }
}

fn validate_config(config: &HttpToolConfig) -> Result<(), ToolError> {
    if config.name.trim().is_empty() {
        return Err(http_failed("http", "tool name cannot be empty"));
    }
    let url =
        Url::parse(&config.url).map_err(|source| http_failed(&config.name, source.to_string()))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(http_failed(
                &config.name,
                format!("unsupported URL scheme {scheme}"),
            ));
        }
    }
    validate_domain(config, &url)
}

fn build_url(config: &HttpToolConfig, args: &Value) -> Result<Url, ToolError> {
    let mut url = Url::parse(&render_template(&config.url, args)?)
        .map_err(|source| http_failed(&config.name, source.to_string()))?;
    validate_domain(config, &url)?;
    for (name, value) in &config.query {
        url.query_pairs_mut()
            .append_pair(name, &render_template(value, args)?);
    }
    Ok(url)
}

fn validate_domain(config: &HttpToolConfig, url: &Url) -> Result<(), ToolError> {
    if config.allowed_domains.is_empty() {
        return Ok(());
    }
    let host = url
        .host_str()
        .ok_or_else(|| http_failed(&config.name, "URL must include a host"))?;
    if config.allowed_domains.iter().any(|domain| domain == host) {
        return Ok(());
    }
    Err(http_failed(
        &config.name,
        format!("host {host} is not in allowed domains"),
    ))
}

fn apply_credential(
    request: reqwest::RequestBuilder,
    credential: Option<&AuthCredential>,
) -> reqwest::RequestBuilder {
    match credential {
        Some(AuthCredential::ApiKey(value)) => request.header("x-api-key", value),
        Some(AuthCredential::BearerToken(value)) => request.bearer_auth(value),
        Some(AuthCredential::OAuth2 { access_token, .. }) => request.bearer_auth(access_token),
        Some(AuthCredential::ServiceAccountJson(_)) | None => request,
    }
}

fn render_json_templates(value: &Value, args: &Value) -> Result<Value, ToolError> {
    match value {
        Value::String(text) => Ok(Value::String(render_template(text, args)?)),
        Value::Array(items) => items
            .iter()
            .map(|item| render_json_templates(item, args))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| Ok((key.clone(), render_json_templates(value, args)?)))
            .collect::<Result<BTreeMap<_, _>, ToolError>>()
            .map(|map| Value::Object(map.into_iter().collect())),
        other => Ok(other.clone()),
    }
}

fn render_template(template: &str, args: &Value) -> Result<String, ToolError> {
    let mut rendered = template.to_owned();
    let Some(map) = args.as_object() else {
        return Ok(rendered);
    };
    for (key, value) in map {
        let replacement = match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        rendered = rendered.replace(&format!("{{{key}}}"), &replacement);
    }
    Ok(rendered)
}

fn http_failed(name: &str, message: impl Into<String>) -> ToolError {
    ToolError::Failed {
        name: name.to_owned(),
        message: message.into(),
    }
}
