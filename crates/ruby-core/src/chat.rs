//! Milestone M2: Structured Chat conversation models and emotion extraction.
//! Replaces legacy regex tags with robust JSON parsing, distress mapping,
//! safe Neutral fallbacks, and strictly isolated spoken text.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use ts_rs::TS;
use uuid::Uuid;

use crate::Emotion;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatMessage {
    #[ts(type = "string")]
    pub id: Uuid,
    pub role: ChatRole,
    pub content: String,
    pub timestamp_millis: i64,
    pub emotion: Option<Emotion>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: ChatRole::User,
            content: content.into(),
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            emotion: None,
        }
    }

    pub fn assistant(content: impl Into<String>, emotion: Option<Emotion>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: ChatRole::Assistant,
            content: content.into(),
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
            emotion,
        }
    }
}

static SUPPORT_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)\bi (?:am|'m|feel|am feeling) (?:having )?(?:a )?(?:really )?(?:bad|rough|hard|terrible|awful|tough) day\b").unwrap(),
        Regex::new(r"(?i)\bi(?:'m| am) (?:feeling )?(?:(?:so|really|very) )?(?:sad|upset|down|lonely|overwhelmed|stressed|anxious|hurt|heartbroken|exhausted)\b").unwrap(),
        Regex::new(r"(?i)\bi feel (?:like a failure|disappointed in myself|hopeless|alone|overwhelmed|stressed|anxious|hurt|sad|down)\b").unwrap(),
        Regex::new(r"(?i)\bi(?:'m| am) struggling\b").unwrap(),
        Regex::new(r"(?i)\bi need (?:some )?(?:support|comfort|someone to talk to)\b").unwrap(),
        Regex::new(r"(?i)\bi have (?:a )?(?:really )?(?:bad|rough|hard|terrible|awful|tough) day\b").unwrap(),
        Regex::new(r"(?i)\bi(?:'ve| have) had (?:a )?(?:really )?(?:bad|rough|hard|terrible|awful|tough) day\b").unwrap(),
        Regex::new(r"(?i)\bthings have been (?:really )?(?:hard|rough|difficult) for me\b").unwrap(),
    ]
});

/// Normalizes an input string to one of the seven supported Ruby emotions,
/// resolving aliases and defaulting safely to `Neutral`.
pub fn normalize_emotion(value: &str) -> Emotion {
    let cleaned = value.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    match cleaned.as_str() {
        "happy" | "joyful" => Emotion::Happy,
        "excited" | "enthusiastic" => Emotion::Excited,
        "sad" => Emotion::Sad,
        "empathetic" | "sympathetic" => Emotion::Empathetic,
        "curious" | "interested" => Emotion::Curious,
        "surprised" | "astonished" => Emotion::Surprised,
        "neutral" | "none" | "calm" => Emotion::Neutral,
        _ => Emotion::Neutral,
    }
}

/// Detects clear first-person distress expressions and maps them to `Empathetic`.
pub fn infer_support_emotion(user_text: &str) -> Option<Emotion> {
    let normalized = user_text.trim();
    if normalized.is_empty() {
        return None;
    }
    for pattern in SUPPORT_PATTERNS.iter() {
        if pattern.is_match(normalized) {
            return Some(Emotion::Empathetic);
        }
    }
    None
}

/// Cleanses spoken/rendered dialogue to guarantee no leaked emotion metadata
/// or JSON formatting artifacts can ever be spoken by TTS or displayed in the chat stream.
pub fn clean_spoken_text(text: &str) -> String {
    let mut cleaned = text.trim().to_string();
    // Strip any accidental markdown formatting or JSON keys
    if cleaned.starts_with("```") {
        if let Some(first_newline) = cleaned.find('\n') {
            cleaned = cleaned[first_newline + 1..].to_string();
        }
        if cleaned.ends_with("```") {
            cleaned = cleaned[..cleaned.len().saturating_sub(3)].to_string();
        }
    }
    // Remove accidental inline brackets like [emotion: ...]
    if let Some(start) = cleaned.to_lowercase().find("[emotion:") {
        if let Some(end) = cleaned[start..].find(']') {
            let before = &cleaned[..start];
            let after = &cleaned[start + end + 1..];
            cleaned = format!("{}{}", before.trim_end(), after.trim_start());
        }
    }
    cleaned.trim().to_string()
}

/// Robustly parses model output into user-facing spoken text and validated Emotion.
/// Implements markdown-fence unwrapping, inner JSON detection, distress prompt override,
/// and safe fallback to plain text.
pub fn parse_emotion_response(raw_text: &str, user_text: &str) -> (String, Emotion) {
    let raw_trimmed = raw_text.trim();
    if raw_trimmed.is_empty() {
        return (
            "I received an empty reply from the model. Please try again.".to_string(),
            Emotion::Neutral,
        );
    }

    // Unfence markdown blocks
    let mut candidate = raw_trimmed;
    if candidate.starts_with("```") {
        let lines: Vec<&str> = candidate.lines().collect();
        let start = if !lines.is_empty() && lines[0].starts_with("```") {
            1
        } else {
            0
        };
        let end = if lines.len() > start && lines.last().is_some_and(|l| l.trim() == "```") {
            lines.len() - 1
        } else {
            lines.len()
        };
        candidate = lines[start..end].join("\n").leak();
    }

    let parsed_val: Result<serde_json::Value, _> = serde_json::from_str(candidate);
    let payload = match parsed_val {
        Ok(v) => Some(v),
        Err(_) => {
            // Attempt to slice between first '{' and last '}'
            if let (Some(first), Some(last)) = (candidate.find('{'), candidate.rfind('}')) {
                if last > first {
                    serde_json::from_str::<serde_json::Value>(&candidate[first..=last]).ok()
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    let support_emotion = infer_support_emotion(user_text);

    if let Some(json_obj) = payload.and_then(|v| v.as_object().cloned()) {
        let answer_str = json_obj
            .get("response")
            .or_else(|| json_obj.get("answer"))
            .or_else(|| json_obj.get("text"))
            .and_then(|v| v.as_str());

        if let Some(answer) = answer_str {
            let clean_answer = clean_spoken_text(answer);
            if !clean_answer.is_empty() {
                let emotion_str = json_obj
                    .get("emotion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("neutral");
                let emotion = support_emotion.unwrap_or_else(|| normalize_emotion(emotion_str));
                return (clean_answer, emotion);
            }
        }
    }

    // Safe fallback: plain text answer with distress or neutral emotion
    let fallback_emotion = support_emotion.unwrap_or(Emotion::Neutral);
    (clean_spoken_text(raw_trimmed), fallback_emotion)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatSession {
    #[ts(type = "string")]
    pub id: Uuid,
    pub messages: Vec<ChatMessage>,
}

impl ChatSession {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            messages: Vec::new(),
        }
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_response_and_supported_emotion() {
        let (answer, emotion) = parse_emotion_response(
            r#"{"response": "That is fantastic news!", "emotion": "excited"}"#,
            "",
        );
        assert_eq!(answer, "That is fantastic news!");
        assert_eq!(emotion, Emotion::Excited);
    }

    #[test]
    fn test_invalid_emotion_falls_back_to_neutral_without_losing_answer() {
        let (answer, emotion) = parse_emotion_response(
            r#"{"response": "Here is the answer.", "emotion": "angry"}"#,
            "",
        );
        assert_eq!(answer, "Here is the answer.");
        assert_eq!(emotion, Emotion::Neutral);
    }

    #[test]
    fn test_plain_text_reply_remains_compatible() {
        let (answer, emotion) = parse_emotion_response("A normal plain-text answer.", "");
        assert_eq!(answer, "A normal plain-text answer.");
        assert_eq!(emotion, Emotion::Neutral);
    }

    #[test]
    fn test_json_inside_markdown_fence_is_supported() {
        let raw =
            "```json\n{\"response\": \"I am here with you.\", \"emotion\": \"empathetic\"}\n```";
        let (answer, emotion) = parse_emotion_response(raw, "");
        assert_eq!(answer, "I am here with you.");
        assert_eq!(emotion, Emotion::Empathetic);
    }

    #[test]
    fn test_emotion_aliases_are_normalized() {
        assert_eq!(normalize_emotion("joyful"), Emotion::Happy);
        assert_eq!(normalize_emotion("enthusiastic"), Emotion::Excited);
        assert_eq!(normalize_emotion("sympathetic"), Emotion::Empathetic);
        assert_eq!(normalize_emotion("interested"), Emotion::Curious);
        assert_eq!(normalize_emotion("astonished"), Emotion::Surprised);
        assert_eq!(normalize_emotion("calm"), Emotion::Neutral);
        assert_eq!(normalize_emotion("unknown-label"), Emotion::Neutral);
    }

    #[test]
    fn test_preamble_around_json_is_supported() {
        let raw = "Here is the result:\n{\"response\": \"Take your time.\", \"emotion\": \"empathetic\"}\nDone.";
        let (answer, emotion) = parse_emotion_response(raw, "");
        assert_eq!(answer, "Take your time.");
        assert_eq!(emotion, Emotion::Empathetic);
    }

    #[test]
    fn test_missing_or_empty_answer_falls_back_without_crashing() {
        for raw in &[
            r#"{"emotion": "happy"}"#,
            r#"{"response": "  ", "emotion": "happy"}"#,
        ] {
            let (answer, emotion) = parse_emotion_response(raw, "");
            assert_eq!(answer, raw.trim());
            assert_eq!(emotion, Emotion::Neutral);
        }
    }

    #[test]
    fn test_malformed_json_is_kept_as_plain_text() {
        let raw = r#"{"response": "broken", "emotion": "#;
        let (answer, emotion) = parse_emotion_response(raw, "");
        assert_eq!(answer, raw.trim());
        assert_eq!(emotion, Emotion::Neutral);
    }

    #[test]
    fn test_bad_day_maps_to_empathetic_even_if_model_says_sad() {
        let (answer, emotion) = parse_emotion_response(
            r#"{"response":"I am sorry today has been hard.","emotion":"sad"}"#,
            "I have a bad day today",
        );
        assert_eq!(answer, "I am sorry today has been hard.");
        assert_eq!(emotion, Emotion::Empathetic);
    }

    #[test]
    fn test_supportive_disclosures_map_to_empathetic() {
        let examples = [
            "I've had a rough day",
            "I feel disappointed in myself",
            "I'm overwhelmed",
            "I'm so sad right now",
            "I am struggling",
            "I need someone to talk to",
        ];
        for text in examples {
            assert_eq!(
                infer_support_emotion(text),
                Some(Emotion::Empathetic),
                "Failed for: {}",
                text
            );
        }
    }

    #[test]
    fn test_unrelated_negative_word_does_not_force_empathy() {
        assert_eq!(
            infer_support_emotion("How do I fix a bad day in my calendar app?"),
            None
        );
        assert_eq!(
            infer_support_emotion("Explain the sad ending of this movie."),
            None
        );
    }

    #[test]
    fn test_non_distress_keeps_model_emotion() {
        let (answer, emotion) = parse_emotion_response(
            r#"{"response":"That is exciting!","emotion":"excited"}"#,
            "I got accepted into my dream university!",
        );
        assert_eq!(answer, "That is exciting!");
        assert_eq!(emotion, Emotion::Excited);
    }

    #[test]
    fn test_emotion_label_never_spoken_or_typed() {
        // Even if model leaks emotion inside response text or formatting
        let raw = r#"{"response": "Here is the data [emotion: excited]", "emotion": "excited"}"#;
        let (answer, emotion) = parse_emotion_response(raw, "Status update");
        assert_eq!(answer, "Here is the data");
        assert_eq!(emotion, Emotion::Excited);
        assert!(!answer.contains("excited"));
        assert!(!answer.contains("[emotion:"));
    }

    #[test]
    fn test_chat_session_accumulates_turns() {
        let mut session = ChatSession::new();
        session.add_message(ChatMessage::user("Hello"));
        session.add_message(ChatMessage::assistant("Hi there!", Some(Emotion::Happy)));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, ChatRole::User);
        assert_eq!(session.messages[1].role, ChatRole::Assistant);
        assert_eq!(session.messages[1].emotion, Some(Emotion::Happy));
    }
}
