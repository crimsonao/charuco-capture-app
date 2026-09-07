# Author Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show publisher `oz` in the NSIS installer and `© oz` in the Start page footer.

**Architecture:** Hardcode the same author string in two places — `tauri.conf.json` `bundle.publisher` for the Windows installer, and a muted footer in `StartPage.vue` for the in-app home screen. No shared build-time sync.

**Tech Stack:** Tauri 2 (`tauri.conf.json`), Vue 3 SFC (`StartPage.vue`)

## Global Constraints

- Display name is exactly `oz`
- Start page footer text is exactly `© oz`
- Do not show author on Setup / Capture / Done pages
- Do not add Tauri commands or Cargo.toml sync
- Match existing StartPage scoped CSS patterns (system-ui, muted grays)

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/tauri.conf.json` | NSIS / Windows publisher metadata |
| `src/pages/StartPage.vue` | Start page UI including footer author line |

---

### Task 1: NSIS publisher

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: none
- Produces: `bundle.publisher = "oz"` for Tauri NSIS bundling

- [x] **Step 1: Add publisher to bundle config**

In `src-tauri/tauri.conf.json`, under the existing `"bundle"` object, add `"publisher": "oz"` alongside `active`, `targets`, `icon`, and `resources`:

```json
"bundle": {
  "active": true,
  "targets": ["nsis"],
  "publisher": "oz",
  "icon": [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.icns",
    "icons/icon.ico"
  ],
  "resources": {
    "opencv-runtime/*.dll": "./"
  }
}
```

- [x] **Step 2: Verify JSON is valid**

Run: `node -e "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')); console.log('ok')"`  
Expected: `ok`

- [ ] **Step 3: Commit** (only if user requested a commit)

```bash
git add src-tauri/tauri.conf.json
git commit -m "feat(bundle): set NSIS publisher to oz"
```

---

### Task 2: Start page footer

**Files:**
- Modify: `src/pages/StartPage.vue`

**Interfaces:**
- Consumes: none
- Produces: footer element with text `© oz` on the start page only

- [x] **Step 1: Add footer markup after `.start__actions`**

Inside `<main class="start">`, after the actions block, add:

```html
<footer class="start__footer" aria-label="作者">
  © oz
</footer>
```

- [x] **Step 2: Add footer styles**

Append to the existing `<style scoped>` block:

```css
.start__footer {
  margin-top: 2.5rem;
  font-size: 0.8rem;
  color: #888;
}
```

Keep header, directory input, status, and buttons unchanged.

- [ ] **Step 3: Visual check**

Run: `pnpm tauri dev` (or open the app if already running)  
Expected: Start page shows muted `© oz` below the primary actions; other pages unchanged.

- [ ] **Step 4: Commit** (only if user requested a commit)

```bash
git add src/pages/StartPage.vue
git commit -m "feat(ui): show author footer on start page"
```

---

### Task 3: Installer verification (manual)

**Files:** none (verification only)

- [ ] **Step 1: Rebuild NSIS bundle when ready**

Run the project's usual Windows release/bundle command (e.g. `pnpm tauri build`).

- [ ] **Step 2: Confirm publisher**

Open the generated NSIS installer / installed app entry and confirm publisher shows as `oz`.

---

## Spec coverage

| Spec requirement | Task |
|------------------|------|
| `bundle.publisher: "oz"` | Task 1 |
| Start page `© oz` footer | Task 2 |
| Fresh build for installer | Task 3 |
| No other pages / no Cargo sync | Global constraints |
