#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardrailPhase {
    Input,
    ToolCall,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl GuardrailDecision {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    pub fn block(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("guardrail {name} blocked {phase:?}: {reason}")]
pub struct GuardrailError {
    pub name: String,
    pub phase: GuardrailPhase,
    pub reason: String,
}

pub trait Guardrail: Send + Sync {
    fn name(&self) -> &str;

    fn check(&self, phase: GuardrailPhase, text: &str) -> GuardrailDecision;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordGuardrail {
    keyword: String,
    phase: GuardrailPhase,
}

impl KeywordGuardrail {
    pub fn new(keyword: impl Into<String>, phase: GuardrailPhase) -> Self {
        Self {
            keyword: keyword.into(),
            phase,
        }
    }
}

impl Guardrail for KeywordGuardrail {
    fn name(&self) -> &str {
        "keyword"
    }

    fn check(&self, phase: GuardrailPhase, text: &str) -> GuardrailDecision {
        if phase == self.phase && text.to_ascii_lowercase().contains(&self.keyword) {
            return GuardrailDecision::block(format!("blocked keyword {}", self.keyword));
        }
        GuardrailDecision::allow()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiiGuardrail {
    phase: GuardrailPhase,
}

impl PiiGuardrail {
    pub fn email(phase: GuardrailPhase) -> Self {
        Self { phase }
    }
}

impl Guardrail for PiiGuardrail {
    fn name(&self) -> &str {
        "pii"
    }

    fn check(&self, phase: GuardrailPhase, text: &str) -> GuardrailDecision {
        if phase == self.phase && contains_email(text) {
            return GuardrailDecision::block("blocked email address");
        }
        GuardrailDecision::allow()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretGuardrail {
    phase: GuardrailPhase,
}

impl SecretGuardrail {
    pub fn new(phase: GuardrailPhase) -> Self {
        Self { phase }
    }
}

impl Guardrail for SecretGuardrail {
    fn name(&self) -> &str {
        "secret"
    }

    fn check(&self, phase: GuardrailPhase, text: &str) -> GuardrailDecision {
        if phase == self.phase && text.split_whitespace().any(secret_like) {
            return GuardrailDecision::block("blocked secret-like token");
        }
        GuardrailDecision::allow()
    }
}

pub fn enforce_guardrails(
    guardrails: &[std::sync::Arc<dyn Guardrail>],
    phase: GuardrailPhase,
    text: &str,
) -> Result<(), GuardrailError> {
    for guardrail in guardrails {
        let decision = guardrail.check(phase, text);
        if !decision.allowed {
            return Err(GuardrailError {
                name: guardrail.name().to_owned(),
                phase,
                reason: decision.reason.unwrap_or_else(|| "blocked".to_owned()),
            });
        }
    }
    Ok(())
}

fn contains_email(text: &str) -> bool {
    text.split_whitespace().any(|word| {
        let word = word.trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '@' && character != '.' && character != '_' && character != '-');
        let Some((local, domain)) = word.split_once('@') else {
            return false;
        };
        !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
    })
}

fn secret_like(value: &str) -> bool {
    let value = value.trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '-' && character != '_');
    if value.starts_with("sk-") && value.len() >= 20 {
        return true;
    }
    value.len() >= 32 && value.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
}
