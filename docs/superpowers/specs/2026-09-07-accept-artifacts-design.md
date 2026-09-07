# Accept Artifacts Output Design

**Date:** 2026-09-07  
**Branch:** `perf/faster-camera-open` (or current feature branch)  
**Status:** Approved (approach A)

## Problem

On accept, the app writes `session.json` and `accepted.json` with the same payload. Mid-session also rewrites `session.json`. Callers need three distinct final artifacts and no intermediate JSON/TXT.

## Decision

**Approach A:** Write once on successful accept only:

| File | Role |
|------|------|
| `camera.json` | Intrinsics + session summary (no `images`) |
| `camera.txt` | Plain-text 3×3 matrix + 5 distortion coeffs |
| `report.json` | Overall score + per-image scores |

During collect / improve / manual stop: keep only `img_*.jpg`. Do not write `session.json` or `accepted.json`.

## Schemas

### `camera.json`

```json
{
  "accepted": true,
  "averageGrade": "B",
  "averagePercent": 82.4,
  "cameraCalibrationData": {
    "cameraMatrix": [[fx, 0, cx], [0, fy, cy], [0, 0, 1]],
    "distCoeffs": [k1, k2, p1, p2, k3]
  },
  "markerMm": 15.0,
  "meanReprojectionError": 0.4866,
  "overallReprojectionError": 0.5713,
  "scoreTarget": 80.0,
  "scoreTargetGrade": "B",
  "squareMm": 20.0
}
```

Rounding: percents 1 decimal; reprojection errors 4 decimals (match existing helpers).

### `camera.txt`

```
fx 0.0 cx
0.0 fy cy
0.0 0.0 1.0
k1
k2
p1
p2
k3
```

Space-separated matrix rows; one coeff per line. Use the same `CalibResult` numbers as JSON (full float formatting is fine).

### `report.json`

```json
{
  "averagePercent": 82.4,
  "averageGrade": "B",
  "scoreTarget": 80.0,
  "images": [
    {
      "path": "<absolute path>",
      "percent": 85.0,
      "grade": "B",
      "reprojectionError": 0.42
    }
  ]
}
```

## Behavior

- `accept_session` builds all three payloads from `SessionConfig` + shots + `last_calib`, writes all three, then emits Done.
- If any write fails: do not set `accepted`, do not emit Done; best-effort delete any of the three files already written in that attempt; show existing save-error hint.
- `persist` no longer writes JSON (no-op or removed). Mid-session failure tests that blocked `session.json` are replaced by accept-time failure tests on the new files.

## Non-goals

- Intermediate score snapshots on disk
- Keeping `session.json` / `accepted.json` aliases
- Changing scoring / calibration math
- Changing JPEG autosave

## Test plan

- Unit: builders for camera JSON nesting, TXT lines, report image list
- Flow: accept writes three files; mid-session dir has only JPGs; accept write failure does not Done
- README smoke checklist updated
