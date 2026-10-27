//! A control-API client: what the CLI and tests use to talk to an engine.

use std::collections::VecDeque;
use std::path::Path;

use mouser_proto::control::{
    LogsChunk, Method, Notification, Request, Response, RpcError, SubscribeParams, Topic,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::control::server::Duplex;

/// One connection to a running engine's control endpoint.
pub struct ControlClient {
    read: Box<dyn AsyncRead + Send + Unpin>,
    write: Box<dyn AsyncWrite + Send + Unpin>,
    buf: Vec<u8>,
    next_id: u64,
    notifications: VecDeque<Notification>,
    /// `logs.chunk` notifications, which are not on a topic, in the order
    /// they arrived.
    chunks: Vec<LogsChunk>,
}

#[cfg(unix)]
pub(crate) async fn open(path: &Path) -> std::io::Result<Box<dyn Duplex>> {
    Ok(Box::new(tokio::net::UnixStream::connect(path).await?))
}

#[cfg(windows)]
pub(crate) async fn open(path: &Path) -> std::io::Result<Box<dyn Duplex>> {
    use tokio::net::windows::named_pipe::ClientOptions;
    /// `ERROR_PIPE_BUSY`: every instance is serving a client; one is created
    /// again within moments, so wait and retry.
    const PIPE_BUSY: i32 = 231;
    let name = crate::control::server::pipe_name(path);
    loop {
        match ClientOptions::new().open(&name) {
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            other => return Ok(Box::new(other?)),
        }
    }
}

impl ControlClient {
    /// Connect to the control socket (Unix) or pipe (Windows) at `path`.
    pub async fn connect(path: &Path) -> std::io::Result<Self> {
        let (read, write) = tokio::io::split(open(path).await?);
        Ok(Self {
            read: Box::new(read),
            write: Box::new(write),
            buf: Vec::new(),
            next_id: 0,
            notifications: VecDeque::new(),
            chunks: Vec::new(),
        })
    }

    /// The next line from the server; cancellation safe. `None` once closed.
    async fn next_line(&mut self) -> Option<String> {
        loop {
            if let Some(end) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=end).collect();
                return Some(String::from_utf8(line).expect("server sends UTF-8"));
            }
            if self
                .read
                .read_buf(&mut self.buf)
                .await
                .expect("control connection readable")
                == 0
            {
                return None;
            }
        }
    }

    /// Call `method`; notifications that arrive meanwhile are kept for
    /// [`next_notification`](Self::next_notification).
    pub async fn call(&mut self, method: Method, params: Option<Value>) -> Result<Value, RpcError> {
        self.next_id += 1;
        let id = self.next_id;
        let mut line =
            serde_json::to_string(&Request::new(id, method, params)).expect("request serializes");
        line.push('\n');
        self.write
            .write_all(line.as_bytes())
            .await
            .expect("control connection writable");
        loop {
            let line = self
                .next_line()
                .await
                .expect("server answers before closing");
            let value: Value = serde_json::from_str(&line).expect("server sends JSON");
            if value.get("id").is_none() {
                self.keep(value);
                continue;
            }
            let response: Response = serde_json::from_value(value).expect("response");
            assert_eq!(response.id, id, "answers arrive in order");
            return match response.error {
                Some(error) => Err(error),
                None => Ok(response
                    .result
                    .expect("a response has a result or an error")),
            };
        }
    }

    /// [`call`](Self::call) with typed params and result.
    pub async fn call_typed<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: Method,
        params: &P,
    ) -> Result<R, RpcError> {
        let params = serde_json::to_value(params).expect("params serialize");
        let result = self.call(method, Some(params)).await?;
        Ok(serde_json::from_value(result).expect("result matches its type"))
    }

    /// Ask for notifications on `topics`.
    pub async fn subscribe(&mut self, topics: Vec<Topic>) -> Result<(), RpcError> {
        let params = serde_json::to_value(SubscribeParams { topics }).expect("params serialize");
        self.call(Method::Subscribe, Some(params)).await.map(|_| ())
    }

    /// The next notification; `None` once the server closed the connection.
    /// Cancellation safe.
    pub async fn next_notification(&mut self) -> Option<Notification> {
        loop {
            if let Some(note) = self.notifications.pop_front() {
                return Some(note);
            }
            let line = self.next_line().await?;
            self.keep(serde_json::from_str(&line).expect("server sends JSON"));
        }
    }

    /// Keep a notification until it is asked for: a chunk of a log bundle
    /// apart, the rest in order.
    fn keep(&mut self, value: Value) {
        if value["method"] == LogsChunk::METHOD {
            self.chunks
                .push(serde_json::from_value(value["params"].clone()).expect("a log chunk"));
        } else {
            self.notifications
                .push_back(serde_json::from_value(value).expect("notification"));
        }
    }

    /// The chunks of a log bundle received so far (`logs.collect_stream`
    /// sends them before it answers), which are forgotten.
    pub fn take_chunks(&mut self) -> Vec<LogsChunk> {
        std::mem::take(&mut self.chunks)
    }
}
