# Accept Artifacts (`camera.json` / `camera.txt` / `report.json`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On accept only, write `camera.json`, `camera.txt`, and `report.json`; stop writing mid-session `session.json` / duplicate `accepted.json`.

**Architecture:** Keep scoring/calib unchanged. Replace session JSON writers in `session.rs` with three builders + one `write_accept_artifacts`. Wire `accept_session` to that helper; make mid-session `persist` a no-op for JSON.

**Tech Stack:** Rust (`serde_json`), existing `session` / `capture_flow` tests, Vue README only.

**Spec:** `docs/superpowers/specs/2026-09-07-accept-artifacts-design.md`

## File map

| Path | Responsibility |
|------|----------------|
| `src-tauri/src/session.rs` | Builders + writers for accept artifacts; remove session/accepted JSON writers |
| `src-tauri/src/capture_flow.rs` | Accept writes artifacts; persist no longer writes JSON |
| `src-tauri/tests/session_test.rs` | Unit tests for new payloads/files |
| `src-tauri/tests/capture_flow_test.rs` | Flow tests: no mid JSON; accept files; failure paths |
| `README.md` | Smoke checklist file names |

---

### Task 1: Builders + writers in `session.rs` (TDD)

**Files:** `src-tauri/src/session.rs`, `src-tauri/tests/session_test.rs`

- [ ] **Step 1: Failing tests** for `camera_json_value`, `camera_txt_content`, `report_json_value`, `write_accept_artifacts` (assert filenames and key fields / TXT line count).

- [ ] **Step 2: Implement** builders/writers; remove or stop exporting `write_session_json` / `write_accepted_json` once callers updated (may leave temporarily if Task 2 not done).

- [ ] **Step 3: `cargo test --test session_test` PASS**

- [ ] **Step 4: Commit** `feat(session): write camera.json txt and report on accept`

---

### Task 2: Wire `capture_flow` + update flow tests

**Files:** `src-tauri/src/capture_flow.rs`, `src-tauri/tests/capture_flow_test.rs`, `README.md`

- [ ] **Step 1: Update tests** — accept creates three files; mid-session has no `session.json`; failure tests target `camera.json` (or first written file); remove assertions that mid-session JSON lists images.

- [ ] **Step 2: Implement** `accept_session` → `write_accept_artifacts`; `persist` returns `Ok(())` without writing (or delete JSON path); remove fallback rewrites of `session.json`.

- [ ] **Step 3: `cargo test --test session_test --test capture_flow_test` PASS**

- [ ] **Step 4: Update README smoke lines; commit** `feat(capture): emit accept artifacts instead of session json`
