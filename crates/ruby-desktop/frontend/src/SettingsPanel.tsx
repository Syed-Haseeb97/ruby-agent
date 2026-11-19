import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface AppConfigPayload {
  allowed_roots: string[];
  computer_use_enabled: boolean;
  computer_use_dry_run: boolean;
  advanced_shell_enabled: boolean;
  allow_screenshots: boolean;
  recording_enabled: boolean;
  voice_enabled: boolean;
  voice_model: string;
  voice_session_limit_minutes: number;
}

interface SaveConfigResult {
  success: boolean;
  warning?: string;
}

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

export const SettingsPanel: React.FC<SettingsPanelProps> = ({ isOpen, onClose }) => {
  const [config, setConfig] = useState<AppConfigPayload>({
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

  const [rootsInput, setRootsInput] = useState("D:\\projects");
  const [rootWarning, setRootWarning] = useState<string | null>(null);
  const [showConfirmModal, setShowConfirmModal] = useState(false);
  const [pendingFlag, setPendingFlag] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      loadConfig();
    }
  }, [isOpen]);

  const loadConfig = async () => {
    try {
      const loaded = await invoke<AppConfigPayload>("get_app_config");
      setConfig(loaded);
      setRootsInput(loaded.allowed_roots.join(", "));
      validateRoots(loaded.allowed_roots);
    } catch (e: any) {
      setErrorMessage(`Failed to load config: ${e}`);
    }
  };

  const validateRoots = (roots: string[]) => {
    let warn: string | null = null;
    for (const r of roots) {
      const trimmed = r.trim().toUpperCase();
      if (
        trimmed === "C:\\" ||
        trimmed === "C:/" ||
        trimmed === "D:\\" ||
        trimmed === "D:/" ||
        trimmed === "/" ||
        trimmed === "\\" ||
        trimmed === "%USERPROFILE%" ||
        trimmed === "%USERPROFILE%\\" ||
        (trimmed.length <= 3 && trimmed.endsWith(":"))
      ) {
        warn = `Root '${r.trim()}' encompasses an entire drive or user home directory. Specifying a subfolder like 'D:\\projects' is strongly recommended.`;
        break;
      }
    }
    setRootWarning(warn);
  };

  const handleRootsChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setRootsInput(val);
    const split = val.split(",").map((s) => s.trim()).filter(Boolean);
    validateRoots(split);
    setConfig((prev) => ({ ...prev, allowed_roots: split }));
  };

  const handleToggleDangerous = (key: keyof AppConfigPayload, val: boolean) => {
    if (val) {
      setPendingFlag(key);
      setShowConfirmModal(true);
    } else {
      setConfig((prev) => ({ ...prev, [key]: false }));
    }
  };

  const handleConfirmDangerous = () => {
    if (pendingFlag) {
      setConfig((prev) => ({ ...prev, [pendingFlag]: true }));
    }
    setShowConfirmModal(false);
    setPendingFlag(null);
  };

  const handleSave = async (confirmedDangerous = false) => {
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      const res = await invoke<SaveConfigResult>("save_app_config", {
        config,
        confirmed: confirmedDangerous,
      });
      if (res.warning) {
        setRootWarning(res.warning);
      }
      setStatusMessage("Settings successfully saved to local config.");
      setTimeout(() => setStatusMessage(null), 3000);
    } catch (e: any) {
      setErrorMessage(typeof e === "string" ? e : JSON.stringify(e));
    }
  };

  if (!isOpen) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Application Settings"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4 font-sans text-[#16181D]"
    >
      <div className="relative w-full max-w-2xl bg-[#FFFFFF] border border-[#C8C2B5] rounded-2xl shadow-xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="bg-[#F5F3EE] px-6 py-5 border-b border-[#C8C2B5] flex items-center justify-between">
          <div>
            <span className="inline-flex items-center gap-2 rounded-full border border-[#C8C2B5] bg-[#FFFFFF] px-3.5 py-1 font-mono text-[11px] uppercase tracking-wider text-[#1D6B52]">
              Config & Policy
            </span>
            <h2 className="font-serif text-2xl font-bold mt-2 text-[#16181D]">
              Ruby System Settings
            </h2>
          </div>
          <button
            onClick={onClose}
            aria-label="Close Settings"
            className="rounded-full border border-[#C8C2B5] px-3 py-1 font-mono text-xs hover:bg-[#E3EFE8] transition-colors"
          >
            ESC / Close
          </button>
        </div>

        {/* Content Body */}
        <div className="p-6 overflow-y-auto space-y-6 flex-1 bg-[#FFFFFF]">
          {/* Allowed Roots */}
          <div className="border border-[#C8C2B5] rounded-xl p-4 bg-[#F5F3EE]">
            <label
              htmlFor="allowed-roots-input"
              className="block font-mono text-xs uppercase tracking-wider font-semibold text-[#16181D] mb-1"
            >
              Allowed Roots (Comma separated paths)
            </label>
            <p className="text-xs text-[#5F626A] mb-3">
              Mode B and MCP file operations are strictly jailed to these filesystem paths.
            </p>
            <input
              id="allowed-roots-input"
              type="text"
              value={rootsInput}
              onChange={handleRootsChange}
              className="w-full bg-[#FFFFFF] border border-[#C8C2B5] rounded-lg px-3 py-2 text-sm font-mono text-[#16181D] focus:outline-none focus:border-[#1D6B52]"
              placeholder="D:\projects, C:\Users\Name\projects"
            />
            {rootWarning && (
              <div
                role="alert"
                className="mt-3 rounded-lg border border-[#86560D] bg-[#F6ECD9] p-3 text-xs text-[#86560D]"
              >
                <strong>Security Advisory:</strong> {rootWarning}
              </div>
            )}
          </div>

          {/* Dangerous Security Flags */}
          <div className="border border-[#C8C2B5] rounded-xl p-4 space-y-4">
            <h3 className="font-mono text-xs uppercase tracking-wider font-semibold text-[#16181D]">
              Safety Gating & Dangerous Features (Default OFF)
            </h3>

            {/* Computer Use */}
            <div className="flex items-center justify-between py-2 border-b border-[#C8C2B5]/50">
              <div>
                <span className="text-sm font-semibold text-[#16181D]">
                  Mode A Computer Use Operator
                </span>
                <p className="text-xs text-[#5F626A]">
                  Enables OS UI automation and synthetic mouse/keyboard actions.
                </p>
              </div>
              <input
                type="checkbox"
                aria-label="Enable Computer Use"
                checked={config.computer_use_enabled}
                onChange={(e) => handleToggleDangerous("computer_use_enabled", e.target.checked)}
                className="w-4 h-4 accent-[#16181D]"
              />
            </div>

            {/* Advanced Shell */}
            <div className="flex items-center justify-between py-2 border-b border-[#C8C2B5]/50">
              <div>
                <span className="text-sm font-semibold text-[#16181D]">
                  Advanced Shell Execution
                </span>
                <p className="text-xs text-[#5F626A]">
                  Enables local command runner backend in background tasks.
                </p>
              </div>
              <input
                type="checkbox"
                aria-label="Enable Advanced Shell"
                checked={config.advanced_shell_enabled}
                onChange={(e) => handleToggleDangerous("advanced_shell_enabled", e.target.checked)}
                className="w-4 h-4 accent-[#16181D]"
              />
            </div>

            {/* Remote Screenshots */}
            <div className="flex items-center justify-between py-2 border-b border-[#C8C2B5]/50">
              <div>
                <span className="text-sm font-semibold text-[#16181D]">
                  Remote Telegram Screenshots
                </span>
                <p className="text-xs text-[#5F626A]">
                  Allows sending full desktop screen captures over Telegram bot chat.
                </p>
              </div>
              <input
                type="checkbox"
                aria-label="Allow Remote Screenshots"
                checked={config.allow_screenshots}
                onChange={(e) => handleToggleDangerous("allow_screenshots", e.target.checked)}
                className="w-4 h-4 accent-[#16181D]"
              />
            </div>

            {/* Screen Recording */}
            <div className="flex items-center justify-between py-2">
              <div>
                <span className="text-sm font-semibold text-[#16181D]">
                  Screen Recording (FFmpeg)
                </span>
                <p className="text-xs text-[#5F626A]">
                  Captures video segments with automatic denylist protection.
                </p>
              </div>
              <input
                type="checkbox"
                aria-label="Enable Screen Recording"
                checked={config.recording_enabled}
                onChange={(e) => handleToggleDangerous("recording_enabled", e.target.checked)}
                className="w-4 h-4 accent-[#16181D]"
              />
            </div>
          </div>

          {/* Voice Live Settings */}
          <div className="border border-[#C8C2B5] rounded-xl p-4 bg-[#F5F3EE] space-y-3">
            <h3 className="font-mono text-xs uppercase tracking-wider font-semibold text-[#16181D]">
              Voice Live Configuration
            </h3>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <label
                  htmlFor="voice-model-select"
                  className="block font-mono text-[10px] uppercase text-[#5F626A] mb-1"
                >
                  Live Model
                </label>
                <select
                  id="voice-model-select"
                  value={config.voice_model}
                  onChange={(e) => setConfig((p) => ({ ...p, voice_model: e.target.value }))}
                  className="w-full bg-[#FFFFFF] border border-[#C8C2B5] rounded-lg px-2 py-1.5 text-xs font-mono"
                >
                  <option value="gemini-2.5-flash">gemini-2.5-flash (Standard)</option>
                  <option value="gemini-2.0-flash-exp">gemini-2.0-flash-exp (Experimental)</option>
                </select>
              </div>
              <div>
                <label
                  htmlFor="voice-session-limit"
                  className="block font-mono text-[10px] uppercase text-[#5F626A] mb-1"
                >
                  Session Cap (Minutes)
                </label>
                <input
                  id="voice-session-limit"
                  type="number"
                  min="1"
                  max="60"
                  value={config.voice_session_limit_minutes}
                  onChange={(e) =>
                    setConfig((p) => ({
                      ...p,
                      voice_session_limit_minutes: parseInt(e.target.value) || 10,
                    }))
                  }
                  className="w-full bg-[#FFFFFF] border border-[#C8C2B5] rounded-lg px-2 py-1.5 text-xs font-mono"
                />
              </div>
            </div>
          </div>

          {/* Notifications */}
          {statusMessage && (
            <div
              role="status"
              className="p-3 rounded-lg border border-[#1D6B52] bg-[#E3EFE8] text-xs font-mono text-[#1D6B52]"
            >
              {statusMessage}
            </div>
          )}
          {errorMessage && (
            <div
              role="alert"
              className="p-3 rounded-lg border border-[#B53A26] bg-[#F8E9E4] text-xs font-mono text-[#B53A26]"
            >
              {errorMessage}
            </div>
          )}
        </div>

        {/* Footer Actions */}
        <div className="bg-[#F5F3EE] px-6 py-4 border-t border-[#C8C2B5] flex justify-end gap-3">
          <button
            onClick={onClose}
            className="rounded-full border border-[#C8C2B5] bg-[#FFFFFF] px-5 py-2 text-xs font-medium text-[#16181D] hover:bg-stone-50"
          >
            Cancel
          </button>
          <button
            onClick={() => handleSave(false)}
            className="rounded-full bg-[#16181D] text-[#F5F3EE] px-6 py-2 text-xs font-medium hover:bg-black transition-colors"
          >
            Save Configuration
          </button>
        </div>
      </div>

      {/* Explicit Confirmation Dialog for Dangerous Flags */}
      {showConfirmModal && (
        <div
          role="alertdialog"
          aria-labelledby="confirm-dangerous-title"
          className="fixed inset-0 z-60 flex items-center justify-center bg-black/50 p-4"
        >
          <div className="bg-[#FFFFFF] border border-[#B53A26] rounded-2xl p-6 max-w-md w-full shadow-2xl">
            <span className="inline-flex items-center gap-2 rounded-full border border-[#B53A26] bg-[#F8E9E4] px-3.5 py-1 font-mono text-[11px] uppercase tracking-wider text-[#B53A26]">
              Safety Warning
            </span>
            <h4 id="confirm-dangerous-title" className="font-serif text-xl font-bold mt-2 text-[#16181D]">
              Enable Dangerous Feature?
            </h4>
            <p className="text-xs text-[#5F626A] mt-2 leading-relaxed">
              You are turning on <strong>{pendingFlag}</strong>. This grants Ruby autonomy or remote exposure.
              Per safety policy, all dangerous features should remain disabled unless strictly necessary.
            </p>
            <div className="mt-6 flex justify-end gap-3">
              <button
                onClick={() => {
                  setShowConfirmModal(false);
                  setPendingFlag(null);
                }}
                className="rounded-full border border-[#C8C2B5] px-4 py-1.5 text-xs font-mono"
              >
                Cancel
              </button>
              <button
                onClick={handleConfirmDangerous}
                className="rounded-full bg-[#B53A26] text-[#FFFFFF] px-5 py-1.5 text-xs font-mono font-bold"
              >
                Confirm & Enable
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
