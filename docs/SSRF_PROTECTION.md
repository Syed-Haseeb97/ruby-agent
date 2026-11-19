# SSRF & DNS Rebinding Security Architecture
**Document Path**: `docs/SSRF_PROTECTION.md`  
**Crate**: `ruby-policy`  
**Status**: ARCHITECTURAL SECURITY BASELINE  

---

## 1. Threat Model & Limitations of DNS-Time Checks

In HTTP client tools and automated browser sidecars (Playwright), Server-Side Request Forgery (SSRF) represents a critical risk of accessing loopback (`127.0.0.1`, `[::1]`), internal cloud metadata endpoints (`169.254.169.254`), and private networks (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`).

### The DNS Rebinding Attack Vector
1. An attacker registers a domain name (e.g. `rebind.attacker.com`) pointing to an attacker-controlled nameserver.
2. When `ruby-policy` inspects the URL, the nameserver responds with a legitimate public IP (`93.184.216.34`, TTL = 0).
3. The policy check passes because the initial resolution is benign.
4. When the actual HTTP client (e.g. `reqwest` or Playwright) connects microseconds later, the nameserver responds with `127.0.0.1`.
5. **Conclusion**: Pre-flight DNS resolution alone **CANNOT** mathematically prevent DNS rebinding.

---

## 2. Layered Defensive Strategy in Ruby

Ruby implements a three-tier defense-in-depth model:

### Tier 1: Strict Domain Allowlisting (PRIMARY CONTROL)
- Tools operating over the network must require explicit domain allowlisting (`allowed_domains`).
- Wildcards are restricted; arbitrary unvalidated domains are rejected by default.

### Tier 2: Static IP & Encoded Host Sanitization
- Rejects IP literals immediately without network overhead:
  - Loopback (`127.0.0.0/8`, `::1`)
  - Private networks (RFC 1918)
  - Link-local and cloud metadata (`169.254.0.0/16`, `fe80::/10`)
  - Broadcast and wildcard (`0.0.0.0`, `255.255.255.255`)
  - IPv6 mapped/translated addresses (`::ffff:127.0.0.1`)
  - Hex, octal, and dword alternate encodings (e.g. `0x7f.1`, `0177.0.0.1`, `2130706433`).

### Tier 3: Non-Blocking Pre-flight Resolution
- Pre-flight DNS resolution operates as a defense-in-depth secondary filter.
- It must not block the async runtime with synchronous blocking sockets.
- Redirect chains are re-validated at each step to prevent redirect-based SSRF into private networks.
