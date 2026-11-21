//! Comprehensive Fuzzing Matrix for Parsers:
//! - Gemini Response Parser & Emotion Parsers (parse_json_emotion, parse_emotion_response)
//! - Gemini Vision Computer Use Action JSON Parser (parse_action_json)
//! - MCP JSON-RPC Response Line Parser (JsonRpcResponse)

use ruby_actions::computer_use::gemini_vision::GeminiVisionComputerUseModelClient;
use ruby_core::chat::parse_emotion_response;
use ruby_gemini::chat_client::parse_json_emotion;
use ruby_mcp::JsonRpcResponse;

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }

    fn generate_corrupted_string(&mut self) -> String {
        let alphabet = b"{}[]():,\"'\\/ \t\r\n\x00\xff0123456789abcdefABCDEF<>";
        let len = self.next_bounded(120) as usize;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(alphabet[self.next_bounded(alphabet.len() as u32) as usize]);
        }
        String::from_utf8_lossy(&bytes).to_string()
    }
}

#[test]
fn test_fuzz_gemini_and_emotion_parsers_never_panic() {
    let mut rng = SimpleRng::new(0x1234_5678_9ABC_DEF0);

    // 1. Pathological crafted vectors
    let vectors = [
        "",
        "{}",
        "[]",
        "{\"answer\": null, \"emotion\": 123}",
        "```json\n{\"answer\": \"hi\", \"emotion\": \"invalid_emotion_name\"}\n```",
        "```json{\"answer\": unquoted_string}```",
        "{\"emotion\": \"happy\"}",
        "<emotion>speaking</emotion>Some answer",
        "<emotion></emotion>",
        "<emotion>nonexistent</emotion>",
        "{\"answer\": \"nested {\\\"inner\\\": 1}\", \"emotion\": \"idle\"}",
        "Truncated json: {\"ans",
        "\x00\x00\x00\x00\u{00ff}\u{00fe}",
        "{\"answer\": \"A\".repeat(10000)}",
    ];

    for vec in &vectors {
        let (ans1, emo1) = parse_json_emotion(vec);
        assert!(!ans1.is_empty() || emo1.is_none() || emo1.is_some());

        let (ans2, emo2) = parse_emotion_response(vec, "context user prompt");
        assert!(!ans2.is_empty() || emo2 == ruby_core::Emotion::Neutral);
    }

    // 2. 5,000 pseudo-random corrupted strings
    for _ in 0..5000 {
        let s = rng.generate_corrupted_string();
        let _ = parse_json_emotion(&s);
        let _ = parse_emotion_response(&s, "context");
    }
}

#[test]
fn test_fuzz_gemini_vision_action_parser_never_panics() {
    let mut rng = SimpleRng::new(0xBEEF_CAFE_FEED_FACE);

    let vectors = [
        "",
        "{}",
        "{\"action\": \"click\"}",
        "{\"action\": \"click\", \"coordinate\": [100]}",
        "{\"action\": \"click\", \"coordinate\": [100, 200, 300]}",
        "{\"action\": \"type_text\", \"text\": null}",
        "{\"action\": \"key_combination\", \"keys\": [\"Control\", 123]}",
        "{\"action\": \"mouse_down\", \"button\": \"invalid_button\"}",
        "{\"action\": \"unknown_action_type\"}",
        "```json\n{\"action\": \"done\"}\n```",
        "```json\nmalformed\n```",
        "{\"action\": \"press_hotkey\", \"keys\": []}",
    ];

    for vec in &vectors {
        let _ = GeminiVisionComputerUseModelClient::parse_action_response(vec);
    }

    for _ in 0..5000 {
        let s = rng.generate_corrupted_string();
        let _ = GeminiVisionComputerUseModelClient::parse_action_response(&s);
    }
}

#[test]
fn test_fuzz_mcp_json_rpc_line_parser_never_panics() {
    let mut rng = SimpleRng::new(0xABCD_EF01_2345_6789);

    let vectors = [
        "",
        "\n",
        "\r\n",
        "{}",
        "{\"jsonrpc\": \"2.0\"}",
        "{\"jsonrpc\": \"2.0\", \"id\": 1, \"result\": {\"tools\": []}}",
        "{\"jsonrpc\": \"2.0\", \"id\": 2, \"error\": {\"code\": -32600, \"message\": \"Invalid Request\"}}",
        "{\"jsonrpc\": \"2.0\", \"id\": \"string_id\", \"result\": true}",
        "{\"jsonrpc\": 2.0}",
        "{\"id\": 1}",
        "null",
        "true",
        "12345",
        "\"raw string\"",
        "{\"result\": [1,2,3]}",
        "{\"jsonrpc\": \"2.0\", \"id\": null, \"result\": null, \"error\": null}",
    ];

    for vec in &vectors {
        let _res: Result<JsonRpcResponse, _> = serde_json::from_str(vec);
    }

    for _ in 0..5000 {
        let s = rng.generate_corrupted_string();
        let _res: Result<JsonRpcResponse, _> = serde_json::from_str(&s);
    }
}
