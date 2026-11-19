import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface DiagnosticItem {
  category: string;
  name: string;
  status: "pass" | "warn" | "fail";
  message: string;
  remedy: string | null;
}

export interface DoctorReport {
  overall_status: "pass" | "warn" | "fail";
  items: DiagnosticItem[];
  timestamp_rfc3339: string;
}

interface DoctorModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const DoctorModal: React.FC<DoctorModalProps> = ({ isOpen, onClose }) => {
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);
  const [optInNetwork, setOptInNetwork] = useState(false);

  useEffect(() => {
    if (isOpen) {
      runDiagnostics();
    }
  }, [isOpen, optInNetwork]);

  const runDiagnostics = async () => {
    setLoading(true);
    try {
      const res = await invoke<DoctorReport>("run_doctor", {
        optInNetwork,
      });
      setReport(res);
    } catch (e) {
      console.error("Doctor error", e);
    } finally {
      setLoading(false);
    }
  };

  const copyMarkdown = () => {
    if (!report) return;
    const lines = [
      `# Ruby Doctor Diagnostic Report (${report.timestamp_rfc3339})`,
      `**Overall Status**: ${report.overall_status.toUpperCase()}\n`,
      `| Category | Check | Status | Details | Remedy |`,
      `| :--- | :--- | :---: | :--- | :--- |`,
    ];
    for (const item of report.items) {
      lines.push(
        `| ${item.category} | ${item.name} | ${item.status.toUpperCase()} | ${item.message} | ${item.remedy || "-"} |`
      );
    }
    navigator.clipboard.writeText(lines.join("\n"));
    setCopied(true);
    setTimeout(() => setCopied(false), 2500);
  };

  if (!isOpen) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Diagnostic Doctor"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4 font-sans text-[#16181D]"
    >
      <div className="relative w-full max-w-3xl bg-[#FFFFFF] border border-[#C8C2B5] rounded-2xl shadow-xl overflow-hidden flex flex-col max-h-[90vh]">
        {/* Header */}
        <div className="bg-[#F5F3EE] px-6 py-5 border-b border-[#C8C2B5] flex items-center justify-between">
          <div>
            <div className="flex items-center gap-3">
              <span className="inline-flex items-center gap-2 rounded-full border border-[#C8C2B5] bg-[#FFFFFF] px-3.5 py-1 font-mono text-[11px] uppercase tracking-wider text-[#1D6B52]">
                Diagnostics & Health
              </span>
              {report && (
                <span
                  className={`inline-flex items-center px-3 py-0.5 rounded-full font-mono text-xs font-semibold ${
                    report.overall_status === "pass"
                      ? "bg-[#E3EFE8] text-[#1D6B52] border border-[#1D6B52]"
                      : report.overall_status === "warn"
                      ? "bg-[#F6ECD9] text-[#86560D] border border-[#86560D]"
                      : "bg-[#F8E9E4] text-[#B53A26] border border-[#B53A26]"
                  }`}
                >
                  SYSTEM: {report.overall_status.toUpperCase()}
                </span>
              )}
            </div>
            <h2 className="font-serif text-2xl font-bold mt-2 text-[#16181D]">
              Ruby System Doctor
            </h2>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={copyMarkdown}
              className="rounded-full border border-[#C8C2B5] bg-[#FFFFFF] px-3.5 py-1.5 font-mono text-xs hover:bg-[#E3EFE8] transition-colors"
            >
              {copied ? "Copied Markdown!" : "Copy Report"}
            </button>
            <button
              onClick={onClose}
              aria-label="Close Doctor"
              className="rounded-full border border-[#C8C2B5] px-3 py-1 font-mono text-xs hover:bg-stone-100"
            >
              ESC / Close
            </button>
          </div>
        </div>

        {/* Options Bar */}
        <div className="px-6 py-2.5 bg-[#F5F3EE]/60 border-b border-[#C8C2B5] flex items-center justify-between text-xs font-mono">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={optInNetwork}
              onChange={(e) => setOptInNetwork(e.target.checked)}
              className="w-3.5 h-3.5 accent-[#16181D]"
            />
            <span>Include external network ping (api.telegram.org)</span>
          </label>
          <button
            onClick={runDiagnostics}
            disabled={loading}
            className="text-xs font-semibold underline text-[#1D6B52]"
          >
            {loading ? "Inspecting..." : "Re-run Diagnostic"}
          </button>
        </div>

        {/* Diagnostic Items List */}
        <div className="p-6 overflow-y-auto space-y-3 flex-1 bg-[#FFFFFF]">
          {loading && !report && (
            <div className="py-12 text-center text-sm font-mono text-[#5F626A]">
              Inspecting system configuration, isolated data paths, and tool chains...
            </div>
          )}

          {report?.items.map((item, idx) => (
            <div
              key={idx}
              className="border border-[#C8C2B5] rounded-xl p-3.5 bg-[#F5F3EE] flex items-start justify-between gap-4"
            >
              <div className="flex-1">
                <div className="flex items-center gap-2 mb-1">
                  <span className="font-mono text-[10px] uppercase text-[#5F626A] font-semibold">
                    {item.category}
                  </span>
                  <span className="text-xs text-[#C8C2B5]">•</span>
                  <span className="font-mono text-xs font-bold text-[#16181D]">
                    {item.name}
                  </span>
                </div>
                <p className="text-xs text-[#16181D] leading-relaxed">{item.message}</p>
                {item.remedy && (
                  <p className="text-xs text-[#86560D] mt-1.5 font-medium">
                    Remedy: {item.remedy}
                  </p>
                )}
              </div>

              {/* Status Badge */}
              <div className="shrink-0">
                <span
                  className={`inline-flex items-center px-2.5 py-1 rounded-full font-mono text-[10px] font-bold ${
                    item.status === "pass"
                      ? "bg-[#E3EFE8] text-[#1D6B52] border border-[#1D6B52]"
                      : item.status === "warn"
                      ? "bg-[#F6ECD9] text-[#86560D] border border-[#86560D]"
                      : "bg-[#F8E9E4] text-[#B53A26] border border-[#B53A26]"
                  }`}
                >
                  {item.status.toUpperCase()}
                </span>
              </div>
            </div>
          ))}
        </div>

        {/* Footer */}
        <div className="bg-[#F5F3EE] px-6 py-4 border-t border-[#C8C2B5] flex justify-between items-center text-xs font-mono text-[#5F626A]">
          <span>All inspection tokens, secrets, and keys are 100% sanitized & redacted.</span>
          <button
            onClick={onClose}
            className="rounded-full bg-[#16181D] text-[#F5F3EE] px-5 py-2 text-xs font-medium hover:bg-black transition-colors"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
};
