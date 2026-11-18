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

interface ComputerStepItem {
  step: number;
  timestamp: string;
  action_type: string;
  coordinates: [number, number] | null;
  target_window_title: string;
  status: string;
}

interface ComputerUseStatus {
  is_acting: boolean;
  current_goal: string | null;
  step_count: number;
  dry_run: boolean;
  recent_steps: ComputerStepItem[];
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

  // Milestone M4: Computer Use Operator State
  const [cuGoal, setCuGoal] = useState("");
  const [cuDryRun, setCuDryRun] = useState(false);
  const [cuIsActing, setCuIsActing] = useState(false);
  const [cuCurrentGoal, setCuCurrentGoal] = useState<string | null>(null);
  const [cuSteps, setCuSteps] = useState<ComputerStepItem[]>([]);
  const [cuConfirmation, setCuConfirmation] = useState<{
    code: string;
    tool_name: string;
    description: string;
  } | null>(null);
  const [cuConfirmCodeInput, setCuConfirmCodeInput] = useState("");

  const fetchCuStatus = async () => {
    try {
      const status = await invoke<ComputerUseStatus>("get_computer_use_status");
      setCuIsActing(status.is_acting);
      setCuCurrentGoal(status.current_goal);
      if (status.recent_steps && status.recent_steps.length > 0) {
        setCuSteps(status.recent_steps);
      }
    } catch {
      // Ignore
    }
  };

  const handleStartComputerUse = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!cuGoal.trim()) return;
    try {
      await invoke("start_computer_use_task", { goal: cuGoal.trim(), dryRun: cuDryRun });
      setCuIsActing(true);
      setCuCurrentGoal(cuGoal.trim());
      setNotice(`Mode A Computer Use initiated for "${cuGoal.trim()}"`);
      setCuGoal("");
    } catch (err) {
      setNotice(`Failed to start Computer Use: ${String(err)}`);
    }
  };

  const handleStopComputerUse = async () => {
    try {
      await invoke("stop_computer_use_task");
      setCuIsActing(false);
      setCuCurrentGoal(null);
      setNotice("Kill Switch: Computer Use operator halted immediately.");
    } catch (err) {
      setNotice(`Stop error: ${String(err)}`);
    }
  };

  const handleConfirmCode = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!cuConfirmCodeInput.trim()) return;
    try {
      const res = await invoke<string>("confirm_tool_execution", {
        code: cuConfirmCodeInput.trim(),
      });
      setNotice(`Action confirmed: ${res}`);
      setCuConfirmation(null);
      setCuConfirmCodeInput("");
    } catch (err) {
      setNotice(`Confirmation rejected: ${String(err)}`);
    }
  };

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
        if (cuIsActing) {
          handleStopComputerUse();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [cuIsActing]);

  useEffect(() => {
    fetchRemoteStatus();
    fetchCuStatus();
    const interval = setInterval(() => {
      fetchRemoteStatus();
      fetchCuStatus();
    }, 4000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    let unlistenNotice: (() => void) | undefined;
    let unlistenToast: (() => void) | undefined;
    let unlistenCuStart: (() => void) | undefined;
    let unlistenCuStop: (() => void) | undefined;
    let unlistenToolConf: (() => void) | undefined;

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

    listen<{ task_id: string; goal: string; dry_run: boolean }>(
      "computer_use_started",
      (ev) => {
        setCuIsActing(true);
        setCuCurrentGoal(ev.payload.goal);
        setNotice(`Operator Acting: "${ev.payload.goal}"`);
      }
    ).then((un) => {
      unlistenCuStart = un;
    });

    listen<{ reason: string }>("computer_use_stopped", (ev) => {
      setCuIsActing(false);
      setCuCurrentGoal(null);
      setNotice(`Operator Halted: ${ev.payload.reason}`);
    }).then((un) => {
      unlistenCuStop = un;
    });

    listen<{ code: string; tool_name: string; description: string }>(
      "tool_confirmation_requested",
      (ev) => {
        setCuConfirmation(ev.payload);
        setCuConfirmCodeInput(ev.payload.code);
      }
    ).then((un) => {
      unlistenToolConf = un;
    });

    return () => {
      unlistenNotice?.();
      unlistenToast?.();
      unlistenCuStart?.();
      unlistenCuStop?.();
      unlistenToolConf?.();
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

        {/* Milestone M4 Live Computer Use (Mode A) Section */}
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
              <div
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  borderRadius: "9999px",
                  border: "1px solid #C8C2B5",
                  backgroundColor: cuIsActing ? "#F6ECD9" : "#E3EFE8",
                  padding: "0.25rem 0.875rem",
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "11px",
                  textTransform: "uppercase",
                  letterSpacing: "0.08em",
                  color: cuIsActing ? "#86560D" : "#1D6B52",
                  fontWeight: 600,
                  marginBottom: "0.5rem",
                }}
              >
                {cuIsActing ? "OPERATOR ACTIVE (MODE A)" : "LIVE OPERATOR IDLE"}
              </div>
              <h2
                style={{
                  fontFamily: "'Instrument Serif', serif",
                  fontSize: "1.75rem",
                  margin: "0 0 0.25rem",
                  color: "#16181D",
                  lineHeight: 1.0,
                }}
              >
                Computer Use Operator
              </h2>
              <p style={{ margin: 0, color: "#5F626A", fontSize: "0.875rem" }}>
                Autonomous desktop operator guided by Gemini vision reasoning and native Win32 SendInput.
              </p>
            </div>

            <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
              {cuIsActing && (
                <button
                  type="button"
                  onClick={handleStopComputerUse}
                  style={{
                    backgroundColor: "#FFFFFF",
                    border: "1px solid #B53A26",
                    color: "#B53A26",
                    padding: "0.5rem 1rem",
                    borderRadius: "9999px",
                    fontWeight: 600,
                    fontSize: "0.8125rem",
                    cursor: "pointer",
                  }}
                >
                  Kill Switch (Esc)
                </button>
              )}
            </div>
          </div>

          {/* Pending Consequential Confirmation Banner */}
          {cuConfirmation && (
            <div
              style={{
                backgroundColor: "#F8E9E4",
                border: "1px solid #B53A26",
                borderRadius: "0.75rem",
                padding: "1rem",
                marginBottom: "1rem",
              }}
            >
              <div
                style={{
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "11px",
                  textTransform: "uppercase",
                  letterSpacing: "0.08em",
                  color: "#B53A26",
                  fontWeight: 700,
                  marginBottom: "0.25rem",
                }}
              >
                ⚠️ Consequential Action Confirmation Required
              </div>
              <div style={{ color: "#16181D", fontSize: "0.875rem", marginBottom: "0.5rem" }}>
                Tool <strong>{cuConfirmation.tool_name}</strong>: {cuConfirmation.description}
              </div>
              <form onSubmit={handleConfirmCode} style={{ display: "flex", gap: "0.5rem" }}>
                <input
                  type="text"
                  value={cuConfirmCodeInput}
                  onChange={(e) => setCuConfirmCodeInput(e.target.value)}
                  placeholder="Enter 4-digit code (e.g. 1234)"
                  style={{
                    fontFamily: "'Geist Mono', monospace",
                    padding: "0.375rem 0.75rem",
                    borderRadius: "0.375rem",
                    border: "1px solid #C8C2B5",
                    fontSize: "0.875rem",
                    width: "160px",
                  }}
                />
                <button
                  type="submit"
                  style={{
                    backgroundColor: "#16181D",
                    color: "#F5F3EE",
                    border: "none",
                    borderRadius: "9999px",
                    padding: "0.375rem 1rem",
                    fontWeight: 600,
                    fontSize: "0.8125rem",
                    cursor: "pointer",
                  }}
                >
                  Confirm (/yes)
                </button>
                <button
                  type="button"
                  onClick={() => setCuConfirmation(null)}
                  style={{
                    backgroundColor: "#FFFFFF",
                    color: "#5F626A",
                    border: "1px solid #C8C2B5",
                    borderRadius: "9999px",
                    padding: "0.375rem 1rem",
                    fontWeight: 500,
                    fontSize: "0.8125rem",
                    cursor: "pointer",
                  }}
                >
                  Dismiss (/no)
                </button>
              </form>
            </div>
          )}

          {/* Task Dispatch Form */}
          <form
            onSubmit={handleStartComputerUse}
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "0.75rem",
              marginBottom: "1.25rem",
            }}
          >
            <div style={{ display: "flex", gap: "0.5rem" }}>
              <input
                type="text"
                value={cuGoal}
                onChange={(e) => setCuGoal(e.target.value)}
                placeholder="Enter autonomous desktop goal (e.g. 'open notepad and type meeting minutes')"
                disabled={cuIsActing}
                style={{
                  flex: 1,
                  padding: "0.625rem 1rem",
                  borderRadius: "9999px",
                  border: "1px solid #C8C2B5",
                  backgroundColor: cuIsActing ? "#EAE7DF" : "#FFFFFF",
                  color: "#16181D",
                  fontSize: "0.875rem",
                  outline: "none",
                }}
              />
              <button
                type="submit"
                disabled={cuIsActing || !cuGoal.trim()}
                style={{
                  backgroundColor: cuIsActing || !cuGoal.trim() ? "#C8C2B5" : "#16181D",
                  color: "#F5F3EE",
                  border: "none",
                  borderRadius: "9999px",
                  padding: "0.625rem 1.5rem",
                  fontWeight: 600,
                  fontSize: "0.875rem",
                  cursor: cuIsActing || !cuGoal.trim() ? "not-allowed" : "pointer",
                }}
              >
                Execute
              </button>
            </div>

            <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
              <input
                type="checkbox"
                id="dryRunCheckbox"
                checked={cuDryRun}
                onChange={(e) => setCuDryRun(e.target.checked)}
                disabled={cuIsActing}
              />
              <label
                htmlFor="dryRunCheckbox"
                style={{
                  fontSize: "0.8125rem",
                  color: "#5F626A",
                  cursor: "pointer",
                }}
              >
                Dry-Run Mode (Observe and record proposed actions without dispatching SendInput)
              </label>
            </div>
          </form>

          {/* Active Goal Pill */}
          {cuCurrentGoal && (
            <div
              style={{
                backgroundColor: "#F6ECD9",
                border: "1px solid #C8C2B5",
                borderRadius: "0.5rem",
                padding: "0.75rem 1rem",
                marginBottom: "1rem",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
              }}
            >
              <div>
                <span
                  style={{
                    fontFamily: "'Geist Mono', monospace",
                    fontSize: "10px",
                    textTransform: "uppercase",
                    letterSpacing: "0.08em",
                    color: "#86560D",
                    fontWeight: 700,
                    marginRight: "0.5rem",
                  }}
                >
                  ACTIVE GOAL:
                </span>
                <span style={{ fontSize: "0.875rem", color: "#16181D", fontWeight: 500 }}>
                  "{cuCurrentGoal}"
                </span>
              </div>
              <span
                style={{
                  fontFamily: "'Geist Mono', monospace",
                  fontSize: "11px",
                  color: "#86560D",
                  fontWeight: 600,
                }}
              >
                KILL SWITCH: ESC
              </span>
            </div>
          )}

          {/* Step History & Audit Log */}
          <div>
            <div
              style={{
                fontFamily: "'Geist Mono', monospace",
                fontSize: "11px",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                color: "#5F626A",
                marginBottom: "0.5rem",
              }}
            >
              Live Operator Step Log ({cuSteps.length})
            </div>

            {cuSteps.length === 0 ? (
              <div
                style={{
                  backgroundColor: "#F5F3EE",
                  border: "1px dashed #C8C2B5",
                  borderRadius: "0.75rem",
                  padding: "1.5rem",
                  textAlign: "center",
                  color: "#5F626A",
                  fontSize: "0.8125rem",
                }}
              >
                No live operator steps currently recorded. Submit a goal above to start.
              </div>
            ) : (
              <div
                style={{
                  maxHeight: "220px",
                  overflowY: "auto",
                  border: "1px solid #C8C2B5",
                  borderRadius: "0.75rem",
                  backgroundColor: "#FFFFFF",
                }}
              >
                <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.8125rem" }}>
                  <thead>
                    <tr
                      style={{
                        backgroundColor: "#F5F3EE",
                        borderBottom: "1px solid #C8C2B5",
                        textAlign: "left",
                        fontFamily: "'Geist Mono', monospace",
                        fontSize: "10px",
                        textTransform: "uppercase",
                        letterSpacing: "0.05em",
                        color: "#5F626A",
                      }}
                    >
                      <th style={{ padding: "0.5rem 0.75rem" }}>Step</th>
                      <th style={{ padding: "0.5rem 0.75rem" }}>Action</th>
                      <th style={{ padding: "0.5rem 0.75rem" }}>Target Window</th>
                      <th style={{ padding: "0.5rem 0.75rem" }}>Coord</th>
                      <th style={{ padding: "0.5rem 0.75rem" }}>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cuSteps.map((s, idx) => (
                      <tr
                        key={idx}
                        style={{
                          borderBottom: idx === cuSteps.length - 1 ? "none" : "1px solid #EAE7DF",
                        }}
                      >
                        <td
                          style={{
                            padding: "0.5rem 0.75rem",
                            fontFamily: "'Geist Mono', monospace",
                            color: "#5F626A",
                          }}
                        >
                          #{s.step}
                        </td>
                        <td style={{ padding: "0.5rem 0.75rem", fontWeight: 600, color: "#16181D" }}>
                          {s.action_type}
                        </td>
                        <td style={{ padding: "0.5rem 0.75rem", color: "#5F626A" }}>
                          {s.target_window_title}
                        </td>
                        <td
                          style={{
                            padding: "0.5rem 0.75rem",
                            fontFamily: "'Geist Mono', monospace",
                            color: "#5F626A",
                          }}
                        >
                          {s.coordinates ? `(${s.coordinates[0]}, ${s.coordinates[1]})` : "—"}
                        </td>
                        <td style={{ padding: "0.5rem 0.75rem" }}>
                          <span
                            style={{
                              fontFamily: "'Geist Mono', monospace",
                              fontSize: "10px",
                              padding: "0.125rem 0.375rem",
                              borderRadius: "0.25rem",
                              backgroundColor:
                                s.status === "Executed"
                                  ? "#E3EFE8"
                                  : s.status === "DryRun"
                                  ? "#F6ECD9"
                                  : "#F8E9E4",
                              color:
                                s.status === "Executed"
                                  ? "#1D6B52"
                                  : s.status === "DryRun"
                                  ? "#86560D"
                                  : "#B53A26",
                              fontWeight: 600,
                            }}
                          >
                            {s.status}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
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
