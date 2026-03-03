use std::io::{self, BufRead, Write};
use std::time::Instant;

use anyhow::{Result, bail};
use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

use crate::cli::{McpServeArgs, ToolExecArgs};
use crate::tools::{ToolOutput, ToolSpec};

pub const MCP_PROTOCOL_VERSION: &str = "meow.mcp.v1";

#[derive(Debug, Clone, Copy)]
enum McpErrorCode {
    InvalidJson,
    InvalidRequest,
    UnsupportedVersion,
    UnknownMethod,
    UnknownTool,
    ApprovalRequired,
    PolicyDenied,
    ToolExecution,
}

impl McpErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidJson => "invalid_json",
            Self::InvalidRequest => "invalid_request",
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnknownMethod => "unknown_method",
            Self::UnknownTool => "unknown_tool",
            Self::ApprovalRequired => "approval_required",
            Self::PolicyDenied => "policy_denied",
            Self::ToolExecution => "tool_execution_error",
        }
    }
}

#[derive(Debug, Serialize)]
struct McpErrorPayload {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct McpMeta {
    request_id: String,
    method: String,
    timestamp: String,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    version: &'static str,
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpErrorPayload>,
    meta: McpMeta,
}

#[derive(Debug)]
enum ParsedMethod {
    Ping,
    ServerInfo,
    ToolsList,
    ToolsCall {
        tool: String,
        args: Vec<String>,
        approve: bool,
    },
}

#[derive(Debug)]
struct ParsedRequest {
    id: Option<String>,
    request_id: String,
    method_name: String,
    method: ParsedMethod,
}

pub fn serve_stdio<FCall, FList>(
    args: McpServeArgs,
    mut call_tool: FCall,
    mut list_tools: FList,
) -> Result<()>
where
    FCall: FnMut(ToolExecArgs) -> Result<ToolOutput>,
    FList: FnMut() -> Vec<ToolSpec>,
{
    if args.transport != "stdio" {
        bail!("only stdio transport is supported");
    }

    eprintln!(
        "meow mcp stdio server started (protocol={}): one JSON request per line",
        MCP_PROTOCOL_VERSION
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for (idx, line) in stdin.lock().lines().enumerate() {
        let raw = line?;
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            continue;
        }

        if matches!(trimmed, "exit" | "quit") {
            break;
        }

        let response =
            process_input_line(trimmed, (idx + 1) as u64, &mut call_tool, &mut list_tools);
        emit_log(&response);

        let wire = serde_json::to_string(&response)?;
        writeln!(stdout, "{wire}")?;
        stdout.flush()?;
    }

    Ok(())
}

fn process_input_line<FCall, FList>(
    line: &str,
    line_no: u64,
    call_tool: &mut FCall,
    list_tools: &mut FList,
) -> McpResponse
where
    FCall: FnMut(ToolExecArgs) -> Result<ToolOutput>,
    FList: FnMut() -> Vec<ToolSpec>,
{
    let started = Instant::now();
    let parsed = parse_request(line, line_no);

    match parsed {
        Ok(request) => {
            let result = match request.method {
                ParsedMethod::Ping => Ok(json!({"pong": true})),
                ParsedMethod::ServerInfo => Ok(json!({
                    "name": "meow-mcp",
                    "runtime": "meow-soma",
                    "version": env!("CARGO_PKG_VERSION"),
                    "protocol": MCP_PROTOCOL_VERSION,
                    "transport": "stdio",
                })),
                ParsedMethod::ToolsList => Ok(json!({
                    "tools": list_tools(),
                })),
                ParsedMethod::ToolsCall {
                    tool,
                    args,
                    approve,
                } => {
                    let outcome = call_tool(ToolExecArgs {
                        name: tool,
                        args,
                        approve,
                    });

                    match outcome {
                        Ok(output) => Ok(json!({ "output": output })),
                        Err(err) => Err(map_tool_error(err.to_string())),
                    }
                }
            };

            match result {
                Ok(payload) => ok_response(
                    request.id,
                    request.request_id,
                    request.method_name,
                    payload,
                    started.elapsed().as_millis(),
                ),
                Err((code, message)) => err_response(
                    request.id,
                    request.request_id,
                    request.method_name,
                    code,
                    message,
                    started.elapsed().as_millis(),
                ),
            }
        }
        Err((id, request_id, method_name, code, message)) => err_response(
            id,
            request_id,
            method_name,
            code,
            message,
            started.elapsed().as_millis(),
        ),
    }
}

fn parse_request(line: &str, line_no: u64) -> std::result::Result<ParsedRequest, ParseErrorTuple> {
    let raw: Value = serde_json::from_str(line).map_err(|err| {
        (
            None,
            fallback_request_id(None, line_no),
            "parse".to_owned(),
            McpErrorCode::InvalidJson,
            format!("invalid JSON request: {err}"),
        )
    })?;

    let id = parse_optional_id(raw.get("id")).map_err(|message| {
        (
            None,
            fallback_request_id(None, line_no),
            "parse".to_owned(),
            McpErrorCode::InvalidRequest,
            message,
        )
    })?;
    let request_id = fallback_request_id(id.as_deref(), line_no);

    if let Some(version) = raw.get("version").and_then(Value::as_str)
        && version != MCP_PROTOCOL_VERSION
    {
        return Err((
            id,
            request_id,
            "parse".to_owned(),
            McpErrorCode::UnsupportedVersion,
            format!("unsupported version '{version}', expected '{MCP_PROTOCOL_VERSION}'"),
        ));
    }

    if let Some(method) = raw.get("method").and_then(Value::as_str) {
        let params = raw.get("params");
        return parse_method(id, request_id, method, params);
    }

    if raw.get("tool").is_some() {
        let tool = raw
            .get("tool")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                (
                    id.clone(),
                    request_id.clone(),
                    "tools/call".to_owned(),
                    McpErrorCode::InvalidRequest,
                    "legacy request requires string field 'tool'".to_owned(),
                )
            })?;

        let args = extract_args_array(raw.get("args")).map_err(|message| {
            (
                id.clone(),
                request_id.clone(),
                "tools/call".to_owned(),
                McpErrorCode::InvalidRequest,
                message,
            )
        })?;
        let approve = raw.get("approve").and_then(Value::as_bool).unwrap_or(false);

        return Ok(ParsedRequest {
            id,
            request_id,
            method_name: "tools/call".to_owned(),
            method: ParsedMethod::ToolsCall {
                tool,
                args,
                approve,
            },
        });
    }

    Err((
        id,
        request_id,
        "parse".to_owned(),
        McpErrorCode::InvalidRequest,
        "request must include either 'method' or legacy 'tool' field".to_owned(),
    ))
}

type ParseErrorTuple = (Option<String>, String, String, McpErrorCode, String);

fn parse_method(
    id: Option<String>,
    request_id: String,
    method: &str,
    params: Option<&Value>,
) -> std::result::Result<ParsedRequest, ParseErrorTuple> {
    match method {
        "ping" => Ok(ParsedRequest {
            id,
            request_id,
            method_name: "ping".to_owned(),
            method: ParsedMethod::Ping,
        }),
        "server/info" => Ok(ParsedRequest {
            id,
            request_id,
            method_name: "server/info".to_owned(),
            method: ParsedMethod::ServerInfo,
        }),
        "tools/list" => Ok(ParsedRequest {
            id,
            request_id,
            method_name: "tools/list".to_owned(),
            method: ParsedMethod::ToolsList,
        }),
        "tools/call" => {
            let Some(params_obj) = params.and_then(Value::as_object) else {
                return Err((
                    id,
                    request_id,
                    "tools/call".to_owned(),
                    McpErrorCode::InvalidRequest,
                    "method 'tools/call' requires object field 'params'".to_owned(),
                ));
            };

            let tool = params_obj
                .get("tool")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    (
                        id.clone(),
                        request_id.clone(),
                        "tools/call".to_owned(),
                        McpErrorCode::InvalidRequest,
                        "params.tool must be a string".to_owned(),
                    )
                })?;

            let args = extract_args_array(params_obj.get("args")).map_err(|message| {
                (
                    id.clone(),
                    request_id.clone(),
                    "tools/call".to_owned(),
                    McpErrorCode::InvalidRequest,
                    message,
                )
            })?;
            let approve = params_obj
                .get("approve")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            Ok(ParsedRequest {
                id,
                request_id,
                method_name: "tools/call".to_owned(),
                method: ParsedMethod::ToolsCall {
                    tool,
                    args,
                    approve,
                },
            })
        }
        unknown => Err((
            id,
            request_id,
            method.to_owned(),
            McpErrorCode::UnknownMethod,
            format!("unknown method: {unknown}"),
        )),
    }
}

fn parse_optional_id(value: Option<&Value>) -> std::result::Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Number(number)) => Ok(Some(number.to_string())),
        Some(Value::Bool(flag)) => Ok(Some(flag.to_string())),
        _ => Err("field 'id' must be string/number/bool/null".to_owned()),
    }
}

fn fallback_request_id(id: Option<&str>, line_no: u64) -> String {
    id.map(str::to_owned)
        .unwrap_or_else(|| format!("line-{line_no}"))
}

fn extract_args_array(value: Option<&Value>) -> std::result::Result<Vec<String>, String> {
    let Some(candidate) = value else {
        return Ok(Vec::new());
    };

    let Some(items) = candidate.as_array() else {
        return Err("args must be an array of strings".to_owned());
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Some(text) = item.as_str() else {
            return Err("args must be an array of strings".to_owned());
        };
        out.push(text.to_owned());
    }

    Ok(out)
}

fn map_tool_error(message: String) -> (McpErrorCode, String) {
    if message.contains("unknown tool") {
        return (McpErrorCode::UnknownTool, message);
    }
    if message.contains("requires approval") {
        return (McpErrorCode::ApprovalRequired, message);
    }
    if message.contains("execution denied") || message.contains("tool execution denied") {
        return (McpErrorCode::PolicyDenied, message);
    }
    (McpErrorCode::ToolExecution, message)
}

fn ok_response(
    id: Option<String>,
    request_id: String,
    method: String,
    result: Value,
    duration_ms: u128,
) -> McpResponse {
    McpResponse {
        version: MCP_PROTOCOL_VERSION,
        id,
        ok: true,
        result: Some(result),
        error: None,
        meta: McpMeta {
            request_id,
            method,
            timestamp: Utc::now().to_rfc3339(),
            duration_ms,
        },
    }
}

fn err_response(
    id: Option<String>,
    request_id: String,
    method: String,
    code: McpErrorCode,
    message: String,
    duration_ms: u128,
) -> McpResponse {
    McpResponse {
        version: MCP_PROTOCOL_VERSION,
        id,
        ok: false,
        result: None,
        error: Some(McpErrorPayload {
            code: code.as_str().to_owned(),
            message,
        }),
        meta: McpMeta {
            request_id,
            method,
            timestamp: Utc::now().to_rfc3339(),
            duration_ms,
        },
    }
}

#[derive(Debug, Serialize)]
struct McpLogLine<'a> {
    ts: String,
    request_id: &'a str,
    method: &'a str,
    ok: bool,
    error_code: Option<&'a str>,
    duration_ms: u128,
}

fn emit_log(response: &McpResponse) {
    let error_code = response.error.as_ref().map(|err| err.code.as_str());
    let line = McpLogLine {
        ts: Utc::now().to_rfc3339(),
        request_id: &response.meta.request_id,
        method: &response.meta.method,
        ok: response.ok,
        error_code,
        duration_ms: response.meta.duration_ms,
    };

    match serde_json::to_string(&line) {
        Ok(text) => eprintln!("[mcp] {text}"),
        Err(_) => eprintln!(
            "[mcp] request_id={} method={} ok={} duration_ms={}",
            response.meta.request_id, response.meta.method, response.ok, response.meta.duration_ms
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_tool_request_executes() {
        let mut call_tool = |args: ToolExecArgs| -> Result<ToolOutput> {
            Ok(ToolOutput {
                status: "ok".to_owned(),
                stdout: format!("{} {}", args.name, args.args.join(" ")),
                stderr: String::new(),
            })
        };
        let mut list_tools = || Vec::<ToolSpec>::new();

        let response = process_input_line(
            r#"{"id":"abc","tool":"echo","args":["hi"]}"#,
            1,
            &mut call_tool,
            &mut list_tools,
        );

        assert!(response.ok);
        assert_eq!(response.meta.method, "tools/call");
        assert_eq!(response.id.as_deref(), Some("abc"));
    }

    #[test]
    fn tools_list_returns_discovery_payload() {
        let mut call_tool = |_args: ToolExecArgs| -> Result<ToolOutput> {
            Ok(ToolOutput {
                status: "ok".to_owned(),
                stdout: String::new(),
                stderr: String::new(),
            })
        };
        let mut list_tools = || {
            vec![ToolSpec {
                name: "echo".to_owned(),
                description: "Echo".to_owned(),
                risky: false,
            }]
        };

        let response = process_input_line(
            r#"{"version":"meow.mcp.v1","id":"2","method":"tools/list"}"#,
            2,
            &mut call_tool,
            &mut list_tools,
        );

        assert!(response.ok);
        let tools_len = response
            .result
            .as_ref()
            .and_then(|value| value.get("tools"))
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        assert_eq!(tools_len, 1);
    }

    #[test]
    fn malformed_json_returns_invalid_json_code() {
        let mut call_tool = |_args: ToolExecArgs| -> Result<ToolOutput> {
            Ok(ToolOutput {
                status: "ok".to_owned(),
                stdout: String::new(),
                stderr: String::new(),
            })
        };
        let mut list_tools = || Vec::<ToolSpec>::new();

        let response = process_input_line("{", 3, &mut call_tool, &mut list_tools);
        assert!(!response.ok);
        assert_eq!(
            response.error.as_ref().map(|item| item.code.as_str()),
            Some("invalid_json")
        );
    }

    #[test]
    fn unknown_method_returns_error_code() {
        let mut call_tool = |_args: ToolExecArgs| -> Result<ToolOutput> {
            Ok(ToolOutput {
                status: "ok".to_owned(),
                stdout: String::new(),
                stderr: String::new(),
            })
        };
        let mut list_tools = || Vec::<ToolSpec>::new();

        let response = process_input_line(
            r#"{"id":"x","method":"tools/unknown"}"#,
            4,
            &mut call_tool,
            &mut list_tools,
        );

        assert!(!response.ok);
        assert_eq!(
            response.error.as_ref().map(|item| item.code.as_str()),
            Some("unknown_method")
        );
    }

    #[test]
    fn approval_required_is_mapped_to_protocol_error() {
        let mut call_tool = |_args: ToolExecArgs| -> Result<ToolOutput> {
            bail!("tool execution requires approval (outside allowlist) - re-run with --approve")
        };
        let mut list_tools = || Vec::<ToolSpec>::new();

        let response = process_input_line(
            r#"{"id":"r1","method":"tools/call","params":{"tool":"shell","args":["git","push"]}}"#,
            5,
            &mut call_tool,
            &mut list_tools,
        );

        assert!(!response.ok);
        assert_eq!(
            response.error.as_ref().map(|item| item.code.as_str()),
            Some("approval_required")
        );
    }
}
