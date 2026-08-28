use crate::protocol::{
    IntentAbortParams, IntentAmendParams, IntentProposeParams, IntentSealParams, JsonRpcError,
    JsonRpcRequest, JsonRpcResponse, ToolCallParams,
};
use crate::session::SessionManager;
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde_json::json;
use std::sync::Arc;

pub fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/", post(handle_json_rpc))
        .with_state(session_manager)
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
                Err(err) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32001,
                        message: err,
                        data: None,
                    }),
                }),
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
                Err((code, msg, data)) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code,
                        message: msg,
                        data,
                    }),
                }),
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
                Err(message) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32004,
                        message,
                        data: None,
                    }),
                }),
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
                Err(err) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32001,
                        message: err,
                        data: None,
                    }),
                }),
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
                Err(err) => Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32001,
                        message: err,
                        data: None,
                    }),
                }),
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
