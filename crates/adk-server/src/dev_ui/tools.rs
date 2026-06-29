use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

/// Current unix time in milliseconds.
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

/// Current time as an ISO-8601 UTC string (Howard Hinnant's civil-from-days).
pub fn now_iso() -> String {
    let millis_total = now_unix_ms() as i64;
    let total = millis_total.div_euclid(1000);
    let millis = millis_total.rem_euclid(1000);
    let days = total.div_euclid(86_400);
    let tod = total.rem_euclid(86_400);
    let (hour, minute, second) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

#[derive(Debug, Clone)]
pub struct ToolObservation {
    pub call_id: String,
    pub name: String,
    pub args: Value,
    pub response: Value,
    pub state_delta: Option<Value>,
}

/// Async HTTP executor shared by the agent's `httpRequest` tool and the
/// standalone HTTP Tool node. Returns `{ statusCode, body }` or `{ error }`.
pub async fn http_request(
    method: &str,
    url: &str,
    body: Option<&Value>,
    headers: &[(String, String)],
) -> Value {
    if url.trim().is_empty() {
        return json!({ "error": "missing url" });
    }
    let client = reqwest::Client::new();
    let mut request = match method.to_ascii_uppercase().as_str() {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => client.get(url),
    }
    .timeout(std::time::Duration::from_secs(30));
    for (key, value) in headers {
        request = request.header(key, value);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            let parsed = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!(text));
            json!({ "statusCode": status, "body": parsed })
        }
        Err(error) => json!({ "error": format!("HTTP request failed: {error}") }),
    }
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
