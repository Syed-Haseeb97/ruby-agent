//! Milestone M2: Multi-turn Chat Client with Emotion Tagging and Gemini Integration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::GeminiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    User,
    Model,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnResponse {
    pub text: String,
    pub detected_emotion: Option<String>,
}

#[async_trait]
pub trait ChatClient: Send + Sync {
    async fn send_message(
        &self,
        history: &[ChatMessage],
        new_message: &str,
    ) -> Result<ChatTurnResponse, GeminiError>;
}

/// Mock chat client for testing M2 foundations offline.
pub struct MockChatClient {
    pub canned_text: String,
    pub canned_emotion: Option<String>,
}

impl MockChatClient {
    pub fn new(text: impl Into<String>, emotion: Option<impl Into<String>>) -> Self {
        Self {
            canned_text: text.into(),
            canned_emotion: emotion.map(|e| e.into()),
        }
    }
}

#[async_trait]
impl ChatClient for MockChatClient {
    async fn send_message(
        &self,
        history: &[ChatMessage],
        new_message: &str,
    ) -> Result<ChatTurnResponse, GeminiError> {
        let text = if history.is_empty() {
            format!("{}: {}", self.canned_text, new_message)
        } else {
            self.canned_text.clone()
        };
        Ok(ChatTurnResponse {
            text,
            detected_emotion: self.canned_emotion.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_chat_client_responds_with_emotion() {
        let client = MockChatClient::new("Certainly! I am here to help.", Some("happy"));
        let history = vec![ChatMessage {
            role: ChatRole::User,
            content: "Can you assist me?".to_string(),
        }];
        let resp = client.send_message(&history, "Can you assist me?").await;
        assert!(resp.is_ok());
        let turn = resp.unwrap();
        assert_eq!(turn.text, "Certainly! I am here to help.");
        assert_eq!(turn.detected_emotion, Some("happy".to_string()));
    }
}
