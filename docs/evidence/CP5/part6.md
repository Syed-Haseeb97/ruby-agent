# Part 6 Packaging & Release Engineering Evidence

## P1. Release Build & Bundler Configuration

### Cargo Release Profile (`Cargo.toml`)
Configured in root workspace `Cargo.toml`:
```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```
- Maximizes binary performance and security.
- Strips debug symbols to reduce binary surface and footprint.
- Enables Link-Time Optimization (LTO) across all workspace crates.

### Tauri Installer & Bundle Configuration (`crates/ruby-desktop/tauri.conf.json`)
```json
  "bundle": {
    "active": true,
    "targets": "all",
    "copyright": "Copyright 2026 Syed Haseeb / Ruby Assistant Team",
    "shortDescription": "Privacy-first agentic desktop assistant for Windows",
    "longDescription": "Ruby is an autonomous desktop AI assistant featuring speech dictation, liquid orb mood visualization, computer use, background tasks, and remote control.",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "windows": {
      "certificateThumbprint": null,
      "digestAlgorithm": "sha256",
      "timestampUrl": ""
    }
  }
```

---

## P2. Release Engineering Artifacts

### 1. Production Release Checklist (`docs/RELEASE_CHECKLIST.md`)
Step-by-step verification and release workflow:
- Pre-release verification (storage rules, secret scan, full tests, frontend build, doctor pre-flight)
- Build and packaging (SBOM generation, release binary compilation, Tauri installer bundling)
- Tagging and GitHub release publication

### 2. Software Bill of Materials (SBOM) Generator (`scripts/generate_sbom.py`)
- Python script extracting all workspace dependencies from `Cargo.lock` and `package-lock.json`.
- Outputs compliant SPDX 2.3 JSON specification.
- Verification command run:
  ```powershell
  python scripts/generate_sbom.py --output dist/ruby-assistant-sbom.json
  ```
- Result:
  ```json
  {
    "spdxVersion": "SPDX-2.3",
    "packages": 806,
    "relationships": 805
  }
  ```
- Clean JSON conforming to SPDX 2.3 schema with zero syntax errors.

### 3. GitHub Actions Release Pipeline (`.github/workflows/release.yml`)
- Triggers on tag push: `v*.*.*`.
- Matrix on Windows runner.
- Enforces private data check, secret scan, full workspace tests.
- Compiles release binaries, builds Tauri NSIS/MSI installer bundle, generates SPDX SBOM and SHA-256 checksums.
- Drafts GitHub Release via `softprops/action-gh-release@v2`.
- Local-only, committed to Git repository (no remote push).
