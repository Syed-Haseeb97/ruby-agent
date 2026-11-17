import React, { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { OrbPreviewPanel } from "./OrbPreviewPanel";

interface RemoteStatus {
  enabled: boolean;
  paired: boolean;
  paired_user_id: number | null;
  connected: boolean;
  active_pairing_code: string | null;
  pairing_code_expires_in_secs: number | null;
  last_command_time_secs: number | null;
}

interface ChatTurnResult {
  response_text: string;
  emotion: string;
  served_backend: string;
  tts_engine: string;
}

interface MessageItem {
  id: string;
  role: "user" | "assistant";
  text: string;
  emotion?: string;
  servedBackend?: string;
  ttsEngine?: string;
  timestamp: string;
}

export const ConsoleView: React.FC = () => {
  const [notice, setNotice] = useState<string | null>(null);
  const [remoteStatus, setRemoteStatus] = useState<RemoteStatus | null>(null);
  const [messages, setMessages] = useState<MessageItem[]>([
    {
      id: "welcome-1",
      role: "assistant",
      text: "Good day. I am Ruby, your desktop companion. Type a request below or use the Wispr-style dictation chord (Ctrl+Win) to begin.",
      emotion: "Neutral",
      servedBackend: "gemini-3.8-flash",
      ttsEngine: "Edge-TTS",
      timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
    },
  ]);
  const [inputText, setInputText] = useState("");
  const [isThinking, setIsThinking] = useState(false);
  const [selectedTts, setSelectedTts] = useState("Edge-TTS");
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const fetchRemoteStatus = async () => {
    try {
      const status = await invoke<RemoteStatus>("get_remote_status");
      setRemoteStatus(status);
    } catch {
      // Ignore if Tauri IPC unavailable
    }
  };

  const handleDisconnect = async () => {
    try {
      const updated = await invoke<RemoteStatus>("disconnect_remote");
      setRemoteStatus(updated);
      setNotice("Remote disconnected and unassigned successfully.");
      setTimeout(() => setNotice(null), 5000);
    } catch (err) {
      setNotice(`Failed to disconnect: ${String(err)}`);
    }
  };

  const handleSendMessage = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const trimmed = inputText.trim();
    if (!trimmed || isThinking) return;

    const userMsg: MessageItem = {
      id: `user-${Date.now()}`,
      role: "user",
      text: trimmed,
      timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
    };

    setMessages((prev) => [...prev, userMsg]);
    setInputText("");
    setIsThinking(true);

    try {
      const result = await invoke<ChatTurnResult>("send_chat_message", {
        message: trimmed,
        ttsEngine: selectedTts,
      });

      const assistantMsg: MessageItem = {
        id: `asst-${Date.now()}`,
        role: "assistant",
        text: result.response_text,
        emotion: result.emotion,
        servedBackend: result.served_backend,
        ttsEngine: result.tts_engine,
        timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
      };

      setMessages((prev) => [...prev, assistantMsg]);
    } catch (err) {
      const errorMsg: MessageItem = {
        id: `err-${Date.now()}`,
        role: "assistant",
        text: `Error: ${String(err)}`,
        emotion: "Sad",
        servedBackend: "Error",
        timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
      };
      setMessages((prev) => [...prev, errorMsg]);
    } finally {
      setIsThinking(false);
    }
  };

  const handleCancelSpeech = async () => {
    try {
      await invoke("cancel_speech");
      setNotice("Speech cancelled.");
      setTimeout(() => setNotice(null), 3000);
    } catch {
      // Ignore
    }
  };

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, isThinking]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        handleCancelSpeech();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  useEffect(() => {
    fetchRemoteStatus();
    const interval = setInterval(fetchRemoteStatus, 4000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    let unlistenNotice: (() => void) | undefined;
    let unlistenToast: (() => void) | undefined;

    listen<string>("console_notice", (event) => {
      setNotice(event.payload);
      setTimeout(() => setNotice(null), 8000);
    }).then((un) => {
      unlistenNotice = un;
    });

    listen<string>("toast_notification", (event) => {
      if (event.payload.includes("FreeLLMAPI")) {
        setNotice(event.payload);
        setTimeout(() => setNotice(null), 8000);
      }
    }).then((un) => {
      unlistenToast = un;
    });

    return () => {
      unlistenNotice?.();
      unlistenToast?.();
    };
  }, []);

  return (
    <div
      style={{
        width: "100%",
        minHeight: "100vh",
        backgroundColor: "#F5F3EE",
        color: "#16181D",
        fontFamily: "'Geist', sans-serif",
        padding: "2rem",
        boxSizing: "border-box",
        overflowY: "auto",
      }}
    >
      <div style={{ maxWidth: "880px", margin: "0 auto", display: "flex", flexDirection: "column", gap: "2rem" }}>
        {/* Editorial Masthead Header */}
        <header
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "flex-end",
            borderBottom: "1px solid #C8C2B5",
            paddingBottom: "1.25rem",
          }}
        >
          <div>
            <div
              style={{
                fontFamily: "'Geist Mono', monospace",
                fontSize: "11px",
                textTransform: "uppercase",
                letterSpacing: "0.1em",
                color: "#5F626A",
                marginBottom: "0.25rem",
              }}
            >
              Antigravity • Ruby Autonomous Desktop Engine
            </div>
            <h1
              style={{
                fontFamily: "'Instrument Serif', serif",
                fontSize: "2.75rem",
                lineHeight: "0.95",
                fontWeight: 400,
                margin: 0,
                color: "#16181D",
              }}
            >
              Ruby Assistant Console
            </h1>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: "0.75rem" }}>
            {notice && (
              <div
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  borderRadius: "9999px",
                  border: "1px solid #86560D",
                  backgroundColor: "#F6ECD9",
                  padding: "0.25rem 0.875rem",
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "11px",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "#86560D",
                }}
              >
                <span>NOTICE: {notice}</span>
              </div>
            )}
            {/* Static Eyebrow Status Capsule (Strictly static, zero pulsing dots) */}
            <div
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "0.5rem",
                borderRadius: "9999px",
                border: "1px solid #C8C2B5",
                backgroundColor: "#FFFFFF",
                padding: "0.25rem 0.875rem",
                fontFamily: "'Geist Mono', monospace",
                fontSize: "11px",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                color: "#1D6B52",
              }}
            >
              <span>TAURI 2.0 • M2 CONVERSATIONAL ENGINE</span>
            </div>
          </div>
        </header>

        {/* Operational Overview Cards */}
        <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "1rem" }}>
          <div
            style={{
              backgroundColor: "#FFFFFF",
              border: "1px solid #C8C2B5",
              borderRadius: "1rem",
              padding: "1.25rem",
            }}
          >
            <div
              style={{
                fontFamily: "'Geist Mono', monospace",
                fontSize: "10px",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "#5F626A",
                marginBottom: "0.5rem",
              }}
            >
              Workspace Storage
            </div>
            <div style={{ fontSize: "1.125rem", fontWeight: 600, color: "#16181D" }}>
              D:\projects\ruby-agent
            </div>
            <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.8125rem" }}>
              Strict Drive D: residency
            </p>
          </div>

          <div
            style={{
              backgroundColor: "#FFFFFF",
              border: "1px solid #C8C2B5",
              borderRadius: "1rem",
              padding: "1.25rem",
            }}
          >
            <div
              style={{
                fontFamily: "'Geist Mono', monospace",
                fontSize: "10px",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "#5F626A",
                marginBottom: "0.5rem",
              }}
            >
              Model Routing
            </div>
            <div style={{ fontSize: "1.125rem", fontWeight: 600, color: "#16181D" }}>
              gemini-3.8-flash
            </div>
            <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.8125rem" }}>
              Fallback: 3-flash-preview
            </p>
          </div>

          <div
            style={{
              backgroundColor: "#FFFFFF",
              border: "1px solid #C8C2B5",
              borderRadius: "1rem",
              padding: "1.25rem",
            }}
          >
            <div
              style={{
                fontFamily: "'Geist Mono', monospace",
                fontSize: "10px",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "#5F626A",
                marginBottom: "0.5rem",
              }}
            >
              Voice Synthesis
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
              <select
                value={selectedTts}
                onChange={(e) => setSelectedTts(e.target.value)}
                style={{
                  border: "1px solid #C8C2B5",
                  borderRadius: "0.5rem",
                  padding: "0.25rem 0.5rem",
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "12px",
                  backgroundColor: "#F5F3EE",
                  color: "#16181D",
                  cursor: "pointer",
                }}
              >
                <option value="Edge-TTS">Edge-TTS (Neural)</option>
                <option value="Gemini-TTS">Gemini-TTS (API)</option>
                <option value="Windows-Speech">Windows-Speech (SAPI)</option>
              </select>
            </div>
            <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.8125rem" }}>
              Esc or new input cancels
            </p>
          </div>
        </div>

        {/* Milestone M2 Conversational Chat Section */}
        <section
          style={{
            backgroundColor: "#FFFFFF",
            border: "1px solid #C8C2B5",
            borderRadius: "1rem",
            padding: "1.5rem",
            display: "flex",
            flexDirection: "column",
            gap: "1.25rem",
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <div>
              <h2
                style={{
                  fontFamily: "'Instrument Serif', serif",
                  fontSize: "1.75rem",
                  fontWeight: 400,
                  margin: 0,
                  color: "#16181D",
                }}
              >
                Editorial Dialogue Stream
              </h2>
              <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.875rem" }}>
                Multi-turn conversation with real-time emotion tagging and non-blocking speech playback.
              </p>
            </div>

            <button
              onClick={handleCancelSpeech}
              style={{
                border: "1px solid #C8C2B5",
                backgroundColor: "#FFFFFF",
                borderRadius: "9999px",
                padding: "0.35rem 0.85rem",
                fontFamily: "'Geist Mono', monospace",
                fontSize: "11px",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                color: "#16181D",
                cursor: "pointer",
              }}
            >
              Stop Audio (Esc)
            </button>
          </div>

          {/* Messages Stream Container */}
          <div
            style={{
              maxHeight: "360px",
              minHeight: "220px",
              overflowY: "auto",
              display: "flex",
              flexDirection: "column",
              gap: "1rem",
              padding: "1rem",
              backgroundColor: "#F5F3EE",
              borderRadius: "0.75rem",
              border: "1px solid #C8C2B5",
            }}
          >
            {messages.map((msg) => (
              <div
                key={msg.id}
                style={{
                  alignSelf: msg.role === "user" ? "flex-end" : "flex-start",
                  maxWidth: "80%",
                  backgroundColor: msg.role === "user" ? "#16181D" : "#FFFFFF",
                  color: msg.role === "user" ? "#F5F3EE" : "#16181D",
                  padding: "0.875rem 1.125rem",
                  borderRadius: "1rem",
                  border: msg.role === "user" ? "none" : "1px solid #C8C2B5",
                  boxShadow: "0 1px 2px rgba(0,0,0,0.03)",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "0.5rem",
                    marginBottom: "0.35rem",
                    fontFamily: "'Geist Mono', monospace",
                    fontSize: "10px",
                    textTransform: "uppercase",
                    letterSpacing: "0.06em",
                    color: msg.role === "user" ? "#C8C2B5" : "#5F626A",
                  }}
                >
                  <span>{msg.role === "user" ? "You" : "Ruby"}</span>
                  <span>•</span>
                  <span>{msg.timestamp}</span>
                  {msg.emotion && (
                    <span
                      style={{
                        backgroundColor: "#E3EFE8",
                        color: "#1D6B52",
                        padding: "0.1rem 0.4rem",
                        borderRadius: "9999px",
                        fontWeight: 600,
                      }}
                    >
                      {msg.emotion.toUpperCase()}
                    </span>
                  )}
                  {msg.servedBackend && (
                    <span
                      style={{
                        backgroundColor: msg.servedBackend.toLowerCase().includes("freellmapi")
                          ? "#F6ECD9"
                          : "#F5F3EE",
                        color: msg.servedBackend.toLowerCase().includes("freellmapi")
                          ? "#86560D"
                          : "#5F626A",
                        padding: "0.1rem 0.4rem",
                        borderRadius: "9999px",
                        border: "1px solid #C8C2B5",
                      }}
                    >
                      {msg.servedBackend.toUpperCase()}
                    </span>
                  )}
                </div>
                <div style={{ fontSize: "0.9375rem", lineHeight: "1.45" }}>{msg.text}</div>
              </div>
            ))}
            {isThinking && (
              <div
                style={{
                  alignSelf: "flex-start",
                  backgroundColor: "#FFFFFF",
                  padding: "0.75rem 1rem",
                  borderRadius: "1rem",
                  border: "1px solid #C8C2B5",
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "11px",
                  color: "#5F626A",
                }}
              >
                Thinking & generating response...
              </div>
            )}
            <div ref={messagesEndRef} />
          </div>

          {/* Message Input Form */}
          <form onSubmit={handleSendMessage} style={{ display: "flex", gap: "0.75rem" }}>
            <input
              type="text"
              value={inputText}
              onChange={(e) => setInputText(e.target.value)}
              placeholder="Ask Ruby anything, or state how you are feeling..."
              disabled={isThinking}
              style={{
                flex: 1,
                border: "1px solid #C8C2B5",
                borderRadius: "9999px",
                padding: "0.75rem 1.25rem",
                fontFamily: "'Geist', sans-serif",
                fontSize: "0.9375rem",
                backgroundColor: "#FFFFFF",
                color: "#16181D",
                outline: "none",
              }}
            />
            <button
              type="submit"
              disabled={isThinking || !inputText.trim()}
              style={{
                backgroundColor: "#16181D",
                color: "#F5F3EE",
                borderRadius: "9999px",
                padding: "0.75rem 1.5rem",
                fontFamily: "'Geist', sans-serif",
                fontWeight: 500,
                fontSize: "0.9375rem",
                border: "none",
                cursor: isThinking || !inputText.trim() ? "not-allowed" : "pointer",
                opacity: isThinking || !inputText.trim() ? 0.6 : 1.0,
              }}
            >
              Send
            </button>
          </form>
        </section>

        {/* Telegram Remote Section */}
        <section
          style={{
            backgroundColor: "#FFFFFF",
            border: "1px solid #C8C2B5",
            borderRadius: "1rem",
            padding: "1.5rem",
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "flex-start",
              marginBottom: "1rem",
            }}
          >
            <div>
              <h2
                style={{
                  fontFamily: "'Instrument Serif', serif",
                  fontSize: "1.75rem",
                  fontWeight: 400,
                  margin: 0,
                  color: "#16181D",
                }}
              >
                Telegram Remote Control
              </h2>
              <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.875rem" }}>
                Outbound-only secure polling. No public webhooks, no port forwarding.
              </p>
            </div>

            {remoteStatus?.paired ? (
              <button
                onClick={handleDisconnect}
                style={{
                  border: "1px solid #C8C2B5",
                  backgroundColor: "#FFFFFF",
                  borderRadius: "9999px",
                  padding: "0.375rem 0.875rem",
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "11px",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                  color: "#B53A26",
                  cursor: "pointer",
                }}
              >
                Disconnect Remote
              </button>
            ) : null}
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "repeat(2, 1fr)", gap: "1rem" }}>
            <div
              style={{
                backgroundColor: "#F5F3EE",
                border: "1px solid #C8C2B5",
                borderRadius: "0.75rem",
                padding: "1rem",
              }}
            >
              <div
                style={{
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "10px",
                  textTransform: "uppercase",
                  letterSpacing: "0.08em",
                  color: "#5F626A",
                  marginBottom: "0.25rem",
                }}
              >
                Pairing Status
              </div>
              <div style={{ fontSize: "1rem", fontWeight: 600, color: "#16181D" }}>
                {remoteStatus?.paired
                  ? `Paired with User #${remoteStatus.paired_user_id}`
                  : "Unpaired (Standalone Mode)"}
              </div>
              {!remoteStatus?.paired && remoteStatus?.active_pairing_code && (
                <div style={{ marginTop: "0.5rem" }}>
                  <span
                    style={{
                      fontFamily: "'Geist Mono', monospace",
                      fontSize: "1.25rem",
                      fontWeight: 700,
                      backgroundColor: "#FFFFFF",
                      border: "1px solid #C8C2B5",
                      padding: "0.25rem 0.5rem",
                      borderRadius: "0.25rem",
                      letterSpacing: "0.1em",
                    }}
                  >
                    {remoteStatus.active_pairing_code}
                  </span>
                  <p
                    style={{
                      margin: "0.35rem 0 0",
                      color: "#86560D",
                      fontSize: "11px",
                      fontFamily: "'Geist Mono', monospace",
                    }}
                  >
                    Send /pair {remoteStatus.active_pairing_code} to bot within 5 minutes
                  </p>
                </div>
              )}
            </div>

            <div
              style={{
                backgroundColor: "#F5F3EE",
                border: "1px solid #C8C2B5",
                borderRadius: "0.75rem",
                padding: "1rem",
              }}
            >
              <div
                style={{
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "10px",
                  textTransform: "uppercase",
                  letterSpacing: "0.08em",
                  color: "#5F626A",
                  marginBottom: "0.25rem",
                }}
              >
                Inbound Long Polling
              </div>
              <div style={{ fontSize: "1rem", fontWeight: 600, color: "#16181D" }}>
                {remoteStatus?.connected ? "Active (Outbound HTTPS)" : "Disabled / Offline"}
              </div>
              <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.8125rem" }}>
                Offset persisted; replay prevented on restart
              </p>
            </div>
          </div>
        </section>

        {/* Orb Expression Preview Panel */}
        <section
          style={{
            backgroundColor: "#FFFFFF",
            border: "1px solid #C8C2B5",
            borderRadius: "1rem",
            padding: "1.5rem",
          }}
        >
          <OrbPreviewPanel />
        </section>
      </div>
    </div>
  );
};
