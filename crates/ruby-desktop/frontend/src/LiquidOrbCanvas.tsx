/**
 * Glossy Liquid Orb Canvas 2D renderer.
 * Direct 1:1 port of PyQt6 LiquidBlob in ui/liquid_blob.py at commit 7bc2c3a.
 * Preserves exact math, constants, wave equations, 30 FPS cap, and black-core rules.
 */
import React, { useEffect, useRef } from "react";
import { OrbVisualState } from "./types";

interface Props {
  state: OrbVisualState;
  width?: number;
  height?: number;
  deterministicTime?: number;
  deterministicSpinAngle?: number;
  onHoverChange?: (hovered: boolean) => void;
}

// MOOD_COLORS matching ui/liquid_blob.py exactly
const COLOR_MAP: Record<string, [string, string]> = {
  Idle: ["#8b5cf6", "#2563eb"],
  Listening: ["#22d3ee", "#2563eb"],
  Thinking: ["#a78bfa", "#4f46e5"],
  Speaking: ["#60a5fa", "#7c3aed"],
  Happy: ["#c084fc", "#3b82f6"],
  Excited: ["#fb7185", "#7c3aed"],
  Sad: ["#60a5fa", "#60a5fa"],
  Empathetic: ["#7dd3fc", "#818cf8"],
  Curious: ["#67e8f9", "#8b5cf6"],
  Surprised: ["#a5b4fc", "#ec4899"],
  Error: ["#ff526b", "#b91235"],
};

// Exact PyQt6 primary.lighter(175) and primary.lighter(135)
const MOOD_EXACT_LIGHTER: Record<string, [string, string]> = {
  Idle: ["#ffffff", "#c6acff"],
  Listening: ["#c6f7ff", "#67ebff"],
  Thinking: ["#ffffff", "#e8e0ff"],
  Speaking: ["#ffffff", "#b4d6ff"],
  Happy: ["#ffffff", "#eddbff"],
  Excited: ["#ffffff", "#ffc7cf"],
  Sad: ["#b9ddff", "#80bdff"],
  Empathetic: ["#ffffff", "#d4f1ff"],
  Curious: ["#ffffff", "#bbf7ff"],
  Surprised: ["#ffffff", "#fcfdff"],
  Error: ["#ff9aa8", "#ff647b"],
};

export function usesBlackCore(state: OrbVisualState): boolean {
  if (state.operational === "Thinking" || state.operational === "Error") {
    return false;
  }
  if (state.emotion !== "Neutral") {
    return false;
  }
  return (
    state.operational === "Idle" ||
    state.operational === "Listening" ||
    state.operational === "Speaking"
  );
}

function hexToRgba(hex: string, alpha: number): string {
  const clean = hex.replace("#", "");
  const num = parseInt(clean, 16);
  const r = (num >> 16) & 255;
  const g = (num >> 8) & 255;
  const b = num & 255;
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function drawBodyPath(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  radius: number,
  t: number,
  mood: string,
  energy: number
) {
  const count = 120;
  const points: { x: number; y: number }[] = [];
  const bodyBreathe = Math.sin(t * (mood !== "Sad" ? 2.1 : 0.9)) * 0.018;

  for (let i = 0; i < count; i++) {
    const angle = (Math.PI * 2 * i) / count;
    const wave_a = Math.sin(3 * angle + t * 1.25) * energy;
    const wave_b = Math.sin(2 * angle - t * 0.85) * energy * 0.42;
    const wave_c = Math.cos(5 * angle + t * 0.7) * energy * 0.22;
    let deform = 1.0 + wave_a + wave_b + wave_c + bodyBreathe;

    if (mood === "Sad") {
      deform += Math.max(0, Math.sin(angle)) * 0.035;
    } else if (mood === "Happy" || mood === "Excited") {
      deform +=
        Math.sin(angle * 2 + t * (mood === "Excited" ? 2.8 : 2.0)) *
        (mood === "Excited" ? 0.026 : 0.018);
    } else if (mood === "Error") {
      deform += Math.sin(angle * 7 + t * 9.0) * 0.018;
    }

    const r = radius * deform;
    const ySadScale = mood === "Sad" ? 1.045 : 1.0;
    const px = cx + Math.cos(angle) * r;
    const py = cy + Math.sin(angle) * r * ySadScale;
    points.push({ x: px, y: py });
  }

  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  for (let i = 0; i < count; i++) {
    const p1 = points[i];
    const p2 = points[(i + 1) % count];
    ctx.quadraticCurveTo(p1.x, p1.y, (p1.x + p2.x) / 2, (p1.y + p2.y) / 2);
  }
  ctx.closePath();
}

function drawFace(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  r: number,
  t: number,
  mood: string,
  speakingActive: boolean
) {
  let gazeX = 0;
  let gazeY = 0;
  if (mood === "Listening") {
    gazeX = Math.sin(t * 2.2) * 2.5;
  } else if (mood === "Happy" || mood === "Excited") {
    gazeY = -1.5;
  } else if (mood === "Curious") {
    gazeX = r * 0.055;
    gazeY = -r * 0.035;
  } else if (mood === "Surprised") {
    gazeY = -r * 0.015;
  } else if (mood === "Sad" || mood === "Empathetic") {
    gazeY = mood === "Sad" ? 3.0 : 1.5;
  } else if (mood === "Error") {
    gazeX = Math.sin(t * 14.0) * 2.0;
  }

  const eyeY = cy - r * 0.015 + gazeY;
  const eyeDx = r * 0.31;
  const eyeW = r * (mood === "Surprised" ? 0.17 : 0.155);
  let eyeH = r * (mood === "Surprised" ? 0.34 : 0.31);

  // Periodic blink
  const blinkCycle = (t + 0.37) % 4.15;
  let blink = 1.0;
  if (blinkCycle < 0.16) {
    blink = Math.max(0.06, Math.abs(blinkCycle - 0.08) / 0.08);
  } else if (blinkCycle < 0.24) {
    blink = Math.max(0.06, (blinkCycle - 0.16) / 0.08);
  }
  if (mood !== "Happy") {
    eyeH *= blink;
  }

  if (mood === "Sad") eyeH *= 0.82;
  else if (mood === "Empathetic") eyeH *= 0.9;
  else if (mood === "Happy" || mood === "Excited") eyeH *= 0.52;
  else if (mood === "Curious") eyeH *= 1.08;
  else if (mood === "Error") eyeH *= 0.8;

  for (const sign of [-1, 1]) {
    const ex = cx + sign * eyeDx + gazeX;
    const ey = eyeY;
    ctx.save();
    ctx.translate(ex, ey);

    if (mood === "Happy" || mood === "Excited") {
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(-eyeW * 0.9, eyeH * 0.15);
      ctx.quadraticCurveTo(0, -eyeH * 0.72, eyeW * 0.9, eyeH * 0.15);
      ctx.strokeStyle = "#111018";
      ctx.lineWidth = Math.max(4.8, r * 0.065);
      ctx.stroke();

      ctx.beginPath();
      ctx.moveTo(-eyeW * 0.9, eyeH * 0.15);
      ctx.quadraticCurveTo(0, -eyeH * 0.72, eyeW * 0.9, eyeH * 0.15);
      ctx.strokeStyle = "#f7fbff";
      ctx.lineWidth = Math.max(2.6, r * 0.035);
      ctx.stroke();
    } else {
      const eyeGrad = ctx.createLinearGradient(-eyeW, -eyeH, eyeW, eyeH);
      eyeGrad.addColorStop(0, "#ffffff");
      eyeGrad.addColorStop(1, "#cfe5ff");

      ctx.fillStyle = eyeGrad;
      ctx.strokeStyle = "#111018";
      ctx.lineWidth = Math.max(1.0, r * 0.018);
      ctx.beginPath();
      ctx.ellipse(0, 0, eyeW / 2, Math.max(1, eyeH / 2), 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
    ctx.restore();
  }

  if (mood === "Speaking" || speakingActive) {
    const voice = (Math.sin(t * 11.0) + 1.0) / 2.0;
    const mouthW = r * (0.27 + voice * 0.045);
    const mouthH = r * (0.075 + Math.abs(Math.sin(t * 11.0)) * 0.095);
    const mouthGrad = ctx.createLinearGradient(
      cx - mouthW / 2,
      cy + r * 0.17,
      cx + mouthW / 2,
      cy + r * 0.17 + mouthH
    );
    mouthGrad.addColorStop(0, "#ffb4d0");
    mouthGrad.addColorStop(0.52, "#f472b6");
    mouthGrad.addColorStop(1.0, "#c75aab");

    ctx.save();
    ctx.fillStyle = mouthGrad;
    ctx.strokeStyle = "#3a1738";
    ctx.lineWidth = Math.max(1.0, r * 0.018);
    ctx.beginPath();
    ctx.ellipse(cx, cy + r * 0.17 + mouthH / 2, mouthW / 2, mouthH / 2, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  } else if (mood === "Excited") {
    const mouthW = r * 0.22;
    const mouthH = r * (0.095 + Math.abs(Math.sin(t * 4.0)) * 0.035);
    ctx.save();
    ctx.fillStyle = "#5b174e";
    ctx.strokeStyle = "#4a174d";
    ctx.lineWidth = Math.max(1.0, r * 0.018);
    ctx.beginPath();
    ctx.ellipse(cx, cy + r * 0.17 + mouthH / 2, mouthW / 2, mouthH / 2, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  } else if (mood === "Surprised") {
    const mouthW = r * 0.095;
    const mouthH = r * 0.13;
    ctx.save();
    ctx.fillStyle = "#3b1747";
    ctx.strokeStyle = "#4a174d";
    ctx.lineWidth = Math.max(1.0, r * 0.018);
    ctx.beginPath();
    ctx.ellipse(cx, cy + r * 0.17 + mouthH / 2, mouthW / 2, mouthH / 2, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  } else if (mood === "Curious") {
    const mouthW = r * 0.13;
    const mouthH = r * 0.035;
    ctx.save();
    ctx.fillStyle = "#3f255d";
    ctx.beginPath();
    ctx.ellipse(cx, cy + r * 0.2 + mouthH / 2, mouthW / 2, mouthH / 2, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  } else if (mood === "Empathetic") {
    ctx.save();
    ctx.strokeStyle = "#eff6ff";
    ctx.lineWidth = 2.0;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(cx - r * 0.11, cy + r * 0.23);
    ctx.quadraticCurveTo(cx, cy + r * 0.30, cx + r * 0.11, cy + r * 0.23);
    ctx.stroke();
    ctx.restore();
  } else if (mood === "Sad") {
    ctx.save();
    ctx.strokeStyle = "rgba(235, 240, 255, 0.75)";
    ctx.lineWidth = 2.0;
    ctx.lineCap = "round";
    ctx.beginPath();
    // Downturned sad arc matching Qt drawArc
    ctx.ellipse(cx, cy + r * 0.25, r * 0.13, r * 0.06, 0, -25 * Math.PI / 180, -155 * Math.PI / 180, true);
    ctx.stroke();
    ctx.restore();
  } else if (mood === "Error") {
    ctx.save();
    ctx.strokeStyle = "#ffe4e6";
    ctx.lineWidth = 2.2;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(cx - r * 0.09, cy + r * 0.22);
    ctx.lineTo(cx + r * 0.09, cy + r * 0.22);
    ctx.stroke();
    ctx.restore();
  }
}

export function renderLiquidOrbFrame(
  ctx: CanvasRenderingContext2D,
  state: OrbVisualState,
  width: number,
  height: number,
  t: number,
  spinAngle: number = 0,
  hovered: boolean = false
): void {
  const w = width;
  const h = height;

  ctx.clearRect(0, 0, w, h);

  // Center and base radius
  let cx = w / 2;
  let cy = h / 2 - 6;
  let radius = Math.min(w * 0.34, h * 0.34, 150.0);
  if (hovered) {
    radius *= 1.025;
  }

  // Active mood determination
  const effectiveMood: string =
    state.operational === "Thinking"
      ? "Thinking"
      : state.operational === "Error"
      ? "Error"
      : state.emotion !== "Neutral"
      ? state.emotion
      : state.operational;

  // Vertical bounce / breathing
  const breathe = Math.sin(
    t * (effectiveMood !== "Sad" && effectiveMood !== "Empathetic" ? 1.9 : 0.8)
  );
  if (effectiveMood === "Happy" || effectiveMood === "Excited") {
    cy -= Math.abs(
      Math.sin(t * (effectiveMood === "Excited" ? 4.2 : 3.2)) *
        (effectiveMood === "Excited" ? 7 : 5)
    );
  } else if (effectiveMood === "Sad") {
    cy += 3 + Math.abs(breathe) * 3;
  } else if (effectiveMood === "Error") {
    cx += Math.sin(t * 12.0) * 2.0;
  }

  const [primary, secondary] = COLOR_MAP[effectiveMood] || COLOR_MAP.Idle;

  // 1. Ambient Halo (omitted in Error)
  if (effectiveMood !== "Error") {
    const haloAlpha = effectiveMood !== "Sad" ? 55 / 255 : 28 / 255;
    const haloGrad = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius * 1.55);
    haloGrad.addColorStop(0, hexToRgba(primary, haloAlpha));
    haloGrad.addColorStop(0.52, hexToRgba(secondary, 25 / 255));
    haloGrad.addColorStop(1, hexToRgba(secondary, 0));

    ctx.save();
    ctx.fillStyle = haloGrad;
    ctx.beginPath();
    ctx.arc(cx, cy, radius * 1.55, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();

    // 2. Grounding Shadow (QRectF(cx - radius, cy - radius * 0.4, radius * 2, radius * 1.6))
    const shadowGrad = ctx.createRadialGradient(
      cx,
      cy + radius * 0.48,
      0,
      cx,
      cy + radius * 0.48,
      radius * 0.95
    );
    shadowGrad.addColorStop(0, "rgba(0, 0, 0, 0.392)");
    shadowGrad.addColorStop(1, "rgba(0, 0, 0, 0)");

    ctx.save();
    ctx.fillStyle = shadowGrad;
    ctx.beginPath();
    ctx.ellipse(cx, cy + radius * 0.4, radius, radius * 0.8, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  // 3. Deforming Body Silhouette
  let energy = 0.025;
  if (effectiveMood === "Happy") energy = 0.065;
  else if (effectiveMood === "Excited") energy = 0.085;
  else if (effectiveMood === "Curious") energy = 0.035;
  else if (effectiveMood === "Surprised") energy = 0.055;
  else if (effectiveMood === "Empathetic") energy = 0.022;
  else if (effectiveMood === "Listening") energy = 0.038;
  else if (effectiveMood === "Thinking") energy = 0.012;
  else if (effectiveMood === "Sad") energy = 0.018;
  else if (effectiveMood === "Error") energy = 0.07;

  ctx.save();
  drawBodyPath(ctx, cx, cy, radius, t, effectiveMood, energy);

  // Body Gradient Fill
  let highlightX = cx + Math.cos(t * 0.72) * radius * 0.24;
  let highlightY = cy + Math.sin(t * 0.58) * radius * 0.22;
  if (effectiveMood === "Thinking") {
    const yawShine = (spinAngle * Math.PI) / 180;
    highlightX = cx + Math.sin(yawShine) * radius * 0.24;
    highlightY = cy - radius * 0.28;
  }

  const bodyGrad = ctx.createRadialGradient(
    highlightX,
    highlightY,
    0,
    highlightX,
    highlightY,
    radius * 1.38
  );

  if (effectiveMood === "Sad") {
    bodyGrad.addColorStop(0.0, "#b9ddff");
    bodyGrad.addColorStop(0.22, "#80bdff");
    bodyGrad.addColorStop(0.58, "#60a5fa");
    bodyGrad.addColorStop(1.0, "#60a5fa");
  } else if (effectiveMood === "Error") {
    bodyGrad.addColorStop(0.0, "#ff9aa8");
    bodyGrad.addColorStop(0.18, "#ff647b");
    bodyGrad.addColorStop(0.42, "#ff304f");
    bodyGrad.addColorStop(0.72, "#e7193c");
    bodyGrad.addColorStop(1.0, "#b91235");
  } else {
    const exact = MOOD_EXACT_LIGHTER[effectiveMood] || MOOD_EXACT_LIGHTER.Idle;
    bodyGrad.addColorStop(0.0, exact[0]);
    bodyGrad.addColorStop(0.22, exact[1]);
    bodyGrad.addColorStop(0.52, primary);
    bodyGrad.addColorStop(0.82, secondary);
    bodyGrad.addColorStop(1.0, secondary);
  }

  ctx.fillStyle = bodyGrad;
  ctx.fill();
  ctx.strokeStyle = hexToRgba(primary, 185 / 255);
  ctx.lineWidth = 1.4;
  ctx.stroke();
  ctx.restore();

  // 4. Thinking Faceless Rotating Sphere Meridian Shading
  if (effectiveMood === "Thinking") {
    ctx.save();
    const yaw = (spinAngle * Math.PI) / 180;
    const side = Math.sin(yaw);

    const bandX = cx + side * radius * 0.5;
    const bandGrad = ctx.createRadialGradient(
      bandX,
      cy - radius * 0.05,
      0,
      bandX,
      cy - radius * 0.05,
      radius * 0.95
    );
    bandGrad.addColorStop(0.0, "rgba(255, 255, 255, 0.243)");
    bandGrad.addColorStop(0.42, "rgba(210, 205, 255, 0.094)");
    bandGrad.addColorStop(1.0, "rgba(10, 8, 35, 0)");
    ctx.fillStyle = bandGrad;
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, Math.PI * 2);
    ctx.fill();

    const shadeX = cx - side * radius * 0.42;
    const shadeGrad = ctx.createRadialGradient(shadeX, cy, 0, shadeX, cy, radius * 1.05);
    const alphaEdge = (80 + 65 * Math.abs(side)) / 255;
    const alphaRim = (115 + 70 * Math.abs(side)) / 255;
    shadeGrad.addColorStop(0.0, "rgba(18, 12, 55, 0)");
    shadeGrad.addColorStop(0.58, "rgba(18, 12, 55, 0.039)");
    shadeGrad.addColorStop(0.84, `rgba(8, 6, 30, ${alphaEdge})`);
    shadeGrad.addColorStop(1.0, `rgba(5, 4, 20, ${alphaRim})`);
    ctx.fillStyle = shadeGrad;
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  // Inner edge and reflected light
  ctx.save();
  drawBodyPath(ctx, cx - 1.5, cy - 2.0, radius * 0.955, t + 0.08, effectiveMood, energy);
  ctx.strokeStyle = "rgba(255, 255, 255, 0.165)";
  ctx.lineWidth = 1.0;
  ctx.stroke();
  ctx.restore();

  // 5. Dark Face Core (Idle, Listening, Speaking)
  const hasCore = usesBlackCore(state);
  const coreRadius = radius * 0.72;

  if (hasCore) {
    ctx.save();
    const coreGrad = ctx.createRadialGradient(
      cx - coreRadius * 0.32,
      cy - coreRadius * 0.42,
      0,
      cx - coreRadius * 0.32,
      cy - coreRadius * 0.42,
      coreRadius * 1.55
    );
    coreGrad.addColorStop(0.0, "#30313a");
    coreGrad.addColorStop(0.22, "#17181e");
    coreGrad.addColorStop(0.68, "#08090d");
    coreGrad.addColorStop(0.94, "#020305");
    coreGrad.addColorStop(1.0, "#000000");

    ctx.fillStyle = coreGrad;
    ctx.strokeStyle = "#555965";
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(cx, cy, coreRadius, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }

  // 6. Facial Expressions (Thinking is strictly faceless)
  if (effectiveMood !== "Thinking") {
    const faceRadius = hasCore ? coreRadius * 0.88 : radius;
    drawFace(ctx, cx, cy, faceRadius, t, effectiveMood, state.speaking_active);
  }
}

export const LiquidOrbCanvas: React.FC<Props> = ({
  state,
  width = 300,
  height = 280,
  deterministicTime,
  deterministicSpinAngle,
  onHoverChange,
}) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const hoveredRef = useRef<boolean>(false);
  const spinAngleRef = useRef<number>(0);
  const lastTickRef = useRef<number>(performance.now() / 1000);
  const startedAtRef = useRef<number>(performance.now() / 1000);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animId: number;
    let lastRenderTime = performance.now();
    const frameInterval = 1000 / 30; // 30 FPS cap (~33.3ms)

    const render = () => {
      animId = requestAnimationFrame(render);
      const now = performance.now();
      const elapsed = now - lastRenderTime;
      if (elapsed < frameInterval) return;
      lastRenderTime = now - (elapsed % frameInterval);

      const nowSec = now / 1000;
      const dt = Math.min(0.05, Math.max(0, nowSec - lastTickRef.current));
      lastTickRef.current = nowSec;

      // Thinking 300 deg/sec yaw spin
      if (state.operational === "Thinking" && state.thinking_spin_active && dt > 0) {
        spinAngleRef.current = (spinAngleRef.current + 300.0 * dt) % 360.0;
      }

      const t = deterministicTime !== undefined ? deterministicTime : nowSec - startedAtRef.current;
      const spinAngle = deterministicSpinAngle !== undefined ? deterministicSpinAngle : spinAngleRef.current;

      renderLiquidOrbFrame(ctx, state, width, height, t, spinAngle, hoveredRef.current);
    };

    animId = requestAnimationFrame(render);

    const handleMouseEnter = () => {
      hoveredRef.current = true;
      onHoverChange?.(true);
    };

    const handleMouseLeave = () => {
      hoveredRef.current = false;
      onHoverChange?.(false);
    };

    canvas.addEventListener("mouseenter", handleMouseEnter);
    canvas.addEventListener("mouseleave", handleMouseLeave);

    return () => {
      cancelAnimationFrame(animId);
      canvas.removeEventListener("mouseenter", handleMouseEnter);
      canvas.removeEventListener("mouseleave", handleMouseLeave);
    };
  }, [state, width, height, deterministicTime, deterministicSpinAngle, onHoverChange]);

  return (
    <canvas
      ref={canvasRef}
      id="orb-canvas"
      width={width}
      height={height}
      style={{ display: "block" }}
    />
  );
};
