# Security Policy

## Reporting Security Vulnerabilities

Clared is an experimental reference implementation, not a production security product. We still treat boundary bypasses and misleading safety outcomes as security defects.

If you believe you have discovered a vulnerability, bypass, or state leak in Clared:

1. **Do not create a public GitHub issue.**
2. Send a report directly to: `liran@clared.ai`
3. Include:
   - A description of the vulnerability (such as capability forgery, resource-scope bypass, lifecycle escape, or receipt tampering).
   - Minimal reproduction code or test payload.
   - Expected vs actual execution behavior.

We aim to acknowledge reports within 48 hours and coordinate remediation before public disclosure.

The current backend is an in-memory simulator and holds no provider credentials. Reports should distinguish protocol enforcement defects from live-provider integration risks that are not implemented yet.
