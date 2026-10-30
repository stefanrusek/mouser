//! The control client when the engine goes away mid-call: an error the
//! caller can act on, never a panic (T533).

use mouser_proto::control::Method;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::control::client::{ControlClient, GONE};

#[tokio::test]
async fn an_engine_that_closes_before_it_answers_is_an_error() {
    let (ours, mut theirs) = tokio::io::duplex(4096);
    let client = tokio::spawn(async move {
        let mut client = ControlClient::over(Box::new(ours));
        client.call(Method::Status, None).await
    });
    // The engine reads the request and goes.
    let mut request = [0u8; 64];
    let _ = theirs.read(&mut request).await.unwrap();
    drop(theirs);
    let error = client.await.unwrap().expect_err("no answer came");
    assert_eq!(error.code, -32603);
    assert_eq!(error.message, GONE);
}

#[tokio::test]
async fn an_engine_that_is_already_gone_is_an_error() {
    let (ours, theirs) = tokio::io::duplex(4096);
    drop(theirs);
    let mut client = ControlClient::over(Box::new(ours));
    let error = client.call(Method::Status, None).await.expect_err("gone");
    assert_eq!(error.message, GONE);
}

#[tokio::test]
async fn an_answer_still_arrives_as_before() {
    let (ours, mut theirs) = tokio::io::duplex(4096);
    let client = tokio::spawn(async move {
        let mut client = ControlClient::over(Box::new(ours));
        client.call(Method::Status, None).await
    });
    let mut request = [0u8; 256];
    let _ = theirs.read(&mut request).await.unwrap();
    theirs
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n")
        .await
        .unwrap();
    let answer = client.await.unwrap().unwrap();
    assert_eq!(answer["ok"], true);
}
