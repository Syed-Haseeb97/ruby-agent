import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsPanel } from "./SettingsPanel";
import { SetupWizard } from "./SetupWizard";
import { DoctorModal } from "./DoctorModal";

// Mock Tauri invoke and listen APIs
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args: any) => {
    if (cmd === "get_app_config") {
      return Promise.resolve({
        allowed_roots: ["D:\\projects"],
        computer_use_enabled: false,
        computer_use_dry_run: true,
        advanced_shell_enabled: false,
        allow_screenshots: false,
        recording_enabled: false,
        voice_enabled: false,
        voice_model: "gemini-2.5-flash",
        voice_session_limit_minutes: 10,
      });
    }
    if (cmd === "save_app_config") {
      if (args?.config?.computer_use_enabled && !args?.confirmed) {
        return Promise.reject("Enabling dangerous features requires explicit user confirmation.");
      }
      const warn = args?.config?.allowed_roots?.some(
        (r: string) => r === "C:\\" || r === "D:\\" || r === "/"
      )
        ? "Warning: Allowed root covers an entire drive."
        : undefined;
      return Promise.resolve({ success: true, warning: warn });
    }
    if (cmd === "get_secrets_status") {
      return Promise.resolve({
        gemini_api_key_set: true,
        elevenlabs_api_key_set: false,
        telegram_bot_token_set: false,
      });
    }
    if (cmd === "run_doctor") {
      return Promise.resolve({
        overall_status: "pass",
        timestamp_rfc3339: "2026-11-19T12:00:00Z",
        items: [
          {
            category: "Credentials",
            name: "Gemini API Key",
            status: "pass",
            message: "Present in environment / Credential Manager",
            remedy: null,
          },
          {
            category: "Storage",
            name: "Data Directory Isolation",
            status: "pass",
            message: "Resolved workspace directory outside repo",
            remedy: null,
          },
          {
            category: "Security",
            name: "Computer Use Gating",
            status: "pass",
            message: "enabled=false, dry_run=true",
            remedy: null,
          },
          {
            category: "Display",
            name: "Per-Monitor DPI Awareness",
            status: "pass",
            message: "Per-Monitor V2 DPI awareness configured",
            remedy: null,
          },
        ],
      });
    }
    if (cmd === "is_first_run") {
      return Promise.resolve(false);
    }
    return Promise.resolve({});
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("Settings, Onboarding & Doctor Front-end Suite (U1–U4)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("SettingsPanel (U1)", () => {
    it("renders dialog with proper accessibility role and no blinking/pulsing classes", async () => {
      const { container } = render(<SettingsPanel isOpen={true} onClose={() => {}} />);

      expect(screen.getByRole("dialog", { name: /Application Settings/i })).toBeDefined();
      expect(screen.getByText("Ruby System Settings")).toBeDefined();

      // STRICT PROHIBITION: Regex check for animate-pulse or animate-ping anywhere in rendered HTML
      expect(container.innerHTML).not.toMatch(/animate-pulse/);
      expect(container.innerHTML).not.toMatch(/animate-ping/);
      expect(container.innerHTML).not.toMatch(/blink/i);
    });

    it("displays security advisory warning when root directory encompasses an entire drive", async () => {
      render(<SettingsPanel isOpen={true} onClose={() => {}} />);

      const rootsInput = screen.getByLabelText(/Allowed Roots/i);
      fireEvent.change(rootsInput, { target: { value: "D:\\" } });

      await waitFor(() => {
        expect(screen.getByRole("alert")).toBeDefined();
        expect(screen.getByText(/encompasses an entire drive/i)).toBeDefined();
      });
    });

    it("triggers explicit confirmation modal when toggling dangerous security flags", async () => {
      render(<SettingsPanel isOpen={true} onClose={() => {}} />);

      const cuCheckbox = screen.getByLabelText(/Enable Computer Use/i);
      expect((cuCheckbox as HTMLInputElement).checked).toBe(false);

      // Attempt to enable Computer Use
      fireEvent.click(cuCheckbox);

      await waitFor(() => {
        expect(screen.getByRole("alertdialog")).toBeDefined();
        expect(screen.getByText("Enable Dangerous Feature?")).toBeDefined();
        expect(screen.getByText(/safety policy/i)).toBeDefined();
      });

      // Confirm dangerous flag
      const confirmBtn = screen.getByRole("button", { name: /Confirm & Enable/i });
      fireEvent.click(confirmBtn);

      await waitFor(() => {
        expect(screen.queryByRole("alertdialog")).toBeNull();
        expect((cuCheckbox as HTMLInputElement).checked).toBe(true);
      });
    });
  });

  describe("SetupWizard (U2)", () => {
    it("guides through 4-step first-run wizard with accessibility and no blinking dots", async () => {
      const handleComplete = vi.fn();
      const { container } = render(<SetupWizard isOpen={true} onComplete={handleComplete} />);

      // Step 1: Welcome
      expect(screen.getByRole("dialog", { name: /First-Run Setup Wizard/i })).toBeDefined();
      expect(screen.getByText("Welcome to Ruby")).toBeDefined();
      expect(screen.getByText(/Non-Autonomous by Default/i)).toBeDefined();

      // Check for zero blinking dots
      expect(container.innerHTML).not.toMatch(/animate-pulse/);
      expect(container.innerHTML).not.toMatch(/animate-ping/);

      // Navigate to Step 2
      const nextBtn1 = screen.getByRole("button", { name: /Next Step/i });
      fireEvent.click(nextBtn1);

      await waitFor(() => {
        expect(screen.getByText("Configure Allowed Roots")).toBeDefined();
      });

      // Navigate to Step 3
      const nextBtn2 = screen.getByRole("button", { name: /Next Step/i });
      fireEvent.click(nextBtn2);

      await waitFor(() => {
        expect(screen.getByText("Credentials & API Keys")).toBeDefined();
        expect(screen.getByText("GEMINI_API_KEY")).toBeDefined();
      });

      // Navigate to Step 4 (triggers Doctor pre-flight check automatically)
      const nextBtn3 = screen.getByRole("button", { name: /Next Step/i });
      fireEvent.click(nextBtn3);

      await waitFor(() => {
        expect(screen.getByText("System Health Verification")).toBeDefined();
        expect(screen.getByText("Data Directory Isolation")).toBeDefined();
      });

      // Complete setup
      const finishBtn = screen.getByRole("button", { name: /Complete Setup & Launch/i });
      fireEvent.click(finishBtn);

      await waitFor(() => {
        expect(handleComplete).toHaveBeenCalledTimes(1);
      });
    });
  });

  describe("DoctorModal (U3)", () => {
    it("renders diagnostic checklist items with static status badges and no pulses", async () => {
      const { container } = render(<DoctorModal isOpen={true} onClose={() => {}} />);

      expect(screen.getByRole("dialog", { name: /Diagnostic Doctor/i })).toBeDefined();
      expect(screen.getByText("Ruby System Doctor")).toBeDefined();

      await waitFor(() => {
        expect(screen.getByText("SYSTEM: PASS")).toBeDefined();
        expect(screen.getByText("Per-Monitor DPI Awareness")).toBeDefined();
      });

      // Verify no blinking or pulsing animations
      expect(container.innerHTML).not.toMatch(/animate-pulse/);
      expect(container.innerHTML).not.toMatch(/animate-ping/);
      expect(container.innerHTML).not.toMatch(/blink/i);

      // Verify copy report button works
      const copyBtn = screen.getByRole("button", { name: /Copy Report/i });
      expect(copyBtn).toBeDefined();
    });
  });
});
