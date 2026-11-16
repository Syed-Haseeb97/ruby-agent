import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LiquidOrbCanvas } from "./LiquidOrbCanvas";
import { OrbVisualState, SequencedOrbEvent } from "./types";

export const OrbView: React.FC = () => {
  const [orbState, setOrbState] = useState<OrbVisualState>({
    operational: "Idle",
    emotion: "Neutral",
    thinking_spin_active: false,
    speaking_active: false,
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    invoke<SequencedOrbEvent>("get_orb_state")
      .then((res) => {
        if (res && res.state) {
          setOrbState(res.state);
        }
      })
      .catch(() => {});

    listen<SequencedOrbEvent>("orb_state_changed", (ev) => {
      setOrbState(ev.payload.state);
    })
      .then((u) => {
        unlisten = u;
      })
      .catch(() => {});

    return () => {
      unlisten?.();
    };
  }, []);

  return (
    <div
      style={{
        width: 300,
        height: 280,
        background: "transparent",
        overflow: "hidden",
      }}
    >
      <LiquidOrbCanvas state={orbState} width={300} height={280} />
    </div>
  );
};
