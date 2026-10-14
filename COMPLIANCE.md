# Legal and compliance guide

This document explains what a project like ScreenCraft needs in order to be distributed legally and
responsibly, what has been done, and what remains for the maintainer or for anyone redistributing a build.

> **Not legal advice.** It is a practical checklist written by the maintainers. If you ship ScreenCraft
> commercially, bundle it in a product, or operate in a regulated industry, ask a qualified lawyer.

## 1. Our own license

| Item | Status |
| --- | --- |
| Project license | MIT, see [LICENSE](LICENSE). Copyright line names the maintainer. |
| Warranty / liability disclaimer | Included in the MIT text, repeated in the README and on the website's [Terms](https://screencraft.mdnlabs.dev/terms). |
| Contributions | Contributors keep their copyright and license their contribution under MIT by submitting it ("inbound = outbound"). There is no CLA. |

## 2. Third-party code and models

| Item | Where | Requirement |
| --- | --- | --- |
| Third-party notices | [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md), also on the website | MIT/BSD/ISC/Apache require the license and copyright notice to accompany binary distributions. This file is that notice. **Ship it (or a link to it) with every build.** |
| whisper.cpp / ggml | MIT | Credited in notices, README and the Captions panel. |
| OpenAI Whisper weights | MIT | Not bundled; downloaded by the user from Hugging Face. Credited. |
| symphonia and a few other crates | MPL-2.0 | File-level copyleft. Used unmodified; source is public on crates.io. If you modify those crates' files you must publish your modifications to them. |
| Dual-licensed crates (`MIT OR Apache-2.0`, `... OR LGPL`) | | We elect MIT/Apache-2.0. No LGPL code is linked. |
| Regenerating the list | `cargo metadata` over `src-tauri/Cargo.toml` | Re-run when dependencies change (see the notices file header). `cargo deny check licenses` is a good CI gate. |

No GPL/AGPL/LGPL code is linked into ScreenCraft; keep it that way, because it would change the terms
of distribution for the whole binary.

## 3. Trademarks and "inspired by"

- ScreenCraft is **not affiliated** with Screen Studio or any other product it resembles. The README and
  site say so. Do not use other products' names, logos or screenshots in marketing, and do not copy their
  artwork, code or copy text.
- "macOS", "Windows", "Apple Silicon", "Whisper", "Hugging Face" and similar names belong to their owners
  and are used descriptively only.
- Features and workflows are not protectable by copyright; implementations and assets are. All assets
  here were created for this project (see the notices).

## 4. Privacy law (GDPR, UK GDPR, CCPA/CPRA, etc.)

- The app has **no accounts or telemetry**. Its only network features are the notify-only update check (version number only, can be turned off in About, Privacy & Updates) and the user-initiated model download.
  It processes recordings only on the user's device, so the maintainer is not a data controller or
  processor for that content.
- The website uses **no cookies**, stores only the visitor's theme/language preference in local storage
  (a user-requested preference, never transmitted), and uses Cloudflare Web Analytics (cookieless,
  aggregate). A cookie banner is therefore not required. See [Privacy](https://screencraft.mdnlabs.dev/privacy)
  and [Cookies](https://screencraft.mdnlabs.dev/cookies).
- If you ever add analytics, crash reporting, accounts, update checks that send identifiers, or a
  cloud feature, you must update the privacy policy first and may need consent (ePrivacy/GDPR).
- The keyboard log stores only shortcuts and special keys, never plain typing. Keep it that way; storing
  keystrokes would be a keylogger in the eyes of users, antivirus tools and some laws.

## 5. Recording other people (user responsibility, flagged in the UI/docs)

Recording screens, microphones and cameras is regulated: many jurisdictions require consent of some or
all parties to record audio (wiretapping laws), and workplaces/schools may have policies. ScreenCraft's
[Terms](https://screencraft.mdnlabs.dev/terms) and docs tell users they are responsible for consent and for
the rights to anything they record or add (music, logos, third-party footage).

## 6. Platform requirements

| Platform | What is required for distribution | Status |
| --- | --- | --- |
| macOS | Usage-description strings for Screen Recording, Microphone, Camera, Accessibility (in `Info.plist`). Developer ID signing + notarization to avoid Gatekeeper warnings. | Strings present. Ad-hoc signed only; **notarization needs a paid Apple Developer account** (not done). |
| Windows | Authenticode code signing to avoid SmartScreen warnings. WebView2 runtime handling. | Unsigned; installer bootstraps WebView2 via Microsoft's official bootstrapper. A code-signing certificate (or Azure Trusted Signing) is a future step. |
| Both | Distribute the MIT license and third-party notices with the binary. | The repository root carries both; link them from any release page. |

## 7. Export controls and encryption

The app uses TLS (rustls/ring) only to download the optional model. Publicly available open-source
software with mass-market encryption is generally not restricted, but US EAR §742.15(b) asks for an
email notification to BIS/NSA for publicly available encryption source code in some cases. If you publish
builds from the US, check this once; it is a one-off notification, not an ongoing duty.

## 8. Security

- Report vulnerabilities privately; see [SECURITY.md](SECURITY.md).
- The website sets a strict Content-Security-Policy, `X-Frame-Options: DENY` and related headers (see
  `website/public/_headers`).
- Downloads are served from GitHub; publish SHA-256 sums with each release so users can verify them.

## 9. Accessibility and consumer-protection notes

- The website uses semantic HTML, keyboard-reachable controls, and honours `prefers-reduced-motion`.
- Don't claim certifications, awards or user counts you don't have, and keep "no watermark / free" claims
  true. Early-alpha status is stated on the download page.

## 10. Maintainer checklist before each release

1. Bump version in `Cargo.toml`, `src-tauri/tauri.conf.json`, `website/package.json`; update `CHANGELOG.md`.
2. Regenerate `THIRD-PARTY-NOTICES.md` if dependencies changed.
3. Build both installers; record SHA-256 sums in the release notes.
4. Confirm the privacy policy still matches what the app and site do.
5. Tag the release; attach installers; link LICENSE and THIRD-PARTY-NOTICES.
