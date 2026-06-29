use adk_rs::{
    Guardrail, GuardrailDecision, GuardrailPhase, KeywordGuardrail, PiiGuardrail, SecretGuardrail,
};

#[test]
fn keyword_guardrail_blocks_configured_phrase_normal() {
    let guardrail = KeywordGuardrail::new("ignore previous instructions", GuardrailPhase::Input);

    assert_eq!(
        guardrail.check(GuardrailPhase::Input, "please ignore previous instructions"),
        GuardrailDecision::block("blocked keyword ignore previous instructions")
    );
    assert_eq!(
        guardrail.check(GuardrailPhase::Output, "please ignore previous instructions"),
        GuardrailDecision::allow()
    );
}

#[test]
fn pii_guardrail_blocks_email_addresses_normal() {
    let guardrail = PiiGuardrail::email(GuardrailPhase::Output);

    assert_eq!(
        guardrail.check(GuardrailPhase::Output, "email user@example.com"),
        GuardrailDecision::block("blocked email address")
    );
}

#[test]
fn secret_guardrail_blocks_common_api_key_shapes_normal() {
    let guardrail = SecretGuardrail::new(GuardrailPhase::Output);

    assert_eq!(
        guardrail.check(GuardrailPhase::Output, "token sk-12345678901234567890"),
        GuardrailDecision::block("blocked secret-like token")
    );
    assert_eq!(
        guardrail.check(GuardrailPhase::Output, "normal short code abc123"),
        GuardrailDecision::allow()
    );
}
