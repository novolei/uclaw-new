use crate::automation::protocol::humane_v1::FilterRule;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;

static REGEX_CACHE: Lazy<Mutex<HashMap<String, Regex>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn evaluate(rules: &[FilterRule], ctx: &serde_json::Value) -> bool {
    rules.iter().all(|r| eval_one(r, ctx))
}

fn eval_one(r: &FilterRule, ctx: &serde_json::Value) -> bool {
    let actual = ctx.pointer(&r.field);
    match (r.op.as_str(), actual) {
        ("eq", Some(v)) => v == &r.value,
        ("ne", Some(v)) => v != &r.value,
        ("contains", Some(serde_json::Value::String(s))) => {
            r.value.as_str().map_or(false, |needle| s.contains(needle))
        }
        ("matches", Some(serde_json::Value::String(s))) => {
            let pat = match r.value.as_str() { Some(p) => p, None => return false };
            let mut cache = REGEX_CACHE.lock().unwrap();
            let re = cache.entry(pat.to_string())
                .or_insert_with(|| Regex::new(pat).unwrap_or_else(|_| Regex::new("^$").unwrap()));
            re.is_match(s)
        }
        ("gt", Some(a)) | ("lt", Some(a)) => {
            let (Some(an), Some(bn)) = (a.as_f64(), r.value.as_f64()) else { return false };
            if r.op == "gt" { an > bn } else { an < bn }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(field: &str, op: &str, value: serde_json::Value) -> FilterRule {
        FilterRule { field: field.into(), op: op.into(), value }
    }

    #[test]
    fn eq_pass() {
        let ctx = json!({"event": {"branch": "main"}});
        assert!(evaluate(&[rule("/event/branch", "eq", json!("main"))], &ctx));
    }

    #[test]
    fn eq_fail() {
        let ctx = json!({"event": {"branch": "feature"}});
        assert!(!evaluate(&[rule("/event/branch", "eq", json!("main"))], &ctx));
    }

    #[test]
    fn contains_pass() {
        let ctx = json!({"title": "fix bug in widget"});
        assert!(evaluate(&[rule("/title", "contains", json!("bug"))], &ctx));
    }

    #[test]
    fn matches_regex_pass() {
        let ctx = json!({"branch": "release/v1.2.3"});
        assert!(evaluate(&[rule("/branch", "matches", json!("^release/"))], &ctx));
    }

    #[test]
    fn gt_numeric_pass() {
        let ctx = json!({"count": 10});
        assert!(evaluate(&[rule("/count", "gt", json!(5))], &ctx));
    }

    #[test]
    fn unknown_op_fails_closed() {
        let ctx = json!({"x": 1});
        assert!(!evaluate(&[rule("/x", "exists", json!(true))], &ctx));
    }

    #[test]
    fn all_rules_must_pass() {
        let ctx = json!({"a": 1, "b": 2});
        assert!(evaluate(&[
            rule("/a", "eq", json!(1)),
            rule("/b", "eq", json!(2)),
        ], &ctx));
        assert!(!evaluate(&[
            rule("/a", "eq", json!(1)),
            rule("/b", "eq", json!(99)),
        ], &ctx));
    }
}
