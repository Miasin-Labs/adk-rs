use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct ToolObservation {
    pub call_id: String,
    pub name: String,
    pub args: Value,
    pub response: Value,
    pub state_delta: Option<Value>,
}

pub fn execute(call_id: String, name: &str, args: Value, rolls: &mut Vec<i64>) -> ToolObservation {
    match name {
        "roll_die" => roll_die(call_id, args, rolls),
        "check_prime" => check_prime(call_id, args),
        _ => ToolObservation {
            call_id,
            name: name.to_owned(),
            args,
            response: json!({ "error": "unknown tool" }),
            state_delta: None,
        },
    }
}

fn roll_die(call_id: String, args: Value, rolls: &mut Vec<i64>) -> ToolObservation {
    let sides = args
        .get("sides")
        .and_then(Value::as_i64)
        .unwrap_or(6)
        .max(1);
    let result = random_bounded(sides);
    rolls.push(result);
    ToolObservation {
        call_id,
        name: "roll_die".to_owned(),
        args,
        response: json!({ "result": result }),
        state_delta: Some(json!({ "rolls": rolls })),
    }
}

fn check_prime(call_id: String, args: Value) -> ToolObservation {
    let nums = args
        .get("nums")
        .and_then(Value::as_array)
        .map(|nums| nums.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    let primes = nums
        .into_iter()
        .filter(|num| is_prime(*num))
        .collect::<Vec<_>>();
    let result = if primes.is_empty() {
        "No prime numbers found.".to_owned()
    } else {
        format!(
            "{} are prime numbers.",
            primes
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    ToolObservation {
        call_id,
        name: "check_prime".to_owned(),
        args,
        response: json!({ "result": result }),
        state_delta: None,
    }
}

fn random_bounded(sides: i64) -> i64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_nanos());
    i64::from(nanos % u32::try_from(sides).unwrap_or(u32::MAX)) + 1
}

fn is_prime(number: i64) -> bool {
    if number <= 1 {
        return false;
    }
    let mut divisor = 2;
    while divisor * divisor <= number {
        if number % divisor == 0 {
            return false;
        }
        divisor += 1;
    }
    true
}
