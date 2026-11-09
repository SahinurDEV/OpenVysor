# Security Policy

## Reporting a vulnerability

Please do not open a public issue. Use **Report a vulnerability** on the repository's
[Security tab](https://github.com/SahinurDEV/OpenVysor/security), and include the steps to
reproduce the problem and its impact.

OpenVysor is maintained on a best-effort basis. Fixes go into the latest release.

## What to keep in mind

- OpenVysor can see and control a phone's screen, and uses USB debugging to do so. Only allow
  debugging from computers you trust.
- Clipboard sync is off unless you turn it on. When on, text copied on either side reaches the
  other, including passwords.
- Builds are not yet code-signed. Compare downloads against the published `SHA256SUMS.txt`.
