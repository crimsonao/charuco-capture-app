# Author display (NSIS + Start page) design

Date: 2026-09-07  
Status: approved for planning

## Problem

Author `oz` exists in `src-tauri/Cargo.toml` (`authors`) but is not visible to end users: the NSIS installer has no publisher, and the app start page does not show an author line.

## Goals

- NSIS installer shows publisher **oz**.
- Start page (`StartPage`) shows a footer-style author line **© oz**.
- Minimal change; match existing Vue scoped-style patterns.

## Non-goals

- Syncing author from `Cargo.toml` at build/runtime.
- Showing author on Setup / Capture / Done pages.
- About dialog, version page, or EXE file properties beyond what Tauri/NSIS already derive from config.
- Changing product name or installer branding beyond publisher.

## Decisions (from brainstorm)

| Topic | Choice |
|-------|--------|
| Display name | `oz` (same as `Cargo.toml` authors) |
| Start page placement | Footer corner: `© oz` |
| Approach | Hardcode in config + UI (Approach 1) |

## Changes

### 1. NSIS publisher

File: `src-tauri/tauri.conf.json`

Add under `bundle`:

```json
"publisher": "oz"
```

Requires a fresh `tauri build` / NSIS bundle for the installer UI to pick it up.

### 2. Start page footer

File: `src/pages/StartPage.vue`

- Add a footer element at the bottom of `<main class="start">` with text `© oz`.
- Style as muted, small type (e.g. gray), bottom of the start layout — corner/footer feel, not competing with the title or primary CTA.
- Keep existing header, directory field, and Begin button unchanged.

## Verification

- Rebuild Windows NSIS installer; confirm publisher shows as `oz` in installer UI / Add-Remove Programs publisher field as produced by Tauri NSIS.
- Open app start page; confirm `© oz` appears in the footer area without layout breakage.

## Out of sync note

`Cargo.toml` authors, `bundle.publisher`, and StartPage footer are three literals. Changing the name later means updating all three intentionally.
