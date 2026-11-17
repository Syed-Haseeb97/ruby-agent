# Milestone M2 Manual Verification Guide

**Tester**: Syed Haseeb Bukhari  
**Target Platform**: Windows 11 Desktop  
**Binary Location**: `D:\projects\ruby-agent`  

---

## 1. Test Setup

1. Ensure the Gemini API key is configured:
   ```powershell
   $env:GEMINI_API_KEY = "<YOUR_GEMINI_KEY>"
   ```
2. Launch the desktop assistant:
   ```powershell
   cd D:\projects\ruby-agent
   cargo run -p ruby-desktop
   ```
3. Observe the two windows:
   - Floating Liquid Orb (transparent, frameless, non-activating, top-right).
   - Editorial Control Console (`#F5F3EE` warm paper background, `#C8C2B5` stone borders).

---

## 2. Test Cases to Perform on Host Machine

### Test Case 1: Audible Speech Output
- **Action**: In the Console input box, type: `"Hello Ruby, introduce yourself in two sentences."` and press Enter.
- **Expected Result**:
  1. The Orb turns into a rotating violet faceless sphere (`Thinking`).
  2. The assistant reply appears in the stream with an emotion badge (`NEUTRAL` or `HAPPY`).
  3. Audio is audible from your Windows speakers/headphones via `Edge-TTS` (Aria Neural voice).
  4. The Orb displays the speaking mouth animation during speech.
  5. The Orb returns to `Idle` / `Neutral` when speech concludes.

### Test Case 2: Visible Emotion Expressions
- **Action A (Empathetic)**: Type `"I had a really rough and exhausting day today."`
  - **Expected Result**: The emotion badge displays `EMPATHETIC`. The Orb displays calm, gentle downturned eyes without the harsh dark core, demonstrating distress mapping.
- **Action B (Excited)**: Type `"I just won first prize at the national hackathon!"`
  - **Expected Result**: The emotion badge displays `EXCITED`. The Orb displays joyful curved eyes and active animated open mouth.
- **Action C (Curious)**: Type `"How do quantum computers factor large primes?"`
  - **Expected Result**: The emotion badge displays `CURIOUS`. The Orb exhibits tilted gaze and inquisitive expression.

### Test Case 3: Speech Cancellation via Esc
- **Action**: Ask a longer question: `"Explain the history of the Apollo space program."`
- **Action**: While Ruby is speaking, press the **`Escape` (Esc)** key (or click the "Stop Audio" button in the Console).
- **Expected Result**:
  - The voice cuts off instantly.
  - The Orb stops speaking and immediately resets to `Idle` / `Neutral`.
  - Console shows `"Speech cancelled"` notice.

### Test Case 4: Edge-TTS Network Block Fallback
- **Action**: Disconnect internet or block Edge-TTS domain, or select `Windows-Speech` in the Console Voice Synthesis dropdown.
- **Expected Result**:
  - Synthesis automatically falls back to native Windows Speech (SAPI / WinRT).
  - Speech remains audible through the local Windows TTS synthesizer with zero UI freeze.

### Test Case 5: Zero Blinking UI Verification
- **Action**: Inspect all status capsules, badges, and counters in the Console.
- **Expected Result**:
  - All status capsules are completely static.
  - Zero pulsing dots (`animate-pulse`), zero flashing indicators, perfectly calm editorial aesthetic.
