//! Offline HTTP-boundary coverage for MiniMax defaults and explicit model IDs.
use openfang_runtime::drivers::create_driver;
use openfang_runtime::llm_driver::{CompletionRequest, DriverConfig};
use openfang_runtime::model_catalog::ModelCatalog;
use openfang_types::message::{ContentBlock, Message, MessageContent, Role};
use openfang_types::model_catalog::ModelTier;
use openfang_types::tool::ToolDefinition;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn minimax_choices_preserve_legacy_lookup_and_custom_models() {
    let mut catalog = ModelCatalog::new();
    let current = ["MiniMax-M3.1-Flash", "MiniMax-M3"];
    assert_eq!(
        catalog
            .models_by_provider("minimax")
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        current
    );
    assert_eq!(catalog.get_provider("minimax").unwrap().model_count, 2);
    assert_eq!(
        catalog.default_model_for_provider("minimax").unwrap(),
        current[0]
    );
    assert_eq!(catalog.resolve_alias("minimax"), Some(current[0]));
    for model in catalog
        .list_models()
        .iter()
        .filter(|m| m.provider == "minimax")
    {
        assert!(current.contains(&model.id.as_str()));
    }
    for id in [
        "MiniMax-M2.7",
        "minimax-m2.5",
        "minimax-highspeed",
        "abab7",
        "minimax-text-01",
    ] {
        assert_eq!(
            catalog
                .find_model_for_provider(id, "minimax")
                .unwrap()
                .provider,
            "minimax"
        );
    }
    let mut custom = catalog.find_model("MiniMax-M2.7").unwrap().clone();
    custom.tier = ModelTier::Custom;
    custom.context_window = 42_000;
    assert!(catalog.add_custom_model(custom));
    assert_eq!(
        catalog.find_model("MiniMax-M2.7").unwrap().context_window,
        42_000
    );
    assert_eq!(catalog.models_by_provider("minimax").len(), 3);
    let file = tempfile::NamedTempFile::new().unwrap();
    catalog.save_custom_models(file.path()).unwrap();
    let mut reloaded = ModelCatalog::new();
    reloaded.load_custom_models(file.path());
    assert_eq!(
        reloaded.find_model("MiniMax-M2.7").unwrap().context_window,
        42_000
    );
    assert!(reloaded.remove_custom_model("MiniMax-M2.7"));
    assert_eq!(
        reloaded.find_model("MiniMax-M2.7").unwrap().context_window,
        204_800
    );
}

fn response_body(messages_api: bool, stream: bool) -> String {
    match (messages_api, stream) {
        (true, false) => json!({"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}).to_string(),
        (false, false) => json!({"choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string(),
        (false, true) => "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".into(),
        (true, true) => "event: content_block_start\ndata: {\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\nevent: message_stop\ndata: {}\n\n".into(),
    }
}

async fn capture_request(model: &str, endpoint: &str, stream: bool) {
    let messages_api = endpoint.ends_with("/anthropic");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = response_body(messages_api, stream);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        let (header_end, content_length) = loop {
            let n = socket.read(&mut buffer).await.unwrap();
            assert!(n > 0, "request ended before headers");
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                break (end + 4, length);
            }
        };
        while bytes.len() < header_end + content_length {
            let n = socket.read(&mut buffer).await.unwrap();
            assert!(n > 0, "request ended before body");
            bytes.extend_from_slice(&buffer[..n]);
        }
        let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
        let request: Value =
            serde_json::from_slice(&bytes[header_end..header_end + content_length]).unwrap();
        let content_type = if stream {
            "text/event-stream"
        } else {
            "application/json"
        };
        let response = format!("HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
        socket.write_all(response.as_bytes()).await.unwrap();
        (headers, request)
    });
    // Replace only the origin: exercise the documented regional paths offline.
    let path = reqwest::Url::parse(endpoint).unwrap().path().to_string();
    let driver = create_driver(&DriverConfig {
        provider: if messages_api { "anthropic" } else { "minimax" }.into(),
        api_key: Some("fixture-key".into()),
        base_url: Some(format!("http://{address}{path}")),
        skip_permissions: true,
        subprocess_timeout_secs: None,
    })
    .unwrap();
    let current = matches!(model, "MiniMax-M3.1-Flash" | "MiniMax-M3");
    let mut history = vec![Message::user("Hello")];
    if current {
        history = vec![
            Message {
                content: MessageContent::Blocks(vec![ContentBlock::Image {
                    media_type: "image/png".into(),
                    data: "aGVsbG8=".into(),
                }]),
                ..Default::default()
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::Blocks(vec![
                    ContentBlock::Thinking {
                        thinking: "prior reasoning".into(),
                        signature: None,
                        provider_metadata: Some(json!({"format":"reasoning_content"})),
                    },
                    ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "lookup".into(),
                        input: json!({}),
                        provider_metadata: None,
                    },
                ]),
                ..Default::default()
            },
            Message {
                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1".into(),
                    tool_name: "lookup".into(),
                    content: "result".into(),
                    is_error: false,
                }]),
                ..Default::default()
            },
        ];
    }
    let request = CompletionRequest {
        model: model.into(),
        messages: history,
        tools: vec![ToolDefinition {
            name: "lookup".into(),
            description: "Look up a value".into(),
            input_schema: json!({"type":"object","properties":{}}),
        }],
        max_tokens: 4096,
        temperature: 1.0,
        system: None,
        thinking: None,
    };
    let response = if stream {
        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        driver.stream(request, tx).await.unwrap()
    } else {
        driver.complete(request).await.unwrap()
    };
    assert_eq!(response.text(), "ok");
    let (headers, payload) = server.await.unwrap();
    let expected_path = if messages_api {
        "/anthropic/v1/messages"
    } else {
        "/v1/chat/completions"
    };
    assert!(headers.starts_with(&format!("POST {expected_path} HTTP/1.1\r\n")));
    assert_eq!(payload["model"], model);
    assert_eq!(payload["max_tokens"], 4096);
    assert_eq!(payload["stream"].as_bool().unwrap_or(false), stream);
    for field in [
        "thinking",
        "reasoning_effort",
        "output_config",
        "service_tier",
        "reasoning_split",
    ] {
        assert!(
            payload.get(field).is_none(),
            "unexpected field {field}: {payload}"
        );
    }
    assert_eq!(payload["tools"].as_array().unwrap().len(), 1);
    if current && messages_api {
        assert_eq!(
            payload["messages"][0]["content"][0]["source"]["data"],
            "aGVsbG8="
        );
        assert_eq!(
            payload["messages"][1]["content"][0],
            json!({"type":"thinking","thinking":"prior reasoning"})
        );
        assert_eq!(payload["messages"][1]["content"][1]["id"], "call_1");
        assert_eq!(
            payload["messages"][2]["content"][0]["tool_use_id"],
            "call_1"
        );
    } else if current {
        let image_url = payload["messages"].as_array().and_then(|messages| {
            messages.iter().find_map(|message| {
                message["content"].as_array().and_then(|parts| {
                    parts
                        .iter()
                        .find_map(|part| part["image_url"]["url"].as_str())
                })
            })
        });
        assert_eq!(
            image_url,
            Some("data:image/png;base64,aGVsbG8="),
            "MiniMax image part missing from OpenAI-compatible payload: {payload}"
        );
        assert_eq!(
            payload["messages"][1]["reasoning_content"],
            "prior reasoning"
        );
        assert_eq!(payload["messages"][1]["tool_calls"][0]["id"], "call_1");
        assert_eq!(payload["messages"][2]["tool_call_id"], "call_1");
    }
}

#[tokio::test]
async fn minimax_default_and_saved_ids_reach_both_http_transports() {
    let catalog = ModelCatalog::new();
    assert_eq!(
        catalog.get_provider("minimax").unwrap().base_url,
        "https://api.minimax.io/v1"
    );
    let default = catalog.default_model_for_provider("minimax").unwrap();
    for endpoint in [
        "https://api.minimax.io/v1",
        "https://api.minimaxi.com/v1",
        "https://api.minimax.io/anthropic",
        "https://api.minimaxi.com/anthropic",
    ] {
        for model in [
            default.as_str(),
            "MiniMax-M3",
            "MiniMax-M2.7",
            "custom-saved-model",
        ] {
            for stream in [false, true] {
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    capture_request(model, endpoint, stream),
                )
                .await
                .unwrap();
            }
        }
    }
}
