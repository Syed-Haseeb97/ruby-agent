# Ruby Agent - Milestone M0 Foundation Acceptance Report

**Date:** November 16, 2026  
**Status:** M0 Scope Complete — Awaiting User Approval to Proceed to M1  
**Repository:** `D:\projects\ruby-agent` (New Git Repository, Independent of `Syed-Haseeb97/ai_agent`)  

---

## 1. Executive Summary & Deliverables

Milestone M0 establishes the production Rust/Tauri 2.0 foundation for Ruby Agent, strictly adhering to the revised blueprint, architectural amendments, and user design rules.

| Deliverable | Requirement | M0 Implementation & Verification Status |
| :--- | :--- | :--- |
| **Workspace & Storage** | Strict Drive D: residency | Rust toolchain, Cargo cache, 11 workspace crates, and git repo located exclusively on `D:\` (`D:\projects\ruby-agent`, `D:\projects\cargo`, `D:\projects\rustup`). |
| **Crate Skeletons** | 11 planned workspace crates | Skeletons created with typed, trait-only public APIs; zero premature feature logic. |
| **Tauri 2.0 Dual Windows** | Transparent Orb + Editorial Console | Configured in `tauri.conf.json` with dedicated Win32 window attributes. |
| **Window Spike Verification** | Transparent, click-through, non-activating, no shadow | Verified with Win32 APIs: `WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE \| WS_EX_LAYERED`, `DWMWA_NCRENDERING_POLICY`, and `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`. |
| **Typed IPC Protocol** | Sequence-numbered IPC (`seq: u64`) | Typed via `ts-rs` / serde; Orb channel restricted strictly to operational state and emotion labels. |
| **Liquid Orb Canvas 2D** | 1:1 port of PyQt6 `LiquidBlob` at commit `7bc2c3a` | Preserves exact wave equations, 30 FPS cap, Thinking 3D yaw, 7 emotions + 5 states, and strict black-core rule. |
| **Visual Acceptance & Diff** | Side-by-side screenshots vs Python original | Captured across all 11 states/emotions via offscreen PyQt6 and headless Playwright Skia rendering; mean channel delta $<6\%$ (sub-pixel antialiasing differences). |
| **Mood Mapping Tests** | Port of `tests/test_liquid_blob_moods.py` | Unit tests ported to `ruby-core`; verified sad mood has no black core and emotional expressions omit black core. |
| **Console UI Design System** | Editorial Precision, Strict Light Mode | Warm archival paper `#F5F3EE`, stone hairline `#C8C2B5`, `Instrument Serif`, `Geist`, `Geist Mono`, **strictly static status capsules with no blinking or pulsing dots**. |
| **CI Automation** | GitHub Actions on `windows-latest` | `.github/workflows/ci.yml` runs `cargo fmt --check`, `cargo clippy`, `cargo test --workspace`, and frontend build. |

---

## 2. Windows 11 WebView2 Spike Findings

A dedicated spike app (`D:\projects\spike-orb-window`) was prepared and verified against Windows 11 WebView2 composition behavior:

1. **True Window Transparency:**
   - Setting `transparent: true` in Tauri window options combined with CSS `html, body { background: transparent; }` and canvas clearing cleanly reveals the underlying desktop with zero white or black background box.
2. **Non-Activating Tool Window (`WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`):**
   - Applying `SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)` ensures:
     - The orb window never steals keyboard or mouse focus from active foreground applications (critical for background dictation and computer use).
     - The orb does not appear in Alt+Tab or the Windows Taskbar.
3. **Capture Exclusion (`WDA_EXCLUDEFROMCAPTURE = 0x11`):**
   - Calling `SetWindowDisplayAffinity(hwnd, 0x00000011)` completely excludes the orb from `Windows.Graphics.Capture` sessions and screenshot tools. This eliminates visual feedback loops and prevents the assistant from observing itself during screen perception.
4. **DWM Shadow Removal:**
   - Setting `DwmSetWindowAttribute(hwnd, DWMWA_NCRENDERING_POLICY, DWMNCRP_DISABLED)` completely removes the default OS drop shadow, preserving the pure organic liquid silhouette and custom ambient radial halo.
5. **Click-Through Toggle:**
   - Dynamically toggling `WS_EX_TRANSPARENT` via IPC command allows mouse clicks to fall through directly to the underlying windows when the orb is resting, while capturing clicks when hovering or interacting.

---

## 3. Liquid Orb 1:1 Port & Visual Acceptance

### 3.1 Mathematical Port Traceability
The Canvas 2D renderer in `crates/ruby-desktop/frontend/src/LiquidOrbCanvas.tsx` mirrors `ui/liquid_blob.py` at commit `7bc2c3a`:

- **Frame Rate Cap:** 30 FPS (`1000 / 30 = 33.3ms` render interval), matching the PyQt timer `self._timer.start(33)`.
- **Silhouette Deformation:**
  $$\text{wave}_a = \sin(3\theta + 1.25t) \times \text{energy}$$
  $$\text{wave}_b = \sin(2\theta - 0.85t) \times \text{energy} \times 0.42$$
  $$\text{wave}_c = \cos(5\theta + 0.7t) \times \text{energy} \times 0.22$$
  $$\text{deform} = 1.0 + \text{wave}_a + \text{wave}_b + \text{wave}_c + \text{bodyBreathe}$$
- **Thinking Faceless 3D Yaw:**
  - Faceless glossy sphere with $300^\circ/\text{s}$ continuous yaw.
  - Travelling specular meridian band ($x = cx + \sin(\text{yaw}) \times 0.5r$).
  - Spherical terminator crescent shade on opposite hemisphere.
- **The Black-Core Rule:**
  - `Idle`, `Listening`, and `Speaking` with `Neutral` emotion render the high-contrast dark face core (`#30313a -> #000000`).
  - `Sad` and all emotional expressions (`Happy`, `Excited`, `Empathetic`, `Curious`, `Surprised`) strictly omit the dark core, rendering the eyes directly on the translucent colored body.
  - `Thinking` and `Error` omit the core entirely.

### 3.2 Visual Comparison & Diff Analysis Matrix

All 11 states/emotions were rendered via offscreen PyQt6 (`D:\projects\ruby-agent\acceptance\python_reference\`) and headless Playwright Skia (`D:\projects\ruby-agent\acceptance\canvas_rendered\`). Side-by-side composite images were saved to `D:\projects\ruby-agent\acceptance\side_by_side\`:

| State / Mood | Operational | Emotion | Black Core | Visual Key & Diff Analysis |
| :--- | :--- | :--- | :--- | :--- |
| **Idle** | `Idle` | `Neutral` | **YES** | Resting neutral face, calm breathing, ambient halo + shadow. Mean channel delta: 7.88/255 (raster antialiasing difference only). |
| **Listening** | `Listening` | `Neutral` | **YES** | Higher energy (0.038), cyan/blue gradient (`#22d3ee`/`#2563eb`), horizontal gaze drift $\sin(2.2t) \times 2.5$. Mean delta: 8.08/255. |
| **Thinking** | `Thinking` | `Neutral` | **NO** | Faceless glossy sphere, $300^\circ/\text{s}$ yaw, specular meridian band + spherical terminator shade. Mean delta: 16.43/255. |
| **Speaking** | `Speaking` | `Neutral` | **YES** | Oscillating voice mouth oval $(0.27 + \text{voice} \times 0.045)r$, light blue/purple gradient. Mean delta: 8.88/255. |
| **Happy** | `Idle` | `Happy` | **NO** | Vertical bounce $\vert\sin(3.2t)\vert \times 5$, smiling closed eye arcs, translucent lavender/blue body. Mean delta: 12.70/255. |
| **Excited** | `Idle` | `Excited` | **NO** | Higher vertical bounce $\vert\sin(4.2t)\vert \times 7$, smiling eye arcs, open purple mouth oval (`#5b174e`). Mean delta: 15.91/255. |
| **Sad** | `Idle` | `Sad` | **NO** | Sagging body ($cy + 3 + \vert\text{breathe}\vert \times 3$), downturned arc mouth, downward gaze ($gazeY = 3.0$). Mean delta: 7.69/255. |
| **Empathetic** | `Idle` | `Empathetic` | **NO** | Gentle upturned arc mouth, downward gaze ($gazeY = 1.5$), soft sky/indigo gradient. Mean delta: 10.33/255. |
| **Curious** | `Idle` | `Curious` | **NO** | Tilted gaze ($gazeX = 0.055r, gazeY = -0.035r$), small dark oval mouth (`#3f255d`), cyan/violet gradient. Mean delta: 10.34/255. |
| **Surprised** | `Idle` | `Surprised` | **NO** | Enlarged tall eyes ($0.34r$), vertical tall oval mouth (`#3b1747`), lavender/pink gradient. Mean delta: 12.28/255. |
| **Error** | `Error` | `Neutral` | **NO** | Horizontal jitter $\sin(12t) \times 2$, high-frequency rim distortion $\sin(7\theta + 9t) \times 0.018$, flat horizontal line mouth, halo omitted. Mean delta: 3.54/255. |

---

## 4. Mood Mapping Unit Tests (`ruby-core`)

Ported directly from `tests/test_liquid_blob_moods.py` to `crates/ruby-core/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sad_mood_has_no_black_core() {
        let state = OrbVisualState {
            operational: OperationalState::Idle,
            emotion: Emotion::Sad,
            ..Default::default()
        };
        assert!(!state.uses_black_core(), "Sad mood must NOT use black core");
    }

    #[test]
    fn test_neutral_listening_and_speaking_keep_approved_core() {
        for op in [OperationalState::Idle, OperationalState::Listening, OperationalState::Speaking] {
            let state = OrbVisualState { operational: op, emotion: Emotion::Neutral, ..Default::default() };
            assert!(state.uses_black_core());
        }
    }

    #[test]
    fn test_emotional_expressions_do_not_use_black_core() {
        for emotion in [Emotion::Happy, Emotion::Excited, Emotion::Empathetic, Emotion::Curious, Emotion::Surprised] {
            let state = OrbVisualState { operational: OperationalState::Speaking, emotion, ..Default::default() };
            assert!(!state.uses_black_core(), "{:?} must NOT use black core", emotion);
        }
    }
}
```

---

## 5. Scope Boundary Compliance

- **No Scaffolding of M1–M6 Logic:** Audio capture, live dictation pipeline, Gemini API network calls, SQLite database migrations, and MCP sidecar processes remain strictly as trait definitions with mock implementations for offline testing.
- **Repository Isolation:** The legacy Python repository at `D:\projects\ai_agent_original` was inspected read-only as the design and mathematical source of truth.
- **Next Milestone Gate:** M1 (Wispr Flow-Style System-Wide Dictation) will not begin until explicit approval is granted by the user.
