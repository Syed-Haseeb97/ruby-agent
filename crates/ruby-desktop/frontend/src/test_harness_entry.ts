import { renderLiquidOrbFrame } from "./LiquidOrbCanvas";

// Directly exposes the shipped renderer for acceptance fidelity testing
(window as any).renderShippedOrb = (
  op: any,
  emo: any,
  t: number,
  spin: number,
  speaking: boolean
) => {
  const canvas = document.getElementById("orb-canvas") as HTMLCanvasElement;
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  renderLiquidOrbFrame(
    ctx,
    {
      operational: op,
      emotion: emo,
      thinking_spin_active: op === "Thinking",
      speaking_active: speaking,
    },
    canvas.width,
    canvas.height,
    t,
    spin,
    false
  );
};

(window as any).orbReady = true;
