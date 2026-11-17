//! Milestone M2: Chat conversation models and emotion extraction.

use serde::{Deserialize, Serialize};
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

/// Extracts inline emotional tags (e.g. `[emotion: curious]` or `[emotion:happy]`) from raw LLM responses.
pub fn extract_emotion_tag(raw_text: &str) -> (Option<Emotion>, String) {
    let lower = raw_text.to_lowercase();
    if let Some(start_idx) = lower.find("[emotion:") {
        if let Some(end_rel) = lower[start_idx..].find(']') {
            let end_idx = start_idx + end_rel;
            let tag_content = lower[start_idx + 9..end_idx].trim();
            let emotion = match tag_content {
                "happy" => Some(Emotion::Happy),
                "excited" => Some(Emotion::Excited),
                "sad" => Some(Emotion::Sad),
                "empathetic" => Some(Emotion::Empathetic),
                "curious" => Some(Emotion::Curious),
                "surprised" => Some(Emotion::Surprised),
                "neutral" => Some(Emotion::Neutral),
                _ => None,
            };
            let mut cleaned = String::new();
            cleaned.push_str(&raw_text[..start_idx]);
            cleaned.push_str(&raw_text[end_idx + 1..]);
            return (emotion, cleaned.trim().to_string());
        }
    }
    (None, raw_text.trim().to_string())
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
    fn test_extract_emotion_tag_valid() {
        let raw = "Hello there! [emotion: curious] How can I assist you?";
        let (emotion, cleaned) = extract_emotion_tag(raw);
        assert_eq!(emotion, Some(Emotion::Curious));
        assert_eq!(cleaned, "Hello there!  How can I assist you?");
    }

    #[test]
    fn test_extract_emotion_tag_none() {
        let raw = "Just a standard assistant reply with no tags.";
        let (emotion, cleaned) = extract_emotion_tag(raw);
        assert_eq!(emotion, None);
        assert_eq!(cleaned, raw);
    }

    #[test]
    fn test_chat_session_accumulates_turns() {
        let mut session = ChatSession::new();
        session.add_message(ChatMessage::user("Hi Ruby"));
        session.add_message(ChatMessage::assistant("Hello!", Some(Emotion::Happy)));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, ChatRole::User);
        assert_eq!(session.messages[1].role, ChatRole::Assistant);
        assert_eq!(session.messages[1].emotion, Some(Emotion::Happy));
    }
}
