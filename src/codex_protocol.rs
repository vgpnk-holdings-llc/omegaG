//! Small, schema-pinned Codex app-server v2 codec.
//!
//! The wire is newline-delimited JSON-RPC. These builders intentionally emit
//! only fields present in codex-cli 0.145.0-alpha.24's generated stable schema.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::BufRead;

pub const PINNED_CODEX_VERSION: &str = "0.145.0-alpha.24";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Inbound {
    #[serde(default)]
    pub id: Option<RequestId>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub error: Value,
}

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"id": id, "method": method, "params": params})
}

pub fn response(id: &RequestId, result: Value) -> Value {
    json!({"id": id, "result": result})
}

pub fn initialize(id: u64) -> Value {
    request(
        id,
        "initialize",
        json!({
            "clientInfo": {"name": "omegaG", "title": "omegaG controller", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"experimentalApi": false}
        }),
    )
}

pub fn initialized() -> Value {
    json!({"method": "initialized"})
}
pub fn model_list(id: u64) -> Value {
    request(id, "model/list", json!({}))
}
pub fn thread_list(id: u64) -> Value {
    request(
        id,
        "thread/list",
        json!({"limit": 64, "sortKey": "updated_at"}),
    )
}
pub fn skills_list(id: u64, cwd: &str) -> Value {
    request(
        id,
        "skills/list",
        json!({"cwds": [cwd], "forceReload": false}),
    )
}
pub fn thread_read(id: u64, thread_id: &str) -> Value {
    request(
        id,
        "thread/read",
        json!({"threadId": thread_id, "includeTurns": true}),
    )
}
pub fn thread_resume(id: u64, thread_id: &str) -> Value {
    request(id, "thread/resume", json!({"threadId": thread_id}))
}
pub fn thread_start(id: u64, cwd: Option<&str>) -> Value {
    let mut params = serde_json::Map::new();
    if let Some(cwd) = cwd.filter(|s| !s.is_empty()) {
        params.insert("cwd".into(), json!(cwd));
    }
    request(id, "thread/start", Value::Object(params))
}
pub fn thread_fork(id: u64, thread_id: &str) -> Value {
    request(id, "thread/fork", json!({"threadId": thread_id}))
}
pub fn turn_start(
    id: u64,
    thread_id: &str,
    input: Value,
    effort: Option<&str>,
    priority: bool,
) -> Value {
    let mut params = serde_json::Map::from_iter([
        ("threadId".into(), json!(thread_id)),
        ("input".into(), json!([input])),
    ]);
    if let Some(effort) = effort {
        params.insert("effort".into(), json!(effort));
    }
    if priority {
        params.insert("serviceTier".into(), json!("priority"));
    }
    request(id, "turn/start", Value::Object(params))
}
pub fn text_input(text: &str) -> Value {
    json!({"type": "text", "text": text, "text_elements": []})
}
pub fn skill_input(name: &str, path: &str) -> Value {
    json!({"type": "skill", "name": name, "path": path})
}

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    Io,
    Oversize,
    Malformed,
}

pub fn read_frame(reader: &mut impl BufRead) -> Result<Option<Inbound>, FrameError> {
    let mut bytes = Vec::new();
    let read = std::io::Read::take(reader, (MAX_FRAME_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| FrameError::Io)?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(FrameError::Oversize);
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| FrameError::Malformed)
}

pub fn encode_line(value: &Value) -> Vec<u8> {
    let mut out = serde_json::to_vec(value).expect("JSON value serializes");
    out.push(b'\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn handshake_is_exact() {
        assert_eq!(initialize(1)["method"], "initialize");
        assert_eq!(initialize(1)["params"]["clientInfo"]["name"], "omegaG");
        assert_eq!(initialized(), json!({"method":"initialized"}));
    }
    #[test]
    fn mutation_allowlists_are_minimal() {
        assert_eq!(thread_start(1, None)["params"], json!({}));
        assert_eq!(thread_fork(2, "t")["params"], json!({"threadId":"t"}));
        let turn = turn_start(3, "t", text_input("secret"), Some("high"), true);
        assert_eq!(turn["params"]["serviceTier"], "priority");
        assert_eq!(turn["params"]["effort"], "high");
        assert!(
            turn["params"].as_object().unwrap().keys().all(|k| [
                "threadId",
                "input",
                "effort",
                "serviceTier"
            ]
            .contains(&k.as_str()))
        );
    }
    #[test]
    fn approval_roundtrips_string_id_exactly() {
        let id = RequestId::String("opaque-7".into());
        assert_eq!(
            response(&id, json!({"decision":"accept"}))["id"],
            "opaque-7"
        );
    }
    #[test]
    fn approval_roundtrips_number_id_exactly() {
        let id = RequestId::Number(42);
        assert_eq!(response(&id, json!({"decision":"decline"}))["id"], 42);
    }
    #[test]
    fn frames_reject_malformed_and_oversize() {
        let mut bad = BufReader::new(Cursor::new(b"nope\n"));
        assert_eq!(read_frame(&mut bad), Err(FrameError::Malformed));
        let huge = vec![b'x'; MAX_FRAME_BYTES + 1];
        assert_eq!(
            read_frame(&mut BufReader::new(Cursor::new(huge))),
            Err(FrameError::Oversize)
        );
    }
    #[test]
    fn frame_limit_boundaries_are_exact() {
        fn frame(size: usize) -> Vec<u8> {
            let mut bytes = b"{}".to_vec();
            bytes.resize(size.saturating_sub(1), b' ');
            bytes.push(b'\n');
            bytes
        }
        for size in [MAX_FRAME_BYTES - 1, MAX_FRAME_BYTES] {
            assert!(
                read_frame(&mut BufReader::new(Cursor::new(frame(size))))
                    .unwrap()
                    .is_some()
            );
        }
        assert_eq!(
            read_frame(&mut BufReader::new(Cursor::new(frame(MAX_FRAME_BYTES + 1)))),
            Err(FrameError::Oversize)
        );
    }
    #[test]
    fn eof_is_clean() {
        assert_eq!(
            read_frame(&mut BufReader::new(Cursor::new(Vec::<u8>::new()))).unwrap(),
            None
        );
    }
    #[test]
    fn skill_identity_has_exact_name_and_path() {
        assert_eq!(
            skill_input("review", "/x/SKILL.md"),
            json!({"type":"skill","name":"review","path":"/x/SKILL.md"})
        );
    }
}
