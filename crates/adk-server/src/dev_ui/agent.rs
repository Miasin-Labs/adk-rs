use serde_json::{Value, json};

use super::config::OpenAiConfig;

const ROOT_AGENT: &str = "hello_world_agent";
const DESCRIPTION: &str =
    "hello world agent that can roll a dice of 8 sides and check prime numbers.";
const INSTRUCTION: &str = r#"
      You roll dice and answer questions about the outcome of the dice rolls.
      You can roll dice of different sizes.
      You can use multiple tools in parallel by calling functions in parallel(in one request and in one round).
      It is ok to discuss previous dice roles, and comment on the dice rolls.
      When you are asked to roll a die, you must call the roll_die tool with the number of sides. Be sure to pass in an integer. Do not pass in a string.
      You should never roll a die on your own.
      When checking prime numbers, call the check_prime tool with a list of integers. Be sure to pass in a list of integers. You should never pass in a string.
      You should not check prime numbers before calling the tool.
      When you are asked to roll a die and check prime numbers, you should always make the following two function calls:
      1. You should first call the roll_die tool to get a roll. Wait for the function response before calling the check_prime tool.
      2. After you get the function response from roll_die tool, you should call the check_prime tool with the roll_die result.
        2.1 If user asks you to check primes based on previous rolls, make sure you include the previous rolls in the list.
      3. When you respond, you must include the roll_die result from step 1.
      You should always perform the previous 3 steps when asking for a roll and checking prime numbers.
      You should not rely on the previous history on prime results.
    "#;

pub fn app_info(app_name: &str) -> Value {
    json!({
        "name": app_name,
        "rootAgentName": ROOT_AGENT,
        "description": DESCRIPTION,
        "language": "rust",
        "isComputerUse": false,
        "agents": { ROOT_AGENT: agent_summary() }
    })
}

pub fn build_graph(app_name: &str) -> Value {
    json!({
        "name": app_name,
        "root_agent": {
            "name": ROOT_AGENT,
            "description": DESCRIPTION,
            "rerun_on_resume": false,
            "wait_for_output": false,
            "sub_agents": [],
            "model": model_name(),
            "instruction": INSTRUCTION,
            "global_instruction": "",
            "tools": [
                { "name": "roll_die", "type": "tool" },
                { "name": "check_prime", "type": "tool" }
            ],
            "mode": "chat",
            "disallow_transfer_to_parent": false,
            "disallow_transfer_to_peers": false,
            "include_contents": "default"
        }
    })
}

pub fn builder_yaml(app_name: &str) -> String {
    format!(
        r#"name: {app_name}
root_agent:
  name: {ROOT_AGENT}
  model: {model}
  instruction: |
{instruction}
  tools:
    - roll_die
    - check_prime
"#,
        model = model_name(),
        instruction = indent(INSTRUCTION.trim(), 4),
    )
}

fn agent_summary() -> Value {
    json!({
        "name": ROOT_AGENT,
        "description": DESCRIPTION,
        "instruction": INSTRUCTION,
        "tools": [roll_die_declaration(), check_prime_declaration()],
        "sub_agents": []
    })
}

fn roll_die_declaration() -> Value {
    json!({ "functionDeclarations": [{ "description": "Roll a die and return the rolled result.", "name": "roll_die", "parametersJsonSchema": { "properties": { "sides": { "title": "Sides", "type": "integer" } }, "required": ["sides"], "title": "roll_dieParams", "type": "object" } }] })
}

fn check_prime_declaration() -> Value {
    json!({ "functionDeclarations": [{ "description": "Check if a given list of numbers are prime.", "name": "check_prime", "parametersJsonSchema": { "properties": { "nums": { "items": { "type": "integer" }, "title": "Nums", "type": "array" } }, "required": ["nums"], "title": "check_primeParams", "type": "object" } }] })
}

fn model_name() -> String {
    OpenAiConfig::load()
        .map(|config| format!("openai/{}", config.model))
        .unwrap_or_else(|| "gemini-2.5-flash".to_owned())
}

fn indent(text: &str, spaces: usize) -> String {
    let padding = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{padding}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
