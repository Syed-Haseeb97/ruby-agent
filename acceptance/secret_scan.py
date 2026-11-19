import subprocess
import re
import os
import sys

# Scan mode: either staged files (default), all repository files (--all), or explicit file paths
if len(sys.argv) > 1 and sys.argv[1] == "--all":
    proc = subprocess.run(["git", "ls-files"], capture_output=True, text=True, cwd=r"D:\projects\ruby-agent")
    files = [f.strip() for f in proc.stdout.splitlines() if f.strip()]
elif len(sys.argv) > 1 and not sys.argv[1].startswith("--"):
    files = sys.argv[1:]
else:
    proc = subprocess.run(["git", "diff", "--cached", "--name-only"], capture_output=True, text=True, cwd=r"D:\projects\ruby-agent")
    files = [f.strip() for f in proc.stdout.splitlines() if f.strip()]

secret_patterns = [
    (r"AIza[0-9A-Za-z-_]{35}", "Google API Key"),
    (r"sk-[a-zA-Z0-9]{20,}", "OpenAI/Anthropic Secret Key"),
    (r"freellmapi-[A-Za-z0-9_\-]{16,}", "FreeLLMAPI Secret Key"),
    (r"[0-9]{8,10}:[A-Za-z0-9_-]{35}", "Telegram Bot Token"),
    (r"ghp_[a-zA-Z0-9]{20,}", "GitHub Personal Access Token"),
    (r"github_pat_[a-zA-Z0-9_]{20,}", "GitHub Fine-Grained PAT"),
    (r"BEGIN (?:RSA|OPENSSH|DSA|EC|PGP) PRIVATE KEY", "Private Key Header"),
    (r"AKIA[0-9A-Z]{16}", "AWS Access Key ID"),
    (r"(?:api_key|apikey|secret|token)\s*[:=]\s*['\"][A-Za-z0-9_\-]{16,}['\"]", "Hardcoded API Key / Token"),
]

print(f"Scanning {len(files)} files for secrets...")
found_secrets = []

for f in files:
    if "playwright_profile" in f.lower() or f.lower().endswith(".db"):
        found_secrets.append((f, "Forbidden Private Data / Database File in Git", 1))

    full_path = f if os.path.isabs(f) else os.path.join(r"D:\projects\ruby-agent", f)
    if not os.path.isfile(full_path):
        continue
    # Skip binary image files
    if f.endswith((".png", ".ico", ".jpg", ".jpeg", ".webp", ".gif")):
        continue
    try:
        with open(full_path, "r", encoding="utf-8", errors="ignore") as fp:
            content = fp.read()
            for pattern, desc in secret_patterns:
                matches = re.findall(pattern, content)
                if matches:
                    found_secrets.append((f, desc, len(matches)))
    except Exception as e:
        print(f"Error reading {f}: {e}")

if found_secrets:
    print("[FAIL] Secrets found:")
    for f, desc, count in found_secrets:
        print(f"  - {f}: {desc} ({count} occurrences)")
    sys.exit(1)
else:
    print("[SUCCESS] Secret scan clean! 0 secrets or API keys found across all scanned files.")
    sys.exit(0)
