//! n8n expression evaluation backed by an embedded JS engine (boa).
//!
//! Parameter strings starting with `=` are n8n expressions; `{{ ... }}` segments
//! are evaluated as JavaScript with the n8n expression variables in scope
//! (`$json`, `$node`, `$(...)`, `$input`, `$now`, `$today`, `$workflow`,
//! `$itemIndex`). This covers real JS (arithmetic, string methods, ternaries),
//! not just `$json` accessor paths.

use boa_engine::property::Attribute;
use boa_engine::{Context, JsValue, Source, js_string};
use serde_json::{Value, json};

/// The data in scope when resolving a node's parameters.
pub(crate) struct ExprContext {
    /// The current input item's `json` (becomes `$json` / `$input.item.json`).
    pub json: Value,
    /// Map of node name -> that node's first output item `json` (`$node`, `$(...)`).
    pub nodes: Value,
    /// Current time in ms (`$now` / `$today`).
    pub now_ms: u64,
    /// `{ id, name }` of the running workflow (`$workflow`).
    pub workflow: Value,
    /// Index of the current item (`$itemIndex`).
    pub item_index: usize,
    /// Timezone offset in minutes applied to `$now` / `$today` (0 = UTC).
    pub tz_offset_minutes: i64,
}

const PREAMBLE: &str = r#"
(function () {
  var c = globalThis.__ctx;
  globalThis.$json = c.json;
  var nodeObj = {};
  for (var k in c.nodes) { nodeObj[k] = { json: c.nodes[k], item: { json: c.nodes[k] } }; }
  globalThis.$node = nodeObj;
  globalThis.$ = function (name) {
    var j = c.nodes[name];
    return {
      json: j,
      item: { json: j },
      first: function () { return { json: j }; },
      last: function () { return { json: j }; },
      all: function () { return [{ json: j }]; },
    };
  };
  globalThis.$input = {
    item: { json: c.json },
    first: function () { return { json: c.json }; },
    last: function () { return { json: c.json }; },
    all: function () { return [{ json: c.json }]; },
    params: {},
  };
  // Luxon-style DateTime shim (subset) backed by JS Date. `ms` is the true UTC
  // epoch; display reads a zone-shifted Date so wall-clock fields are local.
  var OFF = (c.tzOffset || 0) * 60000;
  function DT(ms) { this.ms = ms; }
  DT.fromMillis = function (ms) { return new DT(ms); };
  DT.now = function () { return new DT(c.now); };
  DT.prototype._d = function () { return new Date(this.ms + OFF); };
  DT.prototype.toMillis = function () { return this.ms; };
  function _pad(n, l) { n = '' + Math.abs(n); while (n.length < l) n = '0' + n; return n; }
  DT.prototype.toISO = function () {
    var d = this._d();
    var off = c.tzOffset || 0;
    var zone = off === 0 ? 'Z' : (off > 0 ? '+' : '-') + _pad(Math.floor(Math.abs(off) / 60), 2) + ':' + _pad(Math.abs(off) % 60, 2);
    return d.getUTCFullYear() + '-' + _pad(d.getUTCMonth() + 1, 2) + '-' + _pad(d.getUTCDate(), 2)
      + 'T' + _pad(d.getUTCHours(), 2) + ':' + _pad(d.getUTCMinutes(), 2) + ':' + _pad(d.getUTCSeconds(), 2)
      + '.' + _pad(d.getUTCMilliseconds(), 3) + zone;
  };
  DT.prototype.toISODate = function () { return this._d().toISOString().slice(0, 10); };
  DT.prototype.toString = function () { return this.toISO(); };
  var _U = { year: 31536000000, month: 2592000000, week: 604800000, day: 86400000, hour: 3600000, minute: 60000, second: 1000, millisecond: 1 };
  function _amt(o, sign) { var ms = 0; for (var k in o) { var u = _U[k] || _U[k.replace(/s$/, '')] || 0; ms += o[k] * u * sign; } return ms; }
  DT.prototype.plus = function (o) { return new DT(this.ms + _amt(o, 1)); };
  DT.prototype.minus = function (o) { return new DT(this.ms + _amt(o, -1)); };
  DT.prototype.startOf = function (unit) {
    var d = this._d();
    if (unit === 'day') { d.setUTCHours(0, 0, 0, 0); }
    else if (unit === 'hour') { d.setUTCMinutes(0, 0, 0); }
    else if (unit === 'month') { d.setUTCDate(1); d.setUTCHours(0, 0, 0, 0); }
    else if (unit === 'year') { d.setUTCMonth(0, 1); d.setUTCHours(0, 0, 0, 0); }
    return new DT(d.getTime() - OFF);
  };
  DT.prototype.diff = function (other, unit) { var u = _U[unit] || _U[(unit || '').replace(/s$/, '')] || 1; return (this.ms - other.ms) / u; };
  DT.prototype.toFormat = function (fmt) {
    var d = this._d();
    return fmt.replace(/yyyy/g, d.getUTCFullYear()).replace(/MM/g, _pad(d.getUTCMonth() + 1, 2))
      .replace(/dd/g, _pad(d.getUTCDate(), 2)).replace(/HH/g, _pad(d.getUTCHours(), 2))
      .replace(/mm/g, _pad(d.getUTCMinutes(), 2)).replace(/ss/g, _pad(d.getUTCSeconds(), 2));
  };
  Object.defineProperty(DT.prototype, 'year', { get: function () { return this._d().getUTCFullYear(); } });
  Object.defineProperty(DT.prototype, 'month', { get: function () { return this._d().getUTCMonth() + 1; } });
  Object.defineProperty(DT.prototype, 'day', { get: function () { return this._d().getUTCDate(); } });
  Object.defineProperty(DT.prototype, 'hour', { get: function () { return this._d().getUTCHours(); } });
  Object.defineProperty(DT.prototype, 'minute', { get: function () { return this._d().getUTCMinutes(); } });
  Object.defineProperty(DT.prototype, 'second', { get: function () { return this._d().getUTCSeconds(); } });
  Object.defineProperty(DT.prototype, 'weekday', { get: function () { var w = this._d().getUTCDay(); return w === 0 ? 7 : w; } });
  globalThis.DateTime = DT;
  globalThis.$now = new DT(c.now);
  globalThis.$today = new DT(c.now).startOf('day');
  globalThis.$workflow = c.workflow;
  var _eid = (c.workflow && c.workflow.executionId) || '';
  globalThis.$execution = {
    id: _eid,
    mode: 'manual',
    resumeUrl: 'http://localhost:8091/webhook-waiting/' + _eid,
    customData: {},
  };
  globalThis.$binary = {};
  globalThis.$itemIndex = c.itemIndex;
  globalThis.$runIndex = 0;
  globalThis.$vars = {};
})();
"#;

/// Evaluate a single JS expression (the inside of `{{ ... }}`). Returns an empty
/// string on any error so a bad expression degrades gracefully.
pub(crate) fn evaluate(expression: &str, context: &ExprContext) -> String {
    try_evaluate(expression, context).unwrap_or_default()
}

fn try_evaluate(expression: &str, context: &ExprContext) -> Result<String, String> {
    let mut engine = Context::default();
    let bundle = json!({
        "json": context.json,
        "nodes": context.nodes,
        "now": context.now_ms,
        "workflow": context.workflow,
        "itemIndex": context.item_index,
        "tzOffset": context.tz_offset_minutes,
    });
    let bundle = JsValue::from_json(&bundle, &mut engine).map_err(|error| error.to_string())?;
    engine
        .register_global_property(js_string!("__ctx"), bundle, Attribute::all())
        .map_err(|error| error.to_string())?;
    engine
        .eval(Source::from_bytes(PREAMBLE))
        .map_err(|error| error.to_string())?;
    let result = engine
        .eval(Source::from_bytes(expression))
        .map_err(|error| error.to_string())?;
    let value = result.to_json(&mut engine).map_err(|error| error.to_string())?;
    Ok(value_to_string(&value))
}

/// Run a Code-node script ("run once for all items"). The body may use `items`,
/// `$input`, `$json`, etc., and should `return` an array of items (each a plain
/// object or `{ json }`). Returns the produced `{ json }` items.
pub(crate) fn run_code(
    code: &str,
    items: &[Value],
    context: &ExprContext,
) -> Result<Vec<Value>, String> {
    let mut engine = Context::default();
    let bundle = json!({
        "json": context.json,
        "nodes": context.nodes,
        "now": context.now_ms,
        "workflow": context.workflow,
        "itemIndex": context.item_index,
        "tzOffset": context.tz_offset_minutes,
        "items": items,
    });
    let bundle = JsValue::from_json(&bundle, &mut engine).map_err(|error| error.to_string())?;
    engine
        .register_global_property(js_string!("__ctx"), bundle, Attribute::all())
        .map_err(|error| error.to_string())?;
    engine
        .eval(Source::from_bytes(PREAMBLE))
        .map_err(|error| error.to_string())?;
    engine
        .eval(Source::from_bytes("globalThis.items = __ctx.items;"))
        .map_err(|error| error.to_string())?;
    let wrapped = format!("(function () {{ {code} }})()");
    let result = engine
        .eval(Source::from_bytes(wrapped.as_str()))
        .map_err(|error| error.to_string())?;
    let value = result.to_json(&mut engine).map_err(|error| error.to_string())?;
    let array = value.as_array().cloned().unwrap_or_else(|| vec![value]);
    Ok(array
        .into_iter()
        .map(|entry| {
            if entry.get("json").is_some() {
                entry
            } else {
                json!({ "json": entry })
            }
        })
        .collect())
}

/// Evaluate a self-contained math/JS expression to a number (used by the
/// agent's `calculator` tool). No n8n context is injected.
pub(crate) fn eval_math(expression: &str) -> Result<f64, String> {
    let mut engine = Context::default();
    let result = engine
        .eval(Source::from_bytes(expression))
        .map_err(|error| error.to_string())?;
    let value = result.to_json(&mut engine).map_err(|error| error.to_string())?;
    value
        .as_f64()
        .ok_or_else(|| "expression did not evaluate to a number".to_owned())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Resolve a parameter value: `=`-prefixed strings have their `{{ }}` segments
/// evaluated; everything else is returned as a literal string.
pub(crate) fn resolve(param: Option<&Value>, fallback: &str, context: &ExprContext) -> String {
    let raw = param.and_then(Value::as_str).unwrap_or(fallback);
    let Some(body) = raw.strip_prefix('=') else {
        return raw.to_owned();
    };
    let mut out = String::new();
    let mut rest = body;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                out.push_str(&evaluate(after[..end].trim(), context));
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
