// Basic git server implementation
// Each repo needs these APIs
// GIT_REPO_URL/info/refs?service= (GET)
// GIT_REPO_URL/git-receive-pack (POST)
// GIT_REPO_URL/git-upload-pack (POST)
use std::{io::{stdin, Read}, process::Stdio};
use tokio::io::AsyncWriteExt;
use serde::Deserialize;

use axum::{
    body::Bytes, extract::{
        Path,
        Query, Request
    }, http::{header, HeaderMap, StatusCode}, response::IntoResponse, routing::{
        get, head, post
    }, Router
};
use tokio::process::Command;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use base64::{engine::general_purpose::STANDARD, Engine as _};

#[tokio::main]
async fn main() {
    println!("Gittig server initialized");

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();


    let app = Router::new()
        .route("/", get(|| async {"hello world"}))
        .route("/repo.git", get(|| async {"hello world"}))
        .route("/repo.git/:service_name", post(service_handler))
        .route("/repo.git/info/refs", get(info_refs_handler))
        .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct InfoRefQueryParam {
    service: String,
}

async fn info_refs_handler(headers: HeaderMap, q: Query<InfoRefQueryParam>) -> impl IntoResponse {
    let InfoRefQueryParam { service } = q.0;
    let service_name = &service.to_string()[4..];  // strip the 'git-' prefix
    let mut hex_length = String::new();
    match service_name {
        "receive-pack" => hex_length.push_str("001f"),
        "upload-pack" => hex_length.push_str("001e"),
        // implement a 404
        _ => return (StatusCode::NOT_FOUND, HeaderMap::new(), "Not found".to_string()),
    };

    if !headers.contains_key("authorization") {
        let mut challenge_headers = HeaderMap::new();
        challenge_headers.insert(header::WWW_AUTHENTICATE, "Basic realm=\"Git Server\"".parse().unwrap());
        return (StatusCode::UNAUTHORIZED, challenge_headers, "Authentication required".to_string());
    }
    let encoded_token = headers["authorization"].clone().to_str().unwrap().to_string();

    if !encoded_token.starts_with("Basic") {
        // Making sure that Basic auth is followed
        let mut challenge_headers = HeaderMap::new();
        challenge_headers.insert(header::WWW_AUTHENTICATE, "Basic realm=\"Git Server\"".parse().unwrap());
        return (StatusCode::UNAUTHORIZED, challenge_headers, "Authentication required".to_string());
    }
    let encoded_part = &encoded_token[6..];
    let decoded_token_vector = &STANDARD.decode(encoded_part).unwrap();
    let token = String::from_utf8_lossy(decoded_token_vector);
    let mut split_creds = token.split(':');
    let username = split_creds.next().unwrap();
    let password = split_creds.next().unwrap();
    
    let command = Command::new("git")
        .arg(service_name)
        .arg("--stateless-rpc")
        .arg("--advertise-refs")
        .arg(".")
        .output()
        .await
        .unwrap();

    let stdout = String::from_utf8_lossy(&command.stdout);

    let response_content = format!("# service=git-{}\n0000", service_name);

    let response = format!("{}{}{}", hex_length, response_content, stdout);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        format!("application/x-git-{}-advertisement", service_name).parse().unwrap(),
    );

    (StatusCode::OK, headers, response)
}

async fn service_handler(Path(service_name): Path<String>, body: Bytes) -> impl IntoResponse {
    match service_name.as_str() {
        "git-receive-pack" | "git-upload-pack" => (),
        // implement a 404
        _ => return (StatusCode::NOT_FOUND, HeaderMap::new(), b"Not found".to_vec()),
    };
    let mut command = Command::new("git")
        .arg(&service_name[4..])
        .arg("--stateless-rpc")
        .arg(".")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn().ok().unwrap();
    command.stdin.as_mut().unwrap().write_all(&body).await.unwrap();

    let output = command.wait_with_output().await.unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        format!("application/x-git-{}-result",&service_name.as_str()[4..]).parse().unwrap(),
    );

    (StatusCode::OK ,headers, output.stdout)

}