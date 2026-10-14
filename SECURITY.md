# Security policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems. Use GitHub's private reporting:
<https://github.com/MarshallNickolauson/ScreenCraft/security/advisories/new>

Include the version, your OS, and steps to reproduce. You can expect an acknowledgement within a few
days. ScreenCraft is maintained by one person, so fixes are best-effort.

## Scope

In scope: the desktop app, the installers, and the website source in this repository.
Out of scope: third-party components (report those upstream), and social-engineering or physical attacks.

## What the app does and does not do

- Runs locally; no accounts, telemetry or background network access.
- Network requests: an optional, notify-only update check (one small JSON file from screencraft.mdnlabs.dev, sends only the app version, can be turned off, never installs anything) and the optional download of a Whisper model from Hugging Face.
- Keyboard logging stores only shortcuts and special keys, never plain typing.
- Recordings and projects live in `~/ScreenCraft Projects`; they are not encrypted.
