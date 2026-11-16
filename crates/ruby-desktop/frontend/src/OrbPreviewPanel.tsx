import React, { useState } from "react";
import { Emotion, OperationalState, OrbVisualState } from "./types";
import { LiquidOrbCanvas, usesBlackCore } from "./LiquidOrbCanvas";

const OPERATIONAL_STATES: OperationalState[] = [
  "Idle",
  "Listening",
  "Thinking",
  "Speaking",
  "Error",
];

const EMOTIONS: Emotion[] = [
  "Neutral",
  "Happy",
  "Excited",
  "Sad",
  "Empathetic",
  "Curious",
  "Surprised",
];

export const OrbPreviewPanel: React.FC = () => {
  const [operational, setOperational] = useState<OperationalState>("Idle");
  const [emotion, setEmotion] = useState<Emotion>("Neutral");
  const [thinkingSpin, setThinkingSpin] = useState<boolean>(true);
  const [speakingActive, setSpeakingActive] = useState<boolean>(false);

  const visualState: OrbVisualState = {
    operational,
    emotion,
    thinking_spin_active: thinkingSpin,
    speaking_active: speakingActive,
  };

  const hasBlackCore = usesBlackCore(visualState);

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "1.25rem",
        padding: "1.5rem",
        backgroundColor: "#FFFFFF",
        border: "1px solid #C8C2B5",
        borderRadius: "1rem",
        fontFamily: "'Geist', sans-serif",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div>
          <h3
            style={{
              fontFamily: "'Instrument Serif', serif",
              fontSize: "1.5rem",
              fontWeight: 400,
              margin: 0,
              color: "#16181D",
            }}
          >
            Orb Mood &amp; State Verification Matrix
          </h3>
          <p style={{ margin: "0.25rem 0 0", color: "#5F626A", fontSize: "0.875rem" }}>
            Acceptance verification against Python PyQt6 LiquidBlob at commit 7bc2c3a.
          </p>
        </div>

        {/* Static Status Capsule */}
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
            color: hasBlackCore ? "#1D6B52" : "#86560D",
          }}
        >
          <span>Black Core: {hasBlackCore ? "ENABLED" : "NONE (TRANSLUCENT)"}</span>
        </div>
      </div>

      <div style={{ display: "flex", gap: "2rem", alignItems: "center" }}>
        {/* Canvas Render Preview */}
        <div
          style={{
            width: 300,
            height: 280,
            border: "1px dashed #C8C2B5",
            borderRadius: "1rem",
            backgroundColor: "#F5F3EE",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <LiquidOrbCanvas state={visualState} width={300} height={280} />
        </div>

        {/* Controls */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "1rem" }}>
          <div>
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
              Operational States
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem" }}>
              {OPERATIONAL_STATES.map((op) => (
                <button
                  key={op}
                  onClick={() => setOperational(op)}
                  style={{
                    padding: "0.375rem 0.75rem",
                    borderRadius: "9999px",
                    border: "1px solid #C8C2B5",
                    backgroundColor: operational === op ? "#16181D" : "#FFFFFF",
                    color: operational === op ? "#F5F3EE" : "#16181D",
                    fontSize: "0.8125rem",
                    cursor: "pointer",
                    fontFamily: "'Geist', sans-serif",
                  }}
                >
                  {op}
                </button>
              ))}
            </div>
          </div>

          <div>
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
              Emotion Overrides (7 Emotional Expressions)
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "0.5rem" }}>
              {EMOTIONS.map((em) => (
                <button
                  key={em}
                  onClick={() => setEmotion(em)}
                  style={{
                    padding: "0.375rem 0.75rem",
                    borderRadius: "9999px",
                    border: "1px solid #C8C2B5",
                    backgroundColor: emotion === em ? "#16181D" : "#FFFFFF",
                    color: emotion === em ? "#F5F3EE" : "#16181D",
                    fontSize: "0.8125rem",
                    cursor: "pointer",
                    fontFamily: "'Geist', sans-serif",
                  }}
                >
                  {em}
                </button>
              ))}
            </div>
          </div>

          <div style={{ display: "flex", gap: "1.5rem", marginTop: "0.5rem" }}>
            <label style={{ display: "flex", alignItems: "center", gap: "0.5rem", fontSize: "0.875rem", color: "#16181D" }}>
              <input
                type="checkbox"
                checked={thinkingSpin}
                onChange={(e) => setThinkingSpin(e.target.checked)}
              />
              Thinking 3D Yaw Rotation
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: "0.5rem", fontSize: "0.875rem", color: "#16181D" }}>
              <input
                type="checkbox"
                checked={speakingActive}
                onChange={(e) => setSpeakingActive(e.target.checked)}
              />
              Speaking Mouth Animation
            </label>
          </div>
        </div>
      </div>
    </div>
  );
};
