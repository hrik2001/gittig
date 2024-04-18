use std::{io::{stdin, Read}, process::Stdio};
use tokio::io::AsyncWriteExt;

// Basic git server implementation
// Each repo needs these APIs
// GIT_REPO_URL/info/refs (GET)
// GIT_REPO_URL/git-receive-pack (POST)
// GIT_REPO_URL/git-upload-pack (POST)
use axum::{
    body::Bytes, extract::{
        Path,
        Query, Request
    }, http::{header, HeaderMap, StatusCode}, response::IntoResponse, routing::{
        get,
        post
    }, Router
};
use tokio::process::Command;

#[tokio::main]
async fn main() {
    println!("Gittig server initialized");

    let app = Router::new()
        .route("/", get(|| async {"hello world"}))
        .route("/repo.git", get(|| async {"hello world"}))
        .route("/repo.git/:service_name", post(service_handler))
        .route("/repo.git/info/refs", get(info_refs_handler));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn info_refs_handler() -> impl IntoResponse {
    let mut command = Command::new("git")
        .arg("upload-pack")
        .arg("--stateless-rpc")
        .arg("--advertise-refs")
        .arg(".")
        .output()
        .await
        .unwrap();

    let stdout = String::from_utf8_lossy(&command.stdout);

    let mut response = String::new();
    response.push_str("001e# service=git-upload-pack\n0000");
    response.push_str(&stdout);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/x-git-upload-pack-advertisement".parse().unwrap(),
    );

    (headers, response)
}

async fn service_handler(Path(service_name): Path<String>, body: String) -> impl IntoResponse {
    match service_name.as_str() {
        "git-receive-pack" | "git-upload-pack" => (),
        // implement a 404
        _ => return String::from("nope"),
    };
    println!("Action to execute: {}", &service_name.as_str()[4..]);
    let mut command = Command::new("git")
        .arg(&service_name[4..])
        .arg("--stateless-rpc")
        .arg(".")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn().ok().unwrap();
    command.stdin.as_mut().unwrap().write_all(body.as_bytes()).await.unwrap();
    // Close stdin to finish and avoid indefinite blocking
    // drop(child_stdin);
    let output = command.wait_with_output().await.unwrap();

    // let output_string_slice = String::from_utf8_lossy(&output.stdout).as_ref();
    let output_string = String::from(String::from_utf8_lossy(&output.stdout).as_ref());
    println!("Output from git: {}", output_string);
    output_string
}