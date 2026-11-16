//! Read-only: prints what Codex's shared server reports for each loaded thread.
use onlydiffs_core::services::codex_app_server::AppServer;
use serde_json::json;

#[tokio::main]
async fn main() {
    let mut server = AppServer::connect().await.expect("connect to the Codex daemon");
    let listed = server.request("thread/loaded/list", json!({})).await.expect("list");
    println!("loaded: {listed}");
    for id in listed["data"].as_array().cloned().unwrap_or_default() {
        let read = server.request("thread/read", json!({ "threadId": id })).await;
        match read {
            Ok(value) => {
                let mut thread = value["thread"].clone();
                // Keep the output to the fields that decide routing.
                if let Some(map) = thread.as_object_mut() { map.remove("turns"); map.remove("preview"); }
                println!("{}", serde_json::to_string_pretty(&thread).unwrap());
            }
            Err(error) => println!("{id}: {}", error.message()),
        }
    }
}
