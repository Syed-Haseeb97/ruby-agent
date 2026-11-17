import React, { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { OrbPreviewPanel } from "./OrbPreviewPanel";

export const ConsoleView: React.FC = () => {
  const [notice, setNotice] = useState<string | null>(null);

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
            {/* Static Eyebrow Status Capsule (Strictly no blinking/pulsing dots) */}
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
              <span>TAURI 2.0 • M0 FOUNDATION READY</span>
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
              Wispr Dictation Hotkey
            </div>
            <div style={{ fontSize: "1.125rem", fontWeight: 600, color: "#16181D" }}>
              Ctrl + Win
            </div>
            <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.8125rem" }}>
              WH_KEYBOARD_LL low-level hook
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
              Emergency Kill Switch
            </div>
            <div style={{ fontSize: "1.125rem", fontWeight: 600, color: "#B53A26" }}>
              Ctrl+Alt+Shift+R
            </div>
            <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.8125rem" }}>
              Instant process &amp; input release
            </p>
          </div>
        </div>

        {/* Orb Verification & Acceptance Section */}
        <section>
          <OrbPreviewPanel />
        </section>

        {/* System Parity Architecture Summary */}
        <section
          style={{
            backgroundColor: "#FFFFFF",
            border: "1px solid #C8C2B5",
            borderRadius: "1rem",
            padding: "1.5rem",
          }}
        >
          <h2
            style={{
              fontFamily: "'Instrument Serif', serif",
              fontSize: "1.5rem",
              fontWeight: 400,
              margin: "0 0 1rem",
              color: "#16181D",
            }}
          >
            M0 Architecture Verification Summary
          </h2>
          <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem", fontSize: "0.875rem", color: "#5F626A" }}>
            <div>
              <strong style={{ color: "#16181D" }}>Orb Transparency &amp; Non-Activating:</strong>{" "}
              Excluded from screen captures via <code>SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)</code>,{" "}
              <code>WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE</code>, and DWM shadow disabled.
            </div>
            <div>
              <strong style={{ color: "#16181D" }}>Canvas 2D Fidelity:</strong> 1:1 port of PyQt6 LiquidBlob{" "}
              (commit <code>7bc2c3a</code>). Preserves exact wave equations, 30 FPS cap, Thinking 3D yaw, and the strict black-core rule.
            </div>
            <div>
              <strong style={{ color: "#16181D" }}>Typed IPC Bridge:</strong> Sequence-numbered events{" "}
              (<code>seq: u64</code>) preventing stale responses; only operational state and emotion labels cross the orb IPC boundary.
            </div>
          </div>
        </section>
      </div>
    </div>
  );
};
