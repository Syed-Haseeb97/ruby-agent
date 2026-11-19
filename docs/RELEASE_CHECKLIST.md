# Ruby Assistant — Production Release Checklist

## Pre-Release Verification (Local / Gated)

1. **Working Directory & Storage Rules Check**:
   - Ensure repository is located on Drive `D:\` (`D:\projects\ruby-agent`).
   - Confirm Drive `C:\` was never used for code, dependencies, or intermediate artifacts.
   - Confirm zero `.db` or `playwright_profile` directories exist in git:
     ```powershell
     git ls-files | findstr /i "playwright_profile .db$"
     # Output must be completely empty
     ```

2. **Secret Scan Verification**:
   - Run the automated static secret scanner across the entire repository:
     ```powershell
     python acceptance/secret_scan.py --all
     # Output must show: [SUCCESS] Secret scan clean! 0 secrets or API keys found across all scanned files.
     ```

3. **Full Workspace Test Suite**:
   - Run the complete Rust test suite across all 12 member crates:
     ```powershell
     cargo test --workspace
     # Must report: 0 failed, 0 filtered out
     ```

4. **Frontend Production Build & Security Audit**:
   - Run npm security audit and production build:
     ```powershell
     cd crates/ruby-desktop/frontend
     npm audit
     # Must report: found 0 vulnerabilities
     npm run build
     # Must compile with 0 TypeScript/Vite errors
     cd ../../..
     ```

5. **Diagnostic Doctor Check**:
   - Run the pre-flight doctor check:
     ```powershell
     cargo run -p ruby-desktop -- doctor --json
     ```
   - Confirm all critical platform checks report `healthy: true` (Path Isolation, Security Flags, Tooling).

---

## Build & Packaging Steps

6. **Generate SBOM (Software Bill of Materials)**:
   - Run the SBOM generator script to produce SPDX 2.3 JSON specification:
     ```powershell
     python scripts/generate_sbom.py --output dist/ruby-assistant-sbom.json
     ```
   - Verify generated JSON conforms to schema.

7. **Compile Release Binary**:
   - Build with production optimization flags (`lto = true`, `opt-level = 3`, `codegen-units = 1`, `panic = "abort"`, `strip = true`):
     ```powershell
     cargo build --release -p ruby-desktop
     ```
   - Output binary will be generated at `target/release/ruby-desktop.exe`.

8. **Package Windows Installer (Tauri Bundle)**:
   - Generate NSIS / MSI standalone bundle installer:
     ```powershell
     cargo tauri build
     ```
   - Output installer will be generated at `target/release/bundle/nsis/Ruby_0.1.0_x64-setup.exe` or `bundle/msi/`.

9. **Generate Release Checksums**:
   - Compute SHA-256 hashes of release binaries and installer:
     ```powershell
     Get-FileHash target/release/ruby-desktop.exe -Algorithm SHA256 | Format-List
     Get-FileHash target/release/bundle/nsis/Ruby_0.1.0_x64-setup.exe -Algorithm SHA256 | Format-List
     ```
   - Write hashes to `dist/SHA256SUMS.txt`.

---

## Tagging & Release Publication

10. **Create Git Release Tag**:
    - Tag release with semantic versioning:
      ```powershell
      git tag -a v0.1.0 -m "Release v0.1.0: Privacy-first agentic desktop assistant"
      ```

11. **Draft GitHub Release**:
    - Trigger `.github/workflows/release.yml` on tag push:
      ```powershell
      git push origin v0.1.0
      ```
    - Upload artifacts:
      - `Ruby_0.1.0_x64-setup.exe` (Windows NSIS Installer)
      - `ruby-desktop.exe` (Standalone release executable)
      - `ruby-assistant-sbom.json` (SPDX SBOM)
      - `SHA256SUMS.txt` (Cryptographic verification checksums)
