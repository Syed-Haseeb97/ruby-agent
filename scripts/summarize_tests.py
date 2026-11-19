#!/usr/bin/env python3
"""
Test summary generator for Ruby Workspace.
Parses raw `cargo test` execution output and writes docs/TEST_SUMMARY.md
with accurate per-crate test counts.
"""

import sys
import re
import os
from collections import defaultdict
from pathlib import Path

def parse_cargo_test_output(raw_text: str):
    stats = defaultdict(lambda: {"passed": 0, "failed": 0, "ignored": 0, "tests": []})
    
    curr_crate = "unknown"
    crate_runner_re = re.compile(r"out[\\/]([a-zA-Z0-9_-]+)-[a-f0-9]+\.exe", re.IGNORECASE)
    test_re = re.compile(r"^test ([\w:]+) \.\.\. (ok|FAILED|ignored)")
    
    for l in raw_text.splitlines():
        m = crate_runner_re.search(l)
        if m:
            c = m.group(1).replace("_", "-")
            if "mock-telegram" in c:
                curr_crate = "ruby-remote (mock_telegram_tests)"
            elif c.startswith("ruby-"):
                curr_crate = c
            else:
                curr_crate = f"ruby-{c}"
                
        tm = test_re.match(l.strip())
        if tm:
            name, outcome = tm.groups()
            # Disambiguate interleaved ruby-remote targets
            if curr_crate == "ruby-remote" and name.startswith("test_"):
                crate = "ruby-remote (mock_telegram_tests)"
            elif curr_crate == "ruby-remote (mock_telegram_tests)" and (
                "r1_router" in name or "key_storage" in name or "antigravity" in name
            ):
                crate = "ruby-remote"
            else:
                crate = curr_crate
                
            stats[crate]["tests"].append((name, outcome))
            if outcome == "ok":
                stats[crate]["passed"] += 1
            elif outcome == "FAILED":
                stats[crate]["failed"] += 1
            else:
                stats[crate]["ignored"] += 1
                
    return stats

def generate_markdown_summary(crate_stats) -> str:
    md = []
    md.append("# Ruby Workspace Test Suite Summary\n")
    md.append("Generated automatically by `scripts/summarize_tests.py` from raw test execution logs.\n")
    md.append("| Target / Crate | Passed | Failed | Ignored | Total | Status |")
    md.append("| :--- | :---: | :---: | :---: | :---: | :---: |")
    
    total_passed = 0
    total_failed = 0
    total_ignored = 0
    
    sorted_crates = sorted(crate_stats.keys())
    for crate in sorted_crates:
        st = crate_stats[crate]
        p = st["passed"]
        f = st["failed"]
        i = st["ignored"]
        t = p + f + i
        if t == 0:
            continue
        total_passed += p
        total_failed += f
        total_ignored += i
        status = "PASSED" if f == 0 else "FAILED"
        md.append(f"| `{crate}` | {p} | {f} | {i} | {t} | {status} |")
        
    grand_total = total_passed + total_failed + total_ignored
    md.append("| :--- | :---: | :---: | :---: | :---: | :---: |")
    md.append(f"| **GRAND TOTAL** | **{total_passed}** | **{total_failed}** | **{total_ignored}** | **{grand_total}** | **{'ALL PASSED' if total_failed == 0 else 'FAILURES DETECTED'}** |\n")
    
    return "\n".join(md)

def main():
    raw = None
    if len(sys.argv) > 1 and os.path.exists(sys.argv[1]):
        path = sys.argv[1]
        with open(path, "rb") as f:
            data = f.read()
        if data.startswith((b"\xff\xfe", b"\xfe\xff")):
            raw = data.decode("utf-16", errors="replace")
        else:
            raw = data.decode("utf-8", errors="replace")
    else:
        if not sys.stdin.isatty():
            raw = sys.stdin.read()
        else:
            default_path = Path("docs/TEST_RUN_RAW.txt")
            if default_path.exists():
                with open(default_path, "rb") as f:
                    data = f.read()
                if data.startswith((b"\xff\xfe", b"\xfe\xff")):
                    raw = data.decode("utf-16", errors="replace")
                else:
                    raw = data.decode("utf-8", errors="replace")
            else:
                print("Usage: python scripts/summarize_tests.py [path_to_raw_test_output.txt]")
                sys.exit(1)
                
    stats = parse_cargo_test_output(raw)
    md_content = generate_markdown_summary(stats)
    
    out_path = Path("docs/TEST_SUMMARY.md")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(md_content)
        
    print(f"[SUCCESS] Wrote test summary to {out_path}")
    print(md_content)

if __name__ == "__main__":
    main()
