# Voco Frontend Audit Report

**Audit scope:** All files under `/Volumes/Extreme SSD/voco/src/`
**Reference:** `/Volumes/Extreme SSD/voco/implementation_plan.md`
**Audited on:** Frontend-auditor agent run

## Executive Summary

The Voco frontend is a **React 19 + Vite + TypeScript** application that consumes **Astryx `@astryxdesign/core` components**. It has a functional main window shell and a partially-built pill window, but the implementation is **heavily weighted toward UI scaffolding and mock/demo data**, with many backend-facing capabilities only stubbed or simulated. The **Dictation mode is almost entirely simulated**, while **Meeting mode** relies on real Tauri commands for data but still feeds itself with hard-coded mock dialogue. Theme support is limited to **two custom themes** rather than the eight-curated Astryx design system the plan calls for. No real settings for hotkeys, audio devices, system tray, or microphone permissions exist yet.

Overall: **Phase 1 shell and Phase 3 data UI are largely present**, but **Phase 2 dictation logic and Phase 5 UX polish are largely missing or stubbed**.

---

## Phase 1: Foundation Status

| Task | ID | Status | Notes |
|------|----|--------|-------|
| Scaffold Tauri 2 + React + Vite + TypeScript | P1.1 | ✅ Functional | `package.json` uses React 19, Vite 7, Tauri 2, TypeScript 5.8. |
| Install Astryx + neutral theme | P1.2 | ✅ Installed / ⚠️ Misused | `@astryxdesign/core` and `@astryxdesign/theme-neutral` are installed and imported in `src/styles/index.css`. However, most components use raw `style={{ ... }}` with hard-coded or custom CSS variables, bypassing Astryx semantic tokens. |
| App shell with `AppShell` + `SideNav` | P1.8 | ✅ Functional | `MainWindow.tsx` renders `AppShell` with a `SideNav` containing Dictation, Meetings, and Settings items. |
| System tray menu | P1.7 | ❌ Missing | No frontend code wires or reacts to tray menu events. |
| Global hotkeys | P1.9 | ❌ Missing | `tauri-plugin-global-shortcut` is in `Cargo.toml`, but no frontend code listens for or displays global shortcuts. Dictation UI shows a hard-coded `Option + Space` label with no real hook. |
| SQLite DB usage | P1.10 | ✅ Functional | Frontend correctly invokes `get_meetings`, `get_meeting_transcript`, `start_meeting`, `stop_meeting`, `add_meeting_segment`, `update_meeting_duration`, `rename_speaker`. |
| Base theme system (Midnight + Daylight) | P1.11 | ⚠️ Partial | Only two custom themes are implemented. The plan calls for 8 Astryx curated themes. |

### Component-level findings (Phase 1)

| File | Functional? | Real Tauri? | Theme / Astryx? | Notes |
|------|-------------|-------------|-----------------|-------|
| `App.tsx` | ✅ Shell functional | `getCurrentWindow()` used | ⚠️ Uses `Theme` but forces only `neutralTheme` and custom `data-theme` attributes. | Handles `main` vs `pill` window routing. Theme persisted in `localStorage`. |
| `main.tsx` | ✅ Standard | N/A | ✅ Imports Astryx CSS layers | No issues. |
| `styles/index.css` | ✅ | N/A | ⚠️ Imports Astryx layers but defines custom component layer | Only 2 theme files imported. |
| `styles/pill.css` | ✅ | N/A | ⚠️ Custom theme overrides | Hard-coded `rgba` values; uses custom variables. |
| `styles/themes/midnight.css` | ✅ | N/A | ❌ Custom color palette | Replaces Astryx palette with hand-rolled colors. |
| `styles/themes/daylight.css` | ✅ | N/A | ❌ Custom color palette | Same as above. |

---

## Phase 2: Dictation Mode Status

| Task | ID | Status | Notes |
|------|----|--------|-------|
| Whisper-rs Metal integration | P2.1 | ✅ Backend present | Out of frontend scope, but no frontend model selection drives dictation. |
| Silero VAD | P2.2 | ✅ Backend present | Not wired into frontend feedback. |
| `SttEngine` trait | P2.3 | ✅ Backend present | Not exposed to frontend. |
| Dictation service (hotkey → capture → STT → result) | P2.4 | ❌ Stubbed | `MainWindow` Dictation tab only toggles local `isRecording` and increments a timer. No `start_dictation` command is called. No text injection occurs. |
| Pill overlay window | P2.5 | ⚠️ Partial | `PillWindow.tsx` exists but has no open/close lifecycle control; it only shows if `windowLabel === "pill"`. |
| Waveform visualization | P2.6 | ⚠️ Partial | `WaveformCanvas.tsx` is a real Canvas animation. The dictation tab uses a **mock CSS pulse wave** instead of this component. |
| Stream audio levels + partial text | P2.7 | ✅ Listens | `PillWindow` listens to `transcription-partial`/`partial-transcription`; `WaveformCanvas` listens to `audio-level`/`audio_level`. |
| Text injection via macOS Accessibility API | P2.8 | ❌ Missing | No frontend invocation or backend command exposed to inject text. |
| Push-to-talk, toggle, auto-stop modes | P2.9 | ❌ Missing | Dictation UI is a simple toggle with no mode selection. |
| Model Manager: download Whisper GGUF | P2.10 | ⚠️ Partial | `ModelSelector` calls `download_model`, but includes a **simulated fallback** that ticks progress locally if the backend does not emit events. |
| Model selector UI | P2.11 | ⚠️ Partial | `ModelSelector` renders cards and groups by provider, but the list is mixed with **hard-coded external models** and only local models come from the backend. |

### Component-level findings (Phase 2)

| File | Functional? | Real Tauri? | Theme / Astryx? | Notes |
|------|-------------|-------------|-----------------|-------|
| `windows/PillWindow.tsx` | ⚠️ Partial UI | `stop_dictation` invoked; events listened | ✅ Uses Astryx colors via CSS vars | No open/close control; only a stop button. |
| `components/waveform/WaveformCanvas.tsx` | ✅ Real canvas | Listens to Tauri events | ⚠️ Uses Astryx accent color if available, otherwise hard-coded violet | Not used in the main Dictation tab. |
| `components/models/ModelSelector.tsx` | ⚠️ Real commands + stubs | `list_models`, `get_providers`, `download_model`, `delete_model` | ✅ Uses Astryx components | Hard-codes external models per provider type; simulated download fallback. |
| `components/models/ModelCard.tsx` | ✅ Pure UI | N/A | ✅ Uses Astryx components | Tier/RAM helpers are hard-coded heuristics. |
| `components/models/ModelDownloader.tsx` | ✅ Pure UI | N/A | ✅ Uses Astryx components | No issues. |

---

## Phase 3: Meeting Mode + Diarization Status

| Task | ID | Status | Notes |
|------|----|--------|-------|
| System audio capture | P3.1 | ✅ Backend present | Frontend does not control audio device selection. |
| Speaker diarization | P3.2 | ❌ Stubbed | `MainWindow` injects a **hard-coded mock dialogue** every 7 seconds via `add_meeting_segment`. No real diarization events are consumed. |
| Meeting DB / history | P3.3 | ✅ Functional | `get_meetings`, `fetchMeetings`, `MeetingList` all work against real Tauri commands. |
| Meeting session | P3.4 | ✅ Functional | `start_meeting` and `stop_meeting` are invoked; `active_meeting_id` setting is read on mount. |
| Real-time transcript streaming | P3.5 | ⚠️ Polling | Frontend polls `get_meeting_transcript` every second rather than streaming events. |
| Diarization UI | P3.6 | ✅ Present | `SpeakerTimeline`, `SpeakerBadge`, `SegmentCard` are fully implemented and interactive. |
| Meeting summary | P3.7 | ✅ Functional | `summarize_meeting` and `regenerate_summary` are invoked from `SummaryView` callbacks. |
| Meeting history | P3.8 | ✅ Functional | `MeetingList` displays title, date, duration, summary snippet. |

### Component-level findings (Phase 3)

| File | Functional? | Real Tauri? | Theme / Astryx? | Notes |
|------|-------------|-------------|-----------------|-------|
| `windows/MainWindow.tsx` | ⚠️ Mixed real + mock | Many real commands; also simulates transcript | ⚠️ Heavy inline styling | Core orchestration. Dictation is a stub; meeting recording is fed by fake dialogue. |
| `components/meeting/MeetingControls.tsx` | ✅ Pure UI | N/A | ✅ Uses Astryx `Button` | `onStart` prop is a no-op in `MainWindow`. Pause/Resume only set React state. |
| `components/meeting/MeetingTimer.tsx` | ✅ Pure UI | N/A | ✅ Uses Astryx `Text` | No issues. |
| `components/meeting/MeetingList.tsx` | ✅ Pure UI | N/A | ✅ Uses Astryx components | No issues. |
| `components/meeting/SpeakerTimeline.tsx` | ✅ Pure UI | N/A | ✅ Uses Astryx components | Uses custom speaker color variables. |
| `components/meeting/SummaryView.tsx` | ⚠️ UI functional | Callbacks call real commands in parent | ✅ Uses Astryx components | Copy-to-clipboard is local browser API; Export dropdown has **no real export commands**. |
| `components/meeting/ScreenRecordingOnboarding.tsx` | ✅ Functional | Uses `@tauri-apps/plugin-opener` | ✅ Uses Astryx components | Opens macOS Screen Recording privacy settings. No microphone-permission onboarding. |
| `components/transcript/SegmentCard.tsx` | ✅ Pure UI | N/A | ✅ Uses Astryx components | No issues. |
| `components/transcript/SpeakerBadge.tsx` | ✅ Functional | N/A | ⚠️ Custom badge styles | Double-click rename works; parent calls `rename_speaker` on save. |
| `components/transcript/TranscriptView.tsx` | ✅ Functional | N/A | ✅ Uses Astryx components | Search, auto-scroll, empty states implemented. |


---

## Phase 4: Provider System Status

| Task | ID | Status | Notes |
|------|----|--------|-------|
| Provider configuration | P4.1 | ✅ Functional | `ProviderForm` adds providers via `add_provider`. |
| External provider UI | P4.2 | ✅ Functional | `ProviderList` shows status, edit, delete. |
| Model provider selection per mode | P4.3 | ❌ Not wired | Default STT/LLM provider selects exist in Settings but dictation/meeting flows do not read them. |
| API key management | P4.4 | ⚠️ Partial | `ProviderForm` accepts keys and masks them; no `update_provider` command is used for editing. |

### Component-level findings (Phase 4)

| File | Functional? | Real Tauri? | Theme / Astryx? | Notes |
|------|-------------|-------------|-----------------|-------|
| `components/providers/ProviderForm.tsx` | ✅ Functional | `add_provider` | ✅ Uses Astryx components | Only calls `add_provider`; no `update_provider`. Embedded provider cannot be edited. |
| `components/providers/ProviderList.tsx` | ✅ Functional | `get_providers`, `delete_provider` | ✅ Uses Astryx components | Hard-codes an `embedded` provider if backend list does not include one. |
| `components/providers/ProviderStatus.tsx` | ✅ Functional | `test_provider_connection` | ✅ Uses Astryx components | Embedded is always marked healthy. |

---

## Phase 5: UX Polish Status

| Task | ID | Status | Notes |
|------|----|--------|-------|
| Full onboarding (permissions, theme, first model) | P5.1 | ❌ Partial | Only a screen-recording onboarding modal exists. No microphone permission flow, no first-model download walkthrough. |
| Toast notifications | P5.2 | ❌ Missing | No `Toast` component or event listener. |
| Theme switching | P5.3 | ⚠️ Partial | Toggle works for 2 custom themes; plan calls for 8. No per-theme accent color switching. |
| Keyboard shortcut settings | P5.4 | ❌ Missing | No hotkey configuration UI or command. |
| App icons / branding | P5.5 | ❌ Missing | Title is still `tauri-app` in `tauri.conf.json`; only a hard-coded `V` logo in the sidebar. |
| DMG packaging | P5.6 | ❌ Out of scope | Not a frontend-code issue, but `tauri.conf.json` has default bundle settings and no notarization configuration. |

---

## Summary of Working / Stubbed / Missing Frontend Features

### Working (functional against real Tauri commands or real UI)
- Main application shell with Astryx `AppShell` + `SideNav`.
- Window routing (`main` vs `pill`) via `getCurrentWindow().label`.
- Meeting list display, selection, and history.
- Starting/stopping a meeting via `start_meeting` / `stop_meeting`.
- Fetching and displaying transcript segments.
- Renaming speakers via `rename_speaker`.
- Generating / regenerating meeting summaries via `summarize_meeting` / `regenerate_summary`.
- Provider list, add, delete, and connection testing.
- Saving default STT/LLM provider settings.
- Model list display and download/delete commands.
- Screen recording onboarding modal that opens macOS settings.
- Pill window UI and audio-level waveform canvas.
- Copy-to-clipboard for summary.

### Stubbed / Simulated
- **Dictation mode** — only a timer and a CSS mock pulse wave; no `start_dictation`, no real transcription, no text injection.
- **Meeting transcription feed** — `MainWindow` writes a hard-coded rotating dialogue to the DB every 7 seconds while recording.
- **Model download progress** — `ModelSelector` has a local simulation fallback that ticks progress if the backend does not emit events.
- **External models** — `ModelSelector` fabricates models for Ollama, OpenAI, Groq, etc., based on provider type rather than querying them.
- **Pause/Resume** in meeting controls — only local React state; no backend command.
- **Dictation waveform** — the dictation tab uses a mock bar animation instead of the real `WaveformCanvas` component.

### Missing
- Real global hotkey registration and UI for customization.
- System tray menu integration.
- Text injection into the active macOS application.
- Push-to-talk / toggle / auto-stop dictation modes.
- Opening and closing the pill window from the backend on demand.
- Microphone permission onboarding.
- Audio input/output device selection.
- Real export of summaries (PDF, Markdown, TXT).
- Toast / notification system.
- All 8 Astryx curated themes (only 2 custom themes exist).
- Consistent use of Astryx design tokens instead of custom CSS variables and inline styles.
- Proper app branding (icons, product name in config).
- `update_provider` command for editing providers.
- Keyboard shortcut configuration UI.
- Real-time streaming transcript events (currently polling).


---

## Theming & Astryx Design System Assessment

**What is used correctly:**
- The app imports Astryx reset, base styles, and the neutral theme in `index.css`.
- Astryx primitives (`Card`, `Button`, `Text`, `VStack`, `HStack`, `TextInput`, `Divider`, `Badge`, `ProgressBar`, `AppShell`, `SideNav`) are used throughout.

**What deviates from the Astryx design system:**
- Almost every component passes large inline `style` objects that override Astryx defaults (colors, radii, shadows, padding).
- Custom CSS variables (`--color-background-app`, `--color-accent`, `--color-recording`, etc.) are defined in `styles/themes/` and referenced directly, rather than using Astryx's own token layer.
- Only **two themes** are implemented (`midnight` and `daylight`) instead of the **8 curated themes** described in the plan.
- The pill window uses hard-coded `rgba` values for glassmorphism instead of Astryx translucent surface tokens.
- The `Theme` provider in `App.tsx` is always passed `neutralTheme` and only switches `mode` between `"dark"` and `"light"`; no alternative Astryx theme is loaded.
- Speaker colors are hand-picked instead of using the Astryx categorical color palette.

**Recommendation:** Map the custom variables to Astryx semantic tokens, remove hard-coded inline styles, and implement the remaining 6 themes or switch fully to Astryx-provided theme files.

---

## Cross-Reference Matrix (Feature → Phase → Task ID)

| Feature | Phase | Task ID | Frontend Status |
|---------|-------|---------|-----------------|
| Tauri 2 + React + Vite scaffold | P1 | P1.1 | ✅ |
| Astryx install/import | P1 | P1.2 | ⚠️ Installed but not fully tokenized |
| App shell with `AppShell`/`SideNav` | P1 | P1.8 | ✅ |
| System tray menu | P1 | P1.7 | ❌ |
| Global hotkeys | P1 | P1.9 | ❌ |
| SQLite DB integration | P1 | P1.10 | ✅ |
| 8-curated theme system | P1 | P1.11 | ⚠️ 2 custom themes only |
| Whisper integration | P2 | P2.1 | ✅ Backend only |
| Dictation service workflow | P2 | P2.4 | ❌ Stubbed |
| Pill overlay window | P2 | P2.5 | ⚠️ UI exists, no lifecycle control |
| Waveform canvas | P2 | P2.6 | ✅ Component exists, not used in dictation tab |
| Stream audio/partial text | P2 | P2.7 | ✅ Listens to events |
| Text injection | P2 | P2.8 | ❌ |
| Dictation modes | P2 | P2.9 | ❌ |
| Model download UI | P2 | P2.10/P2.11 | ⚠️ Real commands + simulation fallback |
| System audio capture | P3 | P3.1 | ✅ Backend only |
| Speaker diarization | P3 | P3.2 | ❌ Simulated |
| Meeting DB | P3 | P3.3 | ✅ |
| Meeting session | P3 | P3.4 | ✅ |
| Real-time transcript | P3 | P3.5 | ⚠️ Polling, not streaming |
| Diarization UI | P3 | P3.6 | ✅ |
| Meeting summary | P3 | P3.7 | ✅ |
| Meeting history | P3 | P3.8 | ✅ |
| Provider config | P4 | P4.1 | ✅ |
| External provider UI | P4 | P4.2 | ✅ |
| Model provider selection | P4 | P4.3 | ❌ Not wired to mode flows |
| API key management | P4 | P4.4 | ⚠️ Basic, no update command |
| Onboarding | P5 | P5.1 | ⚠️ Screen recording only |
| Toast notifications | P5 | P5.2 | ❌ |
| Theme switching | P5 | P5.3 | ⚠️ 2 themes only |
| Keyboard shortcuts | P5 | P5.4 | ❌ |
| App icons/branding | P5 | P5.5 | ❌ |

---

## Risk Assessment

1. **Dictation is the headline feature and is still a mock.** If the backend is ready, the frontend needs to call `start_dictation`, receive final transcription, and inject text. This is a P2-critical gap.
2. **Meeting mode uses simulated dialogue.** The UI looks real, but every recording is fed with the same 10-line mock script. This should be replaced with real diarization/STT events.
3. **Model list mixes hard-coded external models.** Users may see models they cannot actually use if the provider does not expose them.
4. **Theme/branding is inconsistent.** Heavy inline styling and only two themes will make the app feel unfinished compared to the Astryx design target.
5. **No system tray or global hotkeys.** The app relies on being manually focused, which defeats the purpose of a floating dictation pill.
6. **Provider edit creates a new provider.** This can leave stale provider entries and is not a true edit workflow.

---

*End of frontend audit report.*

