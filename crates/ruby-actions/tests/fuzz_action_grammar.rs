use ruby_actions::computer_use::gemini_vision::GeminiVisionComputerUseModelClient;
use ruby_actions::LocalAction;

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }
}

#[test]
fn test_fuzz_computer_action_parser_robustness() {
    // 1. Pathological manual vectors
    let malformed_vectors = [
        "",
        "   ",
        "\0",
        "\0\0\0\0",
        "```json",
        "```json\n",
        "```json\n{\"action\": \"click\"}\n", // missing coordinates
        "```json\n{\"action\": \"invalid_action_name\"}\n```",
        "{",
        "}",
        "{{{{{}}}}}",
        "[\"array\", \"instead\", \"of\", \"object\"]",
        "null",
        "true",
        "12345",
        "\"raw string\"",
        "{\"action\": \"click\", \"x\": -99999999999999999999, \"y\": 500}",
        "{\"action\": \"type\", \"text\": \"\0\0\0\x1f\x7f\"}",
        "{\"action\": \"key\", \"combo\": \"Control+Alt+Delete+F13+Super+Hypothetical\"}",
        "{\"action\": \"wait\", \"ms\": 999999999999999999999999999999999999999999999}",
        "```json\n```\n```json\n{\"action\":\"finish\"}\n```",
        "Here is the action: {\"action\": \"finish\"} Hope this helps!",
    ];

    for &vec in &malformed_vectors {
        let _ = GeminiVisionComputerUseModelClient::parse_action_response(vec);
    }

    // 2. Huge string fuzzing (100k characters with embedded nulls and brackets)
    let mut huge_str = String::with_capacity(120_000);
    for i in 0..10_000 {
        huge_str.push_str("{\"action\": \"type\", \"text\": \"fuzz_");
        huge_str.push_str(&i.to_string());
        huge_str.push_str("\0\"}");
    }
    let _ = GeminiVisionComputerUseModelClient::parse_action_response(&huge_str);

    // 3. Generative random UTF-8 and token fuzzing: 5,000 iterations
    let mut rng = SimpleRng::new(0xCAFE_BABE_1337_7331);
    let token_pool = [
        "{", "}", "[", "]", ":", ",", "\"", "'", "```json", "```", "\n", "\r", "\t", "\0",
        "action", "click", "double_click", "triple_click", "middle_click", "right_click",
        "mouse_down", "release_mouse_button", "move_cursor", "type", "key", "wait", "hotkey",
        "scroll_at", "drag_and_drop_at", "finish", "fail", "x", "y", "text", "combo", "ms",
        "null", "true", "false", "NaN", "Infinity", "-1", "0", "999999", "1.5",
        "こんにちは", "🚀", "Русский", "مرحبا",
    ];

    for _ in 0..5000 {
        let len = rng.next_bounded(30) as usize + 1;
        let mut sample = String::new();
        for _ in 0..len {
            let tok = token_pool[rng.next_bounded(token_pool.len() as u32) as usize];
            sample.push_str(tok);
        }

        // Must never panic, crash, or abort
        let _ = GeminiVisionComputerUseModelClient::parse_action_response(&sample);
    }
}

#[test]
fn test_fuzz_local_action_serde_robustness() {
    let mut rng = SimpleRng::new(0xABCD_EF01_2345_6789);
    let mut garbage = Vec::new();

    // 1,000 randomized byte arrays passed to serde_json::from_slice::<LocalAction>
    for _ in 0..1000 {
        let byte_len = rng.next_bounded(256) as usize;
        garbage.clear();
        for _ in 0..byte_len {
            garbage.push((rng.next_u32() & 0xFF) as u8);
        }
        let _ = serde_json::from_slice::<LocalAction>(&garbage);
    }

    // Known valid serialization roundtrips
    let valid_actions = [
        LocalAction::OpenFile("test.txt".into()),
        LocalAction::LaunchApp("notepad".into()),
        LocalAction::CloseApp("calc".into()),
        LocalAction::OpenSystemSettings("display".into()),
        LocalAction::ScreenClick { x: 100, y: 200 },
        LocalAction::SetReminder { seconds: 60, message: "Tea time".into() },
        LocalAction::SetWindowsStartup { enable: false },
        LocalAction::ListFolder("C:\\Safe".into()),
        LocalAction::ReadFileHead { path: "D:\\Safe\\log.txt".into(), max_bytes: 1024 },
        LocalAction::FileInfo("D:\\Safe\\test.bin".into()),
        LocalAction::SendFile { path: "D:\\Safe\\out.pdf".into(), max_bytes: 4096 },
        LocalAction::GitStatus("D:\\Safe\\repo".into()),
        LocalAction::GitLog { repo: "D:\\Safe\\repo".into(), limit: 5 },
        LocalAction::GitDiffStat("D:\\Safe\\repo".into()),
        LocalAction::GhPrList { repo: Some("owner/repo".into()), limit: 10 },
        LocalAction::GhIssueList { repo: None, limit: 10 },
        LocalAction::MediaControl("play_pause".into()),
    ];

    for act in &valid_actions {
        let json = serde_json::to_string(act).expect("serialization must succeed");
        let deserialized: LocalAction = serde_json::from_str(&json).expect("deserialization must succeed");
        assert_eq!(*act, deserialized);
    }
}
