import os
import sys
import tempfile
import subprocess

def test_negative_secret_scan():
    # Build planted fixtures via runtime string concatenation to prevent static detection
    google_key = "AIza" + "SyD" + "1234567890abcdefghijklmnopqrstuvw"
    openai_key = "sk-" + "abcdefghijklmnopqrstuvwxyz1234567890"
    freellm_key = "freellmapi-" + "1234567890abcdef"
    tg_token = "123456789" + ":" + "abcdefghijklmnopqrstuvwxyz123456789"
    gh_pat = "ghp_" + "1234567890abcdefghijklmnopqrstuvwxyz"
    gh_fine = "github_pat_" + "1234567890abcdefghijklmnopqrstuvwxyz1234"
    priv_key = "-----" + "BEGIN RSA PRIVATE KEY" + "-----\n" + "MIIEowIBAAKCAQEA0"
    aws_key = "AKIA" + "IOSFODNN7EXAMPLE"
    hardcoded_assign = "api_key" + " = '1234567890abcdefghij'"

    fixtures = [
        ("google_key.txt", google_key, "Google API Key"),
        ("openai_key.txt", openai_key, "OpenAI/Anthropic Secret Key"),
        ("freellm_key.txt", freellm_key, "FreeLLMAPI Secret Key"),
        ("tg_token.txt", tg_token, "Telegram Bot Token"),
        ("gh_pat.txt", gh_pat, "GitHub Personal Access Token"),
        ("gh_fine.txt", gh_fine, "GitHub Fine-Grained PAT"),
        ("priv_key.txt", priv_key, "Private Key Header"),
        ("aws_key.txt", aws_key, "AWS Access Key ID"),
        ("assign.txt", hardcoded_assign, "Hardcoded API Key / Token"),
        ("leak.db", "SQLite format 3\x00", "Forbidden Private Data / Database File in Git"),
        ("playwright_profile_test", "profile_data", "Forbidden Private Data / Database File in Git"),
    ]

    scanner_path = os.path.join(r"D:\projects\ruby-agent", "acceptance", "secret_scan.py")

    with tempfile.TemporaryDirectory() as temp_dir:
        for fname, content, expected_desc in fixtures:
            file_path = os.path.join(temp_dir, fname)
            with open(file_path, "w", encoding="utf-8") as f:
                f.write(content)

            # Run secret_scan.py pointing at the planted fixture
            proc = subprocess.run(
                [sys.executable, scanner_path, file_path],
                capture_output=True,
                text=True,
                cwd=r"D:\projects\ruby-agent"
            )

            # Assert failure return code
            assert proc.returncode != 0, f"Scanner must fail on planted fixture {fname}, got returncode {proc.returncode}"
            assert expected_desc in proc.stdout, f"Expected '{expected_desc}' in output for {fname}, got: {proc.stdout}"
            print(f"[VERIFIED] Planted fixture {fname} ({expected_desc}) successfully flagged by secret_scan.py")

    print("\n[SUCCESS] All 11 negative test fixtures successfully caught by secret_scan.py!")

if __name__ == "__main__":
    test_negative_secret_scan()
