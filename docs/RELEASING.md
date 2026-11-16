# Releasing

## Cutting a release

1. Update the version in `app/tauri.conf.json` (this is the one the installers use) and keep
   `app/Cargo.toml` and `core/Cargo.toml` in step.
2. Commit, then tag and push:
   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. The **Release** workflow builds installers for macOS (Apple Silicon and Intel), Windows and
   Linux, and attaches them with a `SHA256SUMS.txt` to a **draft** GitHub release. Review the draft,
   edit the notes, and publish it.

The workflow refuses to release if the tag doesn't match the version in `app/tauri.conf.json`.
You can also run it by hand from the Actions tab to get installers as workflow artifacts without
making a release.

| Platform | Installers |
| --- | --- |
| macOS | `.dmg` for Apple Silicon and for Intel |
| Windows | `-setup.exe` (NSIS) and `.msi` |
| Linux | `.deb`, `.rpm`, `.AppImage` |

## Signing: what you need and what users see

The app is distributed from GitHub, **not the App Store**, so none of the App Store certificates
apply. Signing is optional: without it everything still builds and runs, but users get warnings.

### macOS: Developer ID and notarization

Two things are involved, and both need a paid **Apple Developer Program** membership (US$99 a year):

- **Developer ID Application certificate.** The certificate Apple issues for apps distributed outside the
  App Store. It identifies you as the developer. (Don't confuse it with *Apple Distribution*, which is only for
  the App Store.)
- **Notarization.** You upload the signed app to Apple, which scans it automatically and returns a
  ticket that is stapled to the app. Apple requires it for Developer ID apps.

With both, the app opens normally. A free Apple account **cannot** notarize, and there is no free route
to a warning-free install.

**Setup**

1. Join the Apple Developer Program, then create a *Developer ID Application* certificate in the
   Certificates section of the developer account (it needs a certificate signing request from
   Keychain Access).
2. Export it from Keychain Access as a `.p12` with a password, then base64-encode it:
   `base64 -i certificate.p12 | pbcopy`.
3. Create an **app-specific password** at appleid.apple.com (used for notarization).
4. Add these repository secrets (Settings → Secrets and variables → Actions):

   | Secret | Value |
   | --- | --- |
   | `APPLE_CERTIFICATE` | the base64 `.p12` |
   | `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
   | `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` (see `security find-identity -v -p codesigning`) |
   | `APPLE_ID` | your Apple ID email |
   | `APPLE_PASSWORD` | the app-specific password |
   | `APPLE_TEAM_ID` | your 10-character team ID |

The workflow detects the secrets and signs and notarizes automatically. Without them it falls back
to **ad-hoc signing**, which lets the app run on Apple Silicon but does not remove Gatekeeper's warning.

**What users see without notarization.** macOS says the developer can't be verified. They can open the
app anyway: right-click it and choose **Open**, or go to System Settings → Privacy & Security →
**Open Anyway**. If macOS says the app is "damaged", run
`xattr -dr com.apple.quarantine "/Applications/Kiro Credits Bar.app"`.

### Windows

Unsigned installers work, but Microsoft SmartScreen shows "Windows protected your PC". Users click
**More info → Run anyway**. On Windows 11 with **Smart App Control** turned on, unknown unsigned
apps are blocked outright.

Options when you want to fix that (see Tauri's
[Windows signing guide](https://v2.tauri.app/distribute/sign/windows/)):

- An OV or EV code-signing certificate from a certificate authority. Since 2024 even EV certificates
  have to build reputation, so a new release can still warn for a while.
- Azure's code-signing service (formerly *Trusted Signing*, now *Artifact Signing*).
- A custom `signCommand` for any other signing tool.

This is not wired into the workflow yet. When you pick an option, it is a change to
`app/tauri.conf.json` and the Windows build step.

### Linux

Nothing to sign. `SHA256SUMS.txt` is attached to each release so users can verify downloads.

## Known gaps

- CI and release workflows have produced builds for all target platforms. Windows and Linux
  installers still need testing on real desktops.
- Intel macOS builds are cross-compiled on an Apple Silicon runner.
- There is no auto-update. Users download new releases themselves.
