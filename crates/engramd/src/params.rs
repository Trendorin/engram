//! Lenient parameter access: agents send numbers as strings and vice versa.

use engram_proto::RpcError;
use serde_json::Value;

pub struct P<'a>(pub &'a Value);

impl P<'_> {
    pub fn str(&self, k: &str) -> Option<String> {
        match self.0.get(k)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    pub fn req_str(&self, k: &str) -> Result<String, RpcError> {
        self.str(k).filter(|s| !s.is_empty()).ok_or_else(|| RpcError::new("invalid_argument", format!("missing '{k}'")))
    }

    pub fn int(&self, k: &str) -> Option<i64> {
        match self.0.get(k)? {
            Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            Value::String(s) => {
                let s = s.trim();
                s.strip_prefix("0x").map_or_else(|| s.parse().ok(), |h| i64::from_str_radix(h, 16).ok())
            }
            _ => None,
        }
    }

    pub fn int_or(&self, k: &str, d: i64, lo: i64, hi: i64) -> i64 {
        self.int(k).unwrap_or(d).clamp(lo, hi)
    }

    pub fn bool(&self, k: &str) -> Option<bool> {
        match self.0.get(k)? {
            Value::Bool(b) => Some(*b),
            Value::String(s) => match s.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => None,
            },
            Value::Number(n) => Some(n.as_i64() != Some(0)),
            _ => None,
        }
    }
}
