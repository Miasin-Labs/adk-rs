pub const HELLO_WORLD_DOT: &str = r##"// Workflow Visualization
digraph {
    graph [bgcolor="#0F172A" fontname=Helvetica nodesep=0.5 pad=0.5 ranksep=0.8 splines=spline]
    node [color="#475569" fillcolor="#1E293B" fontcolor="#F8FAFC" fontname=Helvetica fontsize=12 margin="0.25,0.15" penwidth=1.5 shape=rect style="rounded,filled"]
    edge [arrowhead=vee arrowsize=0.7 color="#94A3B8" fontcolor="#CBD5E1" fontname=Helvetica fontsize=10 penwidth=1.2]
    hello_world_agent [label=<<FONT COLOR="#42A5F5" POINT-SIZE="14">✦</FONT> hello_world_agent> fillcolor="#1E293B" style="rounded,filled" tooltip=Agent]
    roll_die [label=<<FONT COLOR="#6B7280" POINT-SIZE="14">🔧</FONT> roll_die> fillcolor="#1E293B" style="rounded,filled,dashed" tooltip=Tool]
    check_prime [label=<<FONT COLOR="#6B7280" POINT-SIZE="14">🔧</FONT> check_prime> fillcolor="#1E293B" style="rounded,filled,dashed" tooltip=Tool]
    hello_world_agent -> roll_die [color="#94A3B8" style=dashed]
    hello_world_agent -> check_prime [color="#94A3B8" style=dashed]
}
"##;

use serde_json::{Value, json};

pub fn event_graph(event: Option<&Value>) -> Value {
    let Some(event) = event else {
        return json!({});
    };
    json!({ "dotSrc": highlighted_dot(event) })
}

fn highlighted_dot(event: &Value) -> String {
    let (from, to) = highlight_pair(event).unwrap_or(("hello_world_agent", ""));
    let mut dot = HELLO_WORLD_DOT.to_owned();
    if !to.is_empty() {
        let target = format!("{from} -> {to}");
        dot = dot.replace(&target, &format!("{target} [color=\"#34a853\" penwidth=4]"));
    }
    dot
}

fn highlight_pair(event: &Value) -> Option<(&str, &str)> {
    let part = event.get("content")?.get("parts")?.as_array()?.first()?;
    if let Some(call) = part.get("functionCall") {
        return call
            .get("name")
            .and_then(Value::as_str)
            .map(|name| ("hello_world_agent", name));
    }
    if let Some(response) = part.get("functionResponse") {
        return response
            .get("name")
            .and_then(Value::as_str)
            .map(|name| (name, "hello_world_agent"));
    }
    Some(("hello_world_agent", ""))
}
