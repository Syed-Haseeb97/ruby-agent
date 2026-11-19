import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DoctorReport } from "./DoctorModal";
import { AppConfigPayload } from "./SettingsPanel";

interface SecretsStatus {
  gemini_api_key_set: boolean;
  elevenlabs_api_key_set: boolean;
  telegram_bot_token_set: boolean;
}

interface SetupWizardProps {
  isOpen: boolean;
  onComplete: () => void;
}

export const SetupWizard: React.FC<SetupWizardProps> = ({ isOpen, onComplete }) => {
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);
  const [allowedRoots, setAllowedRoots] = useState("D:\\projects");
  const [rootWarning, setRootWarning] = useState<string | null>(null);
  const [secretsStatus, setSecretsStatus] = useState<SecretsStatus>({
    gemini_api_key_set: false,
    elevenlabs_api_key_set: false,
    telegram_bot_token_set: false,
  });
  const [doctorReport, setDoctorReport] = useState<DoctorReport | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (isOpen) {
      checkSecrets();
    }
  }, [isOpen]);

  const checkSecrets = async () => {
    try {
      const status = await invoke<SecretsStatus>("get_secrets_status");
      setSecretsStatus(status);
    } catch (e) {
      console.error("Secrets status check failed", e);
    }
  };

  const handleRootsChange = (val: string) => {
    setAllowedRoots(val);
    const roots = val.split(",").map((s) => s.trim()).filter(Boolean);
    let warn: string | null = null;
    for (const r of roots) {
      const trimmed = r.toUpperCase();
      if (
        trimmed === "C:\\" ||
        trimmed === "C:/" ||
        trimmed === "D:\\" ||
        trimmed === "D:/" ||
        trimmed === "/" ||
        trimmed === "\\" ||
        (trimmed.length <= 3 && trimmed.endsWith(":"))
      ) {
        warn = `Root '${r}' encompasses an entire drive. Specifying a subfolder like 'D:\\projects' is strongly recommended.`;
        break;
      }
    }
    setRootWarning(warn);
  };

  const runStep4Doctor = async () => {
    try {
      const res = await invoke<DoctorReport>("run_doctor", { optInNetwork: false });
      setDoctorReport(res);
    } catch (e) {
      console.error("Doctor error in wizard", e);
    }
  };

  const handleFinish = async () => {
    setSaving(true);
    try {
      const roots = allowedRoots.split(",").map((s) => s.trim()).filter(Boolean);
      const config: AppConfigPayload = {
        allowed_roots: roots.length > 0 ? roots : ["D:\\projects"],
        computer_use_enabled: false,
        computer_use_dry_run: true,
        advanced_shell_enabled: false,
        allow_screenshots: false,
        recording_enabled: false,
        voice_enabled: false,
        voice_model: "gemini-2.5-flash",
        voice_session_limit_minutes: 10,
      };
      await invoke("save_app_config", { config, confirmed: false });
      onComplete();
    } catch (e) {
      console.error("Failed to complete onboarding wizard", e);
    } finally {
      setSaving(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="First-Run Setup Wizard"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4 font-sans text-[#16181D]"
    >
      <div className="relative w-full max-w-2xl bg-[#FFFFFF] border border-[#C8C2B5] rounded-2xl shadow-xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Wizard Header */}
        <div className="bg-[#F5F3EE] px-6 py-5 border-b border-[#C8C2B5] flex items-center justify-between">
          <div>
            <span className="inline-flex items-center gap-2 rounded-full border border-[#C8C2B5] bg-[#FFFFFF] px-3.5 py-1 font-mono text-[11px] uppercase tracking-wider text-[#1D6B52]">
              First-Run Setup • Step {step} of 4
            </span>
            <h2 className="font-serif text-2xl font-bold mt-2 text-[#16181D]">
              {step === 1 && "Welcome to Ruby"}
              {step === 2 && "Configure Allowed Roots"}
              {step === 3 && "Credentials & API Keys"}
              {step === 4 && "System Health Verification"}
            </h2>
          </div>
          <div className="flex gap-1.5">
            {[1, 2, 3, 4].map((s) => (
              <div
                key={s}
                className={`w-2.5 h-2.5 rounded-full ${
                  s === step
                    ? "bg-[#16181D]"
                    : s < step
                    ? "bg-[#1D6B52]"
                    : "bg-[#C8C2B5]"
                }`}
              />
            ))}
          </div>
        </div>

        {/* Wizard Body */}
        <div className="p-6 overflow-y-auto space-y-5 flex-1 bg-[#FFFFFF]">
          {/* Step 1: Welcome & Philosophy */}
          {step === 1 && (
            <div className="space-y-4">
              <p className="text-sm text-[#16181D] leading-relaxed">
                Ruby is a privacy-first, desktop assistant designed with strict safety boundaries:
              </p>
              <div className="grid grid-cols-1 gap-3">
                <div className="border border-[#C8C2B5] rounded-xl p-3.5 bg-[#F5F3EE]">
                  <strong className="block font-mono text-xs uppercase text-[#16181D] mb-1">
                    Non-Autonomous by Default
                  </strong>
                  <p className="text-xs text-[#5F626A]">
                    Dangerous features (OS computer use, live shell commands, screen recording) default to OFF. All consequential actions require explicit 6-digit user confirmation.
                  </p>
                </div>
                <div className="border border-[#C8C2B5] rounded-xl p-3.5 bg-[#F5F3EE]">
                  <strong className="block font-mono text-xs uppercase text-[#16181D] mb-1">
                    Isolated Storage
                  </strong>
                  <p className="text-xs text-[#5F626A]">
                    All memories, logs, recordings, and job states are kept strictly inside <code className="font-mono text-[11px] bg-[#FFFFFF] px-1 rounded border border-[#C8C2B5]">%LOCALAPPDATA%\Ruby\</code>, completely outside code repositories.
                  </p>
                </div>
                <div className="border border-[#C8C2B5] rounded-xl p-3.5 bg-[#F5F3EE]">
                  <strong className="block font-mono text-xs uppercase text-[#16181D] mb-1">
                    Hardware Kill Switch
                  </strong>
                  <p className="text-xs text-[#5F626A]">
                    Holding the Escape key for 1.2 seconds instantly halts all active computer use, live audio, and background tasks while keeping the console alive.
                  </p>
                </div>
              </div>
            </div>
          )}

          {/* Step 2: Allowed Roots */}
          {step === 2 && (
            <div className="space-y-4">
              <p className="text-xs text-[#5F626A] leading-relaxed">
                Choose the directory trees Ruby is authorized to read and write. Filesystem operations outside these boundaries are denied by security policy.
              </p>
              <div className="border border-[#C8C2B5] rounded-xl p-4 bg-[#F5F3EE]">
                <label
                  htmlFor="wizard-roots"
                  className="block font-mono text-xs uppercase tracking-wider font-semibold text-[#16181D] mb-1"
                >
                  Project Roots
                </label>
                <input
                  id="wizard-roots"
                  type="text"
                  value={allowedRoots}
                  onChange={(e) => handleRootsChange(e.target.value)}
                  className="w-full bg-[#FFFFFF] border border-[#C8C2B5] rounded-lg px-3 py-2 text-sm font-mono text-[#16181D] focus:outline-none focus:border-[#1D6B52]"
                  placeholder="D:\projects"
                />
                {rootWarning && (
                  <div
                    role="alert"
                    className="mt-3 rounded-lg border border-[#86560D] bg-[#F6ECD9] p-3 text-xs text-[#86560D]"
                  >
                    <strong>Warning:</strong> {rootWarning}
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Step 3: Secrets Setup */}
          {step === 3 && (
            <div className="space-y-4">
              <p className="text-xs text-[#5F626A]">
                Ruby never stores plaintext secrets in repository files or config. Keys are resolved from Windows Credential Manager or system environment variables.
              </p>
              <div className="space-y-3">
                <div className="border border-[#C8C2B5] rounded-xl p-3.5 bg-[#F5F3EE] flex items-center justify-between">
                  <div>
                    <span className="font-mono text-xs font-bold text-[#16181D]">
                      GEMINI_API_KEY
                    </span>
                    <p className="text-xs text-[#5F626A]">
                      Powers dictation, multimodal vision, and chat inference.
                    </p>
                  </div>
                  <span
                    className={`px-3 py-1 rounded-full font-mono text-[10px] font-bold ${
                      secretsStatus.gemini_api_key_set
                        ? "bg-[#E3EFE8] text-[#1D6B52] border border-[#1D6B52]"
                        : "bg-[#F6ECD9] text-[#86560D] border border-[#86560D]"
                    }`}
                  >
                    {secretsStatus.gemini_api_key_set ? "CONFIGURED" : "MISSING"}
                  </span>
                </div>

                <div className="border border-[#C8C2B5] rounded-xl p-3.5 bg-[#F5F3EE] flex items-center justify-between">
                  <div>
                    <span className="font-mono text-xs font-bold text-[#16181D]">
                      ELEVENLABS_API_KEY
                    </span>
                    <p className="text-xs text-[#5F626A]">
                      Optional neural cloud speech synthesis (falls back to local SAPI).
                    </p>
                  </div>
                  <span
                    className={`px-3 py-1 rounded-full font-mono text-[10px] font-bold ${
                      secretsStatus.elevenlabs_api_key_set
                        ? "bg-[#E3EFE8] text-[#1D6B52] border border-[#1D6B52]"
                        : "bg-[#F6ECD9] text-[#86560D] border border-[#86560D]"
                    }`}
                  >
                    {secretsStatus.elevenlabs_api_key_set ? "CONFIGURED" : "NOT SET"}
                  </span>
                </div>

                <div className="border border-[#C8C2B5] rounded-xl p-3.5 bg-[#F5F3EE] flex items-center justify-between">
                  <div>
                    <span className="font-mono text-xs font-bold text-[#16181D]">
                      TELEGRAM_BOT_TOKEN
                    </span>
                    <p className="text-xs text-[#5F626A]">
                      Optional remote control over Telegram.
                    </p>
                  </div>
                  <span
                    className={`px-3 py-1 rounded-full font-mono text-[10px] font-bold ${
                      secretsStatus.telegram_bot_token_set
                        ? "bg-[#E3EFE8] text-[#1D6B52] border border-[#1D6B52]"
                        : "bg-[#F6ECD9] text-[#86560D] border border-[#86560D]"
                    }`}
                  >
                    {secretsStatus.telegram_bot_token_set ? "CONFIGURED" : "NOT SET"}
                  </span>
                </div>
              </div>
              <button
                onClick={checkSecrets}
                className="text-xs font-mono font-semibold underline text-[#1D6B52]"
              >
                Refresh Key Status
              </button>
            </div>
          )}

          {/* Step 4: Completion & Doctor Verification */}
          {step === 4 && (
            <div className="space-y-4">
              <p className="text-xs text-[#5F626A]">
                Running the Ruby Doctor diagnostic checklist to verify security gating, DPI awareness, and data directory isolation before final launch:
              </p>
              {doctorReport ? (
                <div className="space-y-2 max-h-56 overflow-y-auto pr-1">
                  {doctorReport.items.map((it, idx) => (
                    <div
                      key={idx}
                      className="border border-[#C8C2B5] rounded-lg p-2.5 bg-[#F5F3EE] flex items-center justify-between text-xs"
                    >
                      <span className="font-mono text-xs font-semibold text-[#16181D]">
                        {it.name}
                      </span>
                      <span
                        className={`px-2 py-0.5 rounded-full font-mono text-[9px] font-bold ${
                          it.status === "pass"
                            ? "bg-[#E3EFE8] text-[#1D6B52] border border-[#1D6B52]"
                            : it.status === "warn"
                            ? "bg-[#F6ECD9] text-[#86560D] border border-[#86560D]"
                            : "bg-[#F8E9E4] text-[#B53A26] border border-[#B53A26]"
                        }`}
                      >
                        {it.status.toUpperCase()}
                      </span>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="text-center py-6">
                  <button
                    onClick={runStep4Doctor}
                    className="rounded-full bg-[#16181D] text-[#F5F3EE] px-5 py-2 text-xs font-medium"
                  >
                    Run Doctor Pre-flight Check
                  </button>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Wizard Footer */}
        <div className="bg-[#F5F3EE] px-6 py-4 border-t border-[#C8C2B5] flex justify-between">
          {step > 1 ? (
            <button
              onClick={() => setStep((s) => (s - 1) as any)}
              className="rounded-full border border-[#C8C2B5] bg-[#FFFFFF] px-5 py-2 text-xs font-medium text-[#16181D]"
            >
              Back
            </button>
          ) : (
            <div />
          )}

          {step < 4 ? (
            <button
              onClick={() => {
                if (step === 3) {
                  runStep4Doctor();
                }
                setStep((s) => (s + 1) as any);
              }}
              className="rounded-full bg-[#16181D] text-[#F5F3EE] px-6 py-2 text-xs font-medium hover:bg-black transition-colors"
            >
              Next Step
            </button>
          ) : (
            <button
              onClick={handleFinish}
              disabled={saving}
              className="rounded-full bg-[#1D6B52] text-[#FFFFFF] px-6 py-2 text-xs font-medium hover:bg-[#15533f] transition-colors"
            >
              {saving ? "Saving..." : "Complete Setup & Launch"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
