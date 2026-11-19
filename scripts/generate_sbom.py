#!/usr/bin/env python3
"""
Generates a Software Bill of Materials (SBOM) in SPDX 2.3 JSON format
by parsing Cargo.lock (Rust dependencies) and package-lock.json (Frontend dependencies).
"""

import json
import os
import sys
import argparse
from datetime import datetime, timezone

def parse_cargo_lock(cargo_lock_path):
    packages = []
    if not os.path.isfile(cargo_lock_path):
        return packages

    with open(cargo_lock_path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    current_pkg = {}
    for line in lines:
        line = line.strip()
        if line == "[[package]]":
            if current_pkg and "name" in current_pkg:
                packages.append(current_pkg)
            current_pkg = {}
        elif line.startswith("name = "):
            current_pkg["name"] = line.split("=", 1)[1].strip().strip('"')
        elif line.startswith("version = "):
            current_pkg["version"] = line.split("=", 1)[1].strip().strip('"')
        elif line.startswith("source = "):
            current_pkg["source"] = line.split("=", 1)[1].strip().strip('"')

    if current_pkg and "name" in current_pkg:
        packages.append(current_pkg)

    return packages

def parse_package_lock(npm_lock_path):
    packages = []
    if not os.path.isfile(npm_lock_path):
        return packages

    try:
        with open(npm_lock_path, "r", encoding="utf-8") as f:
            data = json.load(f)

        # npm lockfile v2 / v3 packages dictionary
        pkgs_dict = data.get("packages", {})
        for pkg_path, meta in pkgs_dict.items():
            if not pkg_path:  # Root package
                continue
            name = pkg_path.split("node_modules/")[-1]
            version = meta.get("version", "unknown")
            packages.append({
                "name": f"npm:{name}",
                "version": version,
                "source": "npm"
            })
    except Exception as e:
        print(f"Warning: Failed to parse package-lock.json: {e}", file=sys.stderr)

    return packages

def generate_spdx_sbom(cargo_pkgs, npm_pkgs):
    created_time = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    spdx_packages = [
        {
            "SPDXID": "SPDXRef-Package-ruby-agent",
            "name": "ruby-agent",
            "versionInfo": "0.1.0",
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
            "licenseConcluded": "MIT",
            "supplier": "Person: Syed Haseeb",
            "description": "Privacy-first agentic desktop assistant for Windows"
        }
    ]

    relationships = []

    seen = set()
    for pkg in cargo_pkgs:
        name = pkg.get("name")
        version = pkg.get("version", "0.0.0")
        key = (name, version)
        if key in seen:
            continue
        seen.add(key)

        spdx_id = f"SPDXRef-Package-cargo-{name}-{version}".replace(".", "-").replace("_", "-")
        spdx_packages.append({
            "SPDXID": spdx_id,
            "name": name,
            "versionInfo": version,
            "downloadLocation": pkg.get("source", "NOASSERTION"),
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE_MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": f"pkg:cargo/{name}@{version}"
                }
            ]
        })
        relationships.append({
            "spdxElementId": "SPDXRef-Package-ruby-agent",
            "relationshipType": "DEPENDS_ON",
            "relatedSpdxElement": spdx_id
        })

    for pkg in npm_pkgs:
        raw_name = pkg.get("name", "")
        clean_name = raw_name.replace("npm:", "")
        version = pkg.get("version", "0.0.0")
        key = (raw_name, version)
        if key in seen:
            continue
        seen.add(key)

        spdx_id = f"SPDXRef-Package-npm-{clean_name}-{version}".replace(".", "-").replace("/", "-").replace("@", "").replace("_", "-")
        spdx_packages.append({
            "SPDXID": spdx_id,
            "name": raw_name,
            "versionInfo": version,
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "externalRefs": [
                {
                    "referenceCategory": "PACKAGE_MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": f"pkg:npm/{clean_name}@{version}"
                }
            ]
        })
        relationships.append({
            "spdxElementId": "SPDXRef-Package-ruby-agent",
            "relationshipType": "DEPENDS_ON",
            "relatedSpdxElement": spdx_id
        })

    sbom = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "ruby-assistant-sbom",
        "documentNamespace": f"https://github.com/Syed-Haseeb97/ruby-agent/spdx/{created_time}",
        "creationInfo": {
            "created": created_time,
            "creators": [
                "Tool: ruby-agent-sbom-generator-1.0",
                "Person: Syed Haseeb"
            ]
        },
        "packages": spdx_packages,
        "relationships": relationships
    }

    return sbom

def main():
    parser = argparse.ArgumentParser(description="Generate SPDX 2.3 SBOM for Ruby Assistant")
    parser.add_argument("--cargo-lock", default="Cargo.lock", help="Path to Cargo.lock")
    parser.add_argument("--npm-lock", default=os.path.join("crates", "ruby-desktop", "frontend", "package-lock.json"), help="Path to package-lock.json")
    parser.add_argument("--output", "-o", default=None, help="Output file path (default stdout)")
    args = parser.parse_args()

    cargo_pkgs = parse_cargo_lock(args.cargo_lock)
    npm_pkgs = parse_package_lock(args.npm_lock)

    sbom = generate_spdx_sbom(cargo_pkgs, npm_pkgs)
    output_str = json.dumps(sbom, indent=2)

    if args.output:
        os.makedirs(os.path.dirname(os.path.abspath(args.output)), exist_ok=True)
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(output_str)
        print(f"SBOM written to {args.output} ({len(sbom['packages'])} packages)")
    else:
        print(output_str)

if __name__ == "__main__":
    main()
