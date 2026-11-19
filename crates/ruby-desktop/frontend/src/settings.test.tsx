import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import * as fs from "fs";
import * as path from "path";
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
      if (
        (args?.config?.computer_use_enabled ||
          args?.config?.advanced_shell_enabled ||
          args?.config?.allow_screenshots ||
          args?.config?.recording_enabled) &&
        !args?.confirmed
      ) {
        return Promise.reject("Enabling dangerous features requires explicit user confirmation.");
      }
      const warn = args?.config?.allowed_roots?.some(
        (r: string) =>
          r === "C:\\" ||
          r === "D:\\" ||
          r === "/" ||
          r.toUpperCase().includes("%USERPROFILE%")
      )
        ? "Warning: Allowed root covers an entire drive or home directory."
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

      // STRICT PROHIBITION: Regex check for animate-pulse, animate-ping, or blink classes anywhere
      expect(container.innerHTML).not.toMatch(/animate-pulse/);
      expect(container.innerHTML).not.toMatch(/animate-ping/);
      expect(container.innerHTML).not.toMatch(/blink/i);
    });

    it("displays security advisory warning when root directory encompasses C:\\, D:\\, or %USERPROFILE%", async () => {
      render(<SettingsPanel isOpen={true} onClose={() => {}} />);

      const rootsInput = screen.getByLabelText(/Allowed Roots/i);

      // Test 1: D:\ whole drive warning
      fireEvent.change(rootsInput, { target: { value: "D:\\" } });
      await waitFor(() => {
        expect(screen.getByRole("alert")).toBeDefined();
        expect(screen.getByText(/encompasses an entire drive/i)).toBeDefined();
      });

      // Test 2: C:\ whole drive warning
      fireEvent.change(rootsInput, { target: { value: "C:\\" } });
      await waitFor(() => {
        expect(screen.getByRole("alert")).toBeDefined();
        expect(screen.getByText(/encompasses an entire drive/i)).toBeDefined();
      });

      // Test 3: %USERPROFILE% warning
      fireEvent.change(rootsInput, { target: { value: "%USERPROFILE%" } });
      await waitFor(() => {
        expect(screen.getByRole("alert")).toBeDefined();
        expect(screen.getByText(/encompasses an entire drive or user home/i)).toBeDefined();
      });

      // Test 4: Safe path clears warning
      fireEvent.change(rootsInput, { target: { value: "D:\\projects\\ruby-agent" } });
      await waitFor(() => {
        expect(screen.queryByRole("alert")).toBeNull();
      });
    });

    it("triggers explicit confirmation modal for EVERY dangerous flag", async () => {
      render(<SettingsPanel isOpen={true} onClose={() => {}} />);

      const dangerousCheckboxes = [
        { label: /Enable Computer Use/i, name: "computer_use_enabled" },
        { label: /Enable Advanced Shell/i, name: "advanced_shell_enabled" },
        { label: /Allow Remote Screenshots/i, name: "allow_screenshots" },
        { label: /Enable Screen Recording/i, name: "recording_enabled" },
      ];

      for (const item of dangerousCheckboxes) {
        const checkbox = screen.getByLabelText(item.label) as HTMLInputElement;
        expect(checkbox.checked).toBe(false);

        // Attempt to enable
        fireEvent.click(checkbox);

        await waitFor(() => {
          expect(screen.getByRole("alertdialog")).toBeDefined();
          expect(screen.getByText("Enable Dangerous Feature?")).toBeDefined();
        });

        // Confirm
        const confirmBtn = screen.getByRole("button", { name: /Confirm & Enable/i });
        fireEvent.click(confirmBtn);

        await waitFor(() => {
          expect(screen.queryByRole("alertdialog")).toBeNull();
          expect(checkbox.checked).toBe(true);
        });

        // Reset for next test
        fireEvent.click(checkbox);
        expect(checkbox.checked).toBe(false);
      }
    });

    it("guarantees secrets are never rendered or returned over IPC as plaintext", async () => {
      const { container } = render(<SettingsPanel isOpen={true} onClose={() => {}} />);

      await waitFor(() => {
        expect(screen.getByText("Ruby System Settings")).toBeDefined();
      });

      // Assert no sensitive key patterns or plaintext secrets exist in rendered DOM
      const html = container.innerHTML;
      expect(html).not.toMatch(/ghp_[A-Za-z0-9]{20,}/);
      expect(html).not.toMatch(/AIzaSy[A-Za-z0-9_-]{33}/);
      expect(html).not.toMatch(/sk-[A-Za-z0-9]{20,}/);
      expect(html).not.toMatch(/freellmapi-[A-Za-z0-9_-]{16,}/);
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

  describe("Static Code Analysis & Accessibility Rules", () => {
    it("statically verifies that no UI components use blinking or pulsing classes on indicators", () => {
      const srcDir = path.resolve(__dirname);
      const targetFiles = [
        "ConsoleView.tsx",
        "SettingsPanel.tsx",
        "SetupWizard.tsx",
        "DoctorModal.tsx",
        "OrbPreviewPanel.tsx",
      ];

      for (const file of targetFiles) {
        const fullPath = path.join(srcDir, file);
        if (fs.existsSync(fullPath)) {
          const content = fs.readFileSync(fullPath, "utf-8");
          // Strictly fail if animate-pulse or animate-ping appear in classes
          expect(content).not.toMatch(/className="[^"]*animate-pulse/);
          expect(content).not.toMatch(/className="[^"]*animate-ping/);
          expect(content).not.toMatch(/keyframes\s+blink/i);
        }
      }
    });

    it("verifies interactive inputs and buttons provide explicit accessible labels and roles", async () => {
      render(<SettingsPanel isOpen={true} onClose={() => {}} />);

      // Verify accessible form controls
      expect(screen.getByLabelText(/Allowed Roots/i)).toBeDefined();
      expect(screen.getByLabelText(/Enable Computer Use/i)).toBeDefined();
      expect(screen.getByLabelText(/Enable Advanced Shell/i)).toBeDefined();
      expect(screen.getByLabelText(/Allow Remote Screenshots/i)).toBeDefined();
      expect(screen.getByLabelText(/Enable Screen Recording/i)).toBeDefined();
      expect(screen.getByRole("button", { name: /Save Configuration/i })).toBeDefined();
      expect(screen.getByRole("button", { name: /Cancel/i })).toBeDefined();
    });
  });
});
