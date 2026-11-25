use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::sandbox::{ProcessJobGuard, apply_sanitized_env};

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("failed to spawn process: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("JSON-RPC remote error ({code}): {message}")]
    Remote { code: i64, message: String },
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
    #[error("child process transport closed unexpectedly")]
    TransportClosed,
}

#[derive(Debug, Clone)]
pub enum IncomingEvent {
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
}

type PendingResponseMap = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, RpcError>>>>>;

#[derive(Clone)]
pub struct RpcClient {
    stdin: Arc<Mutex<ChildStdin>>,
    pending: PendingResponseMap,
    next_id: Arc<AtomicI64>,
}

impl RpcClient {
    pub async fn send_request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        self.write_json(&payload).await?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(RpcError::TransportClosed),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(RpcError::Timeout(timeout))
            }
        }
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), RpcError> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.write_json(&payload).await
    }

    pub async fn send_server_response(&self, id: Value, result: Value) -> Result<(), RpcError> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        });
        self.write_json(&payload).await
    }

    async fn write_json(&self, value: &Value) -> Result<(), RpcError> {
        let mut line = serde_json::to_string(value)?;
        line.push('\n');
        let mut guard = self.stdin.lock().await;
        guard.write_all(line.as_bytes()).await?;
        guard.flush().await?;
        Ok(())
    }
}

pub struct JsonRpcProcess {
    child: Child,
    _job_guard: Option<ProcessJobGuard>,
    pub client: RpcClient,
    pub events_rx: mpsc::UnboundedReceiver<IncomingEvent>,
    pub stderr_lines: Arc<Mutex<Vec<String>>>,
}

impl JsonRpcProcess {
    pub fn spawn(program: &str, args: &[String], cwd: &std::path::Path) -> Result<Self, RpcError> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        apply_sanitized_env(&mut cmd);

        let mut child = cmd.spawn()?;
        let job_guard = match ProcessJobGuard::new() {
            Ok(guard) => {
                if let Some(pid) = child.id() {
                    let _ = guard.assign_pid(pid);
                }
                Some(guard)
            }
            Err(_) => None,
        };

        let stdin = child.stdin.take().ok_or(RpcError::TransportClosed)?;
        let stdout = child.stdout.take().ok_or(RpcError::TransportClosed)?;
        let stderr = child.stderr.take().ok_or(RpcError::TransportClosed)?;

        let stdin_arc = Arc::new(Mutex::new(stdin));
        let pending: PendingResponseMap = Arc::new(Mutex::new(HashMap::new()));
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let stderr_lines = Arc::new(Mutex::new(Vec::new()));

        // Background stdout reader
        let pending_bg = Arc::clone(&pending);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                    continue;
                };

                let has_method = msg.get("method").and_then(Value::as_str).is_some();
                let has_id = msg.get("id").is_some();

                if has_method && has_id {
                    let method = msg["method"].as_str().unwrap_or_default().to_string();
                    let params = msg.get("params").cloned().unwrap_or(Value::Null);
                    let _ = events_tx.send(IncomingEvent::ServerRequest {
                        id: msg["id"].clone(),
                        method,
                        params,
                    });
                } else if has_method {
                    let method = msg["method"].as_str().unwrap_or_default().to_string();
                    let params = msg.get("params").cloned().unwrap_or(Value::Null);
                    let _ = events_tx.send(IncomingEvent::Notification { method, params });
                } else if has_id
                    && let Some(id_num) = msg["id"].as_i64()
                    && let Some(tx) = pending_bg.lock().await.remove(&id_num)
                {
                    if let Some(err) = msg.get("error") {
                        let code = err.get("code").and_then(Value::as_i64).unwrap_or(-1);
                        let message = err
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown RPC error")
                            .to_string();
                        let _ = tx.send(Err(RpcError::Remote { code, message }));
                    } else {
                        let res = msg.get("result").cloned().unwrap_or(Value::Null);
                        let _ = tx.send(Ok(res));
                    }
                }
            }
            // Fail any remaining waiters on EOF
            let mut map = pending_bg.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(RpcError::TransportClosed));
            }
        });

        // Background stderr collector (capped to 200 lines)
        let stderr_bg = Arc::clone(&stderr_lines);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let mut guard = stderr_bg.lock().await;
                if guard.len() < 200 {
                    guard.push(line);
                }
            }
        });

        Ok(Self {
            child,
            _job_guard: job_guard,
            client: RpcClient {
                stdin: stdin_arc,
                pending,
                next_id: Arc::new(AtomicI64::new(1)),
            },
            events_rx,
            stderr_lines,
        })
    }

    pub async fn send_request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        self.client.send_request(method, params, timeout).await
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), RpcError> {
        self.client.send_notification(method, params).await
    }

    pub async fn send_server_response(&self, id: Value, result: Value) -> Result<(), RpcError> {
        self.client.send_server_response(id, result).await
    }

    pub async fn shutdown(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}
