# Voco Build & Configuration Audit Report

**Scope:** Build tooling, dependency manifests, Tauri configuration, capability files, and platform-specific macOS settings for the Voco app under `/Volumes/Extreme SSD/voco`.

**Compared against:** `implementation_plan.md` lines 1261–1316 (Rust dependencies) and architecture requirements (floating pill, macOS private API, audio/screen permissions, LSUIElement, universal binary, etc.).

**Audit date:** 2025-01-21

**Overall build status:**
- `cargo check` in `src-tauri/` succeeds with only warnings (no compilation errors).
- `pnpm build` in the repo root succeeds and produces `dist/`.
- The project *structurally* builds, but many planned features cannot function until the missing dependencies, plugins, macOS entitlements, and Tauri capability/permission declarations are added.

---

## Executive Summary

The current build configuration is a working Tauri 2 starter shell that is **missing the majority of the planned macOS-specific and feature-specific wiring**. The Rust backend compiles and the frontend bundles, but the Cargo manifest lacks several crates and plugins required by the plan, the Tauri config has no macOS private API or pill-window settings, and the capability file is too restrictive for any plugin-backed feature. Without these changes, the app will fail to deliver the floating dictation pill, encrypted API-key storage, notifications, window state, export, clipboard injection, and the planned ML/ScreenCaptureKit features.

---

## Findings

### 🔴 Critical Issues

#### C1 — `macos-private-api` is not enabled

- **Files:** `src-tauri/Cargo.toml` (line 21), `src-tauri/tauri.conf.json` (missing `app.macOSPrivateApi`)
- **Plan requirement:** `tauri = { version = "2", features = ["macos-private-api", "tray-icon"] }` and `macOSPrivateApi: true` in Tauri config (see `implementation_plan.md` line 1266 and the architecture warning on line 65).
- **Current state:**
  - `Cargo.toml` only enables `tray-icon`.
  - `tauri.conf.json` has no `app.macOSPrivateApi` key.
- **Impact:** Transparent, frameless, always-on-top windows (the floating pill overlay) cannot be created on macOS. This is a hard blocker for the dictation pill UI and any frosted-glass / window-vibrancy effects.
- **Fix:** Add `macos-private-api` to the `tauri` feature list and add `"macOSPrivateApi": true` under `app` in `tauri.conf.json`.

#### C2 — Missing Rust crates and plugins required by the plan

`src-tauri/Cargo.toml` is missing several dependencies listed in `implementation_plan.md` lines 1261–1316.

| Dependency | Status in `Cargo.toml` | Plan line | Feature impact |
|---|---|---|---|
| `tauri-plugin-store` | Missing | 1268 | Encrypted API-key storage |
| `tauri-plugin-notification` | Missing | 1269 | Toast / recording notifications |
| `tauri-plugin-positioner` | Missing | 1270 | Tray icon positioning |
| `tauri-plugin-window-state` | Missing | 1271 | Remember window geometry |
| `hound` | Missing | 1284 | WAV export / audio replay |
| `ort` (with `coreml`) | Missing | 1288 | Silero VAD, Parakeet ONNX STT |
| `llama-cpp-rs` (with `metal`) | Missing | 1291 | Embedded GGUF LLM inference |
| `speakrs` | Missing | 1294 | Speaker diarization |
| `reqwest-eventsource` | Missing | 1301 | SSE streaming for LLM APIs |
| `window-vibrancy` | Missing | 1304 | Frosted-glass window effects |
| `objc2` | Missing | 1305 | macOS-native interop (ScreenCaptureKit, accessibility) |

Additionally, `tauri-plugin-global-shortcut` is present in Rust but not exposed to the frontend via `package.json` or capabilities, so the planned hotkey settings UI cannot be built.

- **Impact:** Every phase beyond the basic audio-capture core is incomplete or cannot compile once implemented.
- **Fix:** Add the missing crates to `src-tauri/Cargo.toml` with the feature flags listed above. After adding crates, call `.plugin(...)` for each in `src-tauri/src/lib.rs` (currently only `opener` and `global-shortcut` are initialized).

#### C3 — Missing macOS runtime permissions and `LSUIElement`

- **Files:** `src-tauri/tauri.conf.json` (no `bundle.macOS` section), no `Info.plist` file exists.
- **Plan requirement:** `LSUIElement: true` to hide the app from the Dock when tray-only (line 1255), plus microphone and screen-recording permission flows.
- **Current state:** There is no custom `Info.plist` and no `bundle.macOS.infoPlist` entry to merge one.
- **Impact:**
  - The app appears in the Dock even when the user intends tray-only operation.
  - macOS will show generic system prompts without explanatory usage text for microphone and screen capture, leading to user confusion and possible rejection by permission dialogs.
- **Fix:** Create `src-tauri/Info.plist` and add the required usage-description keys, then reference it from `tauri.conf.json` via `bundle.macOS.infoPlist`. Recommended keys:
  - `LSUIElement` → `true`
  - `NSMicrophoneUsageDescription` → explain why microphone access is needed
  - `NSScreenCaptureUsageDescription` → explain why system-audio capture needs screen recording permission
  - `NSAccessibilityUsageDescription` → if macOS Accessibility API text injection is implemented later

### 🟠 High Issues

#### H1 — Tauri window configuration does not support the floating pill overlay

- **File:** `src-tauri/tauri.conf.json` (lines 13–19)
- **Plan requirement:** Frameless, transparent, always-on-top pill window (line 1077, 988).
- **Current state:** Only one default window is declared, with `title: "tauri-app"`, `width: 800`, `height: 600`. No `transparent`, `decorations`, `alwaysOnTop`, `resizable`, `shadow`, `hiddenTitle`, or `skipTaskbar` settings are present.
- **Impact:** Even after `macOSPrivateApi` is enabled, there is no window definition for the pill UI, so a transparent overlay cannot be created from config. The pill route in `src/App.tsx` (`windowLabel === "pill"`) has no corresponding Tauri window config.
- **Fix:** Add a second window entry (label `"pill"`) with at least:

```json
{
  "label": "pill",
  "url": "/",
  "width": 360,
  "height": 64,
  "resizable": false,
  "maximizable": false,
  "minimizable": false,
  "decorations": false,
  "transparent": true,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "hiddenTitle": true,
  "shadow": false,
  "visible": false,
  "focus": false
}
```

Also add `windowEffects` or use the `window-vibrancy` crate for the frosted-glass look.

#### H2 — Capability file is missing plugin permissions

- **File:** `src-tauri/capabilities/default.json` (lines 6–9)
- **Current state:** Only `core:default` and `opener:default` are granted.
- **Plan impact:** The following Tauri v2 features will be denied by the capability system unless their permissions are added:
  - Global shortcut registration (`global-shortcut:allow-register`, `global-shortcut:allow-unregister`, `global-shortcut:allow-is-registered`)
  - Encrypted settings/key storage (`store:default` or `store:allow-*`)
  - Notifications (`notification:default` or `notification:allow-*`)
  - Tray positioner (`positioner:default`)
  - Window state persistence (`window-state:default`)
  - File dialogs for export (`dialog:default`)
  - File-system writes for export (`fs:default` / `fs:allow-write-text-file`)
  - Clipboard copy/paste and text injection (`clipboard-manager:default`)
  - Window creation / management for the pill (`core:window:allow-create`, `core:window:allow-set-size`, `core:window:allow-set-position`, `core:window:allow-set-always-on-top`, `core:window:allow-close`, `core:window:allow-hide`, `core:window:allow-show`)
- **Fix:** Expand `src-tauri/capabilities/default.json` to include the required plugin permissions. If the pill window is created from the frontend, either add `"pill"` to the `windows` array or create a separate `pill.json` capability with a broader window glob and the core window permissions.

#### H3 — Frontend is missing the JavaScript plugin packages

- **File:** `package.json` (lines 12–28)
- **Current state:** Only `@tauri-apps/api` and `@tauri-apps/plugin-opener` are installed.
- **Missing packages needed for planned features:**
  - `@tauri-apps/plugin-global-shortcut`
  - `@tauri-apps/plugin-store`
  - `@tauri-apps/plugin-notification`
  - `@tauri-apps/plugin-positioner`
  - `@tauri-apps/plugin-window-state`
  - `@tauri-apps/plugin-dialog`
  - `@tauri-apps/plugin-fs`
  - `@tauri-apps/plugin-clipboard-manager`
- **Additional missing dependency from plan:** `react-audio-visualize` (line 85 of plan calls for it alongside the custom Canvas waveform).
- **Impact:** The frontend cannot invoke any of the missing plugins, even after the Rust crates are added. The waveform visualization plan is also partially unmet.
- **Fix:** Add the matching JS packages to `dependencies` in `package.json` and run `pnpm install`.


### 🟡 Medium Issues

#### M1 — Generic branding and bundle identifier

- **File:** `src-tauri/tauri.conf.json` (lines 3–5, 15)
- **Current state:** `productName: "tauri-app"`, `title: "tauri-app"`, `identifier: "com.kashy.tauri-app"`.
- **Plan expectation:** The app is called *Voco*; the identifier should reflect the product (e.g., `com.kashy.voco`).
- **Impact:** App menu, Dock tooltip, and DMG installer all show "tauri-app" instead of the product name. Also noted by the frontend audit under app icons/branding.
- **Fix:** Update `productName`, `title`, and `identifier` to Voco branding.

#### M2 — No Content Security Policy configured

- **File:** `src-tauri/tauri.conf.json` (line 21)
- **Current state:** `"csp": null`
- **Impact:** Production builds run with no CSP, increasing XSS risk if the webview loads any untrusted content. The app is currently local-only, so this is medium rather than critical.
- **Fix:** Add a restrictive CSP, e.g. `"default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' http://localhost:*; img-src 'self' data:;"` (adjusted for API/websocket needs).

#### M3 — No macOS bundle-specific settings

- **File:** `src-tauri/tauri.conf.json` (lines 24–34)
- **Current state:** `bundle` only sets `active: true`, `targets: "all"`, and icons. There is no `bundle.macOS` section.
- **Plan requirement:** Universal binary (`arm64 + x86_64`), energy-efficient packaging, and a minimum macOS version.
- **Impact:** The default build may not produce a universal binary and may default to `10.13` minimum version. Some ScreenCaptureKit / Metal features require newer macOS versions.
- **Fix:** Add a `bundle.macOS` block with:
  - `minimumSystemVersion` (e.g., `"13.0"` or `"14.0"` depending on ScreenCaptureKit requirements)
  - `targets` set to produce universal binaries
  - `hardenedRuntime` (default true, keep it)
  - `infoPlist` pointing to the custom `Info.plist` created in C3

#### M4 — React version mismatch with the plan

- **File:** `package.json` (lines 17–18)
- **Current state:** React `^19.1.0` and React DOM `^19.1.0`.
- **Plan expectation:** React 18 (line 83 of plan).
- **Impact:** Astryx already declares `react >=19.0.0` peer dependency, so React 19 is compatible with the installed UI library. However, it diverges from the plan document.
- **Fix:** No functional change required unless the team wants strict alignment; note it for consistency.

### 🟢 Low Issues / Notes

#### L1 — `vite.config.ts` does not expose `TAURI_` environment variables

- **File:** `vite.config.ts`
- **Current state:** Standard Vite/Tauri config with `clearScreen: false`, port 1420, strict port, and `src-tauri` ignored. No `envPrefix`.
- **Impact:** `import.meta.env.TAURI_PLATFORM`, `TAURI_ARCH`, etc. are not available if the frontend ever needs platform checks.
- **Fix:** Add `envPrefix: ["VITE_", "TAURI_"]` to `vite.config.ts`.

#### L2 — `pnpm-workspace.yaml` contains an unusual `allowBuilds` key

- **File:** `pnpm-workspace.yaml`
- **Current state:** `allowBuilds: esbuild: true`.
- **Impact:** Harmless in the current monorepo-less setup; pnpm treats the repo as a workspace anyway.
- **Fix:** Verify whether this is intentional for later workspace growth; otherwise remove or convert to a standard `packages: []` workspace file.

#### L3 — Extra dependency not in plan

- **File:** `src-tauri/Cargo.toml` (line 22), `package.json` (line 16)
- **Current state:** `tauri-plugin-opener` is installed in both Rust and JS and is used by the Screen Recording onboarding component.
- **Note:** This is not a bug; it is a legitimate addition to open macOS privacy settings. Just note that it is an addition not present in the original dependency list.


---

## Build Verification

I ran the two primary build commands in the current environment (macOS arm64):

```bash
cd /Volumes/Extreme\ SSD/voco/src-tauri && cargo check --message-format=short
# Result: Finished dev profile in 0.78s; 9 warnings (unused imports / variables)

cd /Volumes/Extreme\ SSD/voco && pnpm build
# Result: tsc + vite build succeeded; dist/ emitted
```

`cargo check` and `pnpm build` are expected to keep working after the configuration changes above, because those changes are additive (new dependencies, config keys, and permissions). The first time the missing Rust crates are added, `cargo build` will need to fetch and compile them, which is normal.

---

## Configuration Change Checklist

Use this checklist when updating the project to match the plan.

### Cargo / Rust dependencies
- [ ] Add `macos-private-api` to `tauri` features in `src-tauri/Cargo.toml`.
- [ ] Add `tauri-plugin-store = "2"` to `src-tauri/Cargo.toml`.
- [ ] Add `tauri-plugin-notification = "2"` to `src-tauri/Cargo.toml`.
- [ ] Add `tauri-plugin-positioner = "2"` to `src-tauri/Cargo.toml`.
- [ ] Add `tauri-plugin-window-state = "2"` to `src-tauri/Cargo.toml`.
- [ ] Add `hound = "3.5"` to `src-tauri/Cargo.toml`.
- [ ] Add `ort = { version = "2.0", features = ["coreml"] }` to `src-tauri/Cargo.toml`.
- [ ] Add `llama-cpp-rs = { version = "0.4", features = ["metal"] }` to `src-tauri/Cargo.toml`.
- [ ] Add `speakrs = "0.1"` to `src-tauri/Cargo.toml`.
- [ ] Add `reqwest-eventsource = "0.6"` to `src-tauri/Cargo.toml`.
- [ ] Add `window-vibrancy = "0.5"` to `src-tauri/Cargo.toml`.
- [ ] Add `objc2 = "0.5"` to `src-tauri/Cargo.toml`.
- [ ] Initialize the new plugins in `src-tauri/src/lib.rs` via `.plugin(...)`.

### Frontend dependencies
- [ ] Add `@tauri-apps/plugin-global-shortcut` to `package.json`.
- [ ] Add `@tauri-apps/plugin-store` to `package.json`.
- [ ] Add `@tauri-apps/plugin-notification` to `package.json`.
- [ ] Add `@tauri-apps/plugin-positioner` to `package.json`.
- [ ] Add `@tauri-apps/plugin-window-state` to `package.json`.
- [ ] Add `@tauri-apps/plugin-dialog` to `package.json` (for export dialogs).
- [ ] Add `@tauri-apps/plugin-fs` to `package.json` (for export writes).
- [ ] Add `@tauri-apps/plugin-clipboard-manager` to `package.json` (for copy/paste).
- [ ] Add `react-audio-visualize` to `package.json` (per plan).
- [ ] Run `pnpm install`.

### Tauri configuration (`tauri.conf.json`)
- [ ] Add `"macOSPrivateApi": true` under `app`.
- [ ] Rename `productName` to `"Voco"` and `title` to `"Voco"`.
- [ ] Update `identifier` to a product-specific reverse-DNS string (e.g., `"com.kashy.voco"`).
- [ ] Add a `"pill"` window entry with `transparent: true`, `decorations: false`, `alwaysOnTop: true`, `resizable: false`, `shadow: false`, `hiddenTitle: true`, `skipTaskbar: true`.
- [ ] Add `windowEffects` / vibrancy settings to the main and/or pill window.
- [ ] Add a `bundle.macOS` section with `minimumSystemVersion`, `targets`, and `infoPlist` pointing to `src-tauri/Info.plist`.
- [ ] Add a reasonable `app.security.csp` value.

### macOS permissions (`src-tauri/Info.plist`)
- [ ] Create `src-tauri/Info.plist` with `LSUIElement` set to `true`.
- [ ] Add `NSMicrophoneUsageDescription` with a user-facing reason.
- [ ] Add `NSScreenCaptureUsageDescription` with a user-facing reason.
- [ ] Add `NSAccessibilityUsageDescription` if Accessibility API text injection is implemented.
- [ ] Wire the plist into `bundle.macOS.infoPlist` in `tauri.conf.json`.

### Capabilities
- [ ] Add `global-shortcut:allow-register`, `global-shortcut:allow-unregister`, and `global-shortcut:allow-is-registered` to `src-tauri/capabilities/default.json`.
- [ ] Add `store:default` (or granular `store:allow-*` permissions) when store plugin is used.
- [ ] Add `notification:default` when notification plugin is used.
- [ ] Add `positioner:default` when positioner plugin is used.
- [ ] Add `window-state:default` when window-state plugin is used.
- [ ] Add `dialog:default` and `fs:default` (or scoped file permissions) for export functionality.
- [ ] Add `clipboard-manager:default` for copy/paste and text injection.
- [ ] Add `core:window:allow-create`, `core:window:allow-set-size`, `core:window:allow-set-position`, `core:window:allow-set-always-on-top`, `core:window:allow-close`, `core:window:allow-hide`, and `core:window:allow-show` for pill window management.
- [ ] Either add `"pill"` to the `windows` array of the default capability or create a dedicated `pill.json` capability for the pill window.

### Vite configuration
- [ ] Add `envPrefix: ["VITE_", "TAURI_"]` to `vite.config.ts` if the frontend needs `TAURI_*` env vars.

---

## Bottom Line

The build pipeline itself is healthy: `cargo check` and `pnpm build` both pass. The project is not failing to compile, but it is **missing most of the planned configuration and dependencies** that turn a generic Tauri app into the Voco product. The most urgent fixes are enabling the macOS private API, adding the missing Rust crates and Tauri plugins, declaring the macOS permissions and `LSUIElement`, and expanding the capability file so the frontend can actually use the plugins. Once those are in place, the remaining work is feature implementation; until then, large parts of the architecture cannot be wired up.

