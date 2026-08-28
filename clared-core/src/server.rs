use crate::protocol::{
    IntentAbortParams, IntentAmendParams, IntentProposeParams, IntentSealParams, JsonRpcError,
    JsonRpcRequest, JsonRpcResponse, ToolCallParams,
};
use crate::session::{SessionError, SessionManager};
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/", post(handle_json_rpc))
        .with_state(session_manager)
}

fn session_failure(id: Value, error: SessionError) -> Json<JsonRpcResponse> {
    Json(JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code: error.code,
            message: error.message,
            data: error.data,
        }),
    })
}

async fn handle_json_rpc(
    State(session_mgr): State<Arc<SessionManager>>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let method = req.method.as_str();

    match method {
        "intent/propose" => {
            let params = match req
                .params
                .and_then(|p| serde_json::from_value::<IntentProposeParams>(p).ok())
            {
                Some(p) => p,
                None => {
                    return Json(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params for intent/propose".to_string(),
                            data: None,
                        }),
                    });
                }
            };

            match session_mgr.propose(params) {
                Ok(res) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(json!(res)),
                    error: None,
                }),
                Err(error) => session_failure(req.id, error),
            }
        }
        "tools/call" => {
            let params = match req
                .params
                .and_then(|p| serde_json::from_value::<ToolCallParams>(p).ok())
            {
                Some(p) => p,
                None => {
                    return Json(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params for tools/call".to_string(),
                            data: None,
                        }),
                    });
                }
            };

            match session_mgr.execute_tool(params) {
                Ok(res) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(res),
                    error: None,
                }),
                Err(error) => session_failure(req.id, error),
            }
        }
        "intent/amend" => {
            let params = match req
                .params
                .and_then(|value| serde_json::from_value::<IntentAmendParams>(value).ok())
            {
                Some(params) => params,
                None => {
                    return Json(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params for intent/amend".to_string(),
                            data: None,
                        }),
                    });
                }
            };

            match session_mgr.amend(params) {
                Ok(result) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(result),
                    error: None,
                }),
                Err(error) => session_failure(req.id, error),
            }
        }
        "intent/seal" => {
            let params = match req
                .params
                .and_then(|p| serde_json::from_value::<IntentSealParams>(p).ok())
            {
                Some(p) => p,
                None => {
                    return Json(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params for intent/seal".to_string(),
                            data: None,
                        }),
                    });
                }
            };

            match session_mgr.seal(params) {
                Ok(res) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(res),
                    error: None,
                }),
                Err(error) => session_failure(req.id, error),
            }
        }
        "intent/abort" => {
            let params = match req
                .params
                .and_then(|p| serde_json::from_value::<IntentAbortParams>(p).ok())
            {
                Some(p) => p,
                None => {
                    return Json(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params for intent/abort".to_string(),
                            data: None,
                        }),
                    });
                }
            };

            match session_mgr.abort(params) {
                Ok(res) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(res),
                    error: None,
                }),
                Err(error) => session_failure(req.id, error),
            }
        }
        _ => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: req.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method '{}' not found", method),
                data: None,
            }),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_errors_preserve_protocol_code_and_data() {
        let lifecycle_code = crate::protocol::error_code::INVALID_LIFECYCLE;
        let response = session_failure(
            json!("request-1"),
            SessionError {
                code: lifecycle_code,
                message: "invalid lifecycle".to_string(),
                data: Some(json!({"status": "SETTLED"})),
            },
        )
        .0;
        let error = response.error.expect("response should contain an error");
        assert_eq!(error.code, lifecycle_code);
        assert_eq!(error.data, Some(json!({"status": "SETTLED"})));
    }
}
