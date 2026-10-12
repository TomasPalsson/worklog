Approved: 2026-10-12 by user (pre-approved: /goal directive "do both 1 and 2 … make it a star field … explore with wasd")

# Spec — Topic star field

## TL;DR

The Topic atlas map shows a topic's dots in two or three far-apart groups (live: Tenant Support Agents, 13 dots, 85% nearest-same), and the 10-topic cap squeezes unrelated work into one topic. This feature (1) lets the data pick the number of topics (silhouette, up to 20, no topic under 3 blocks), (2) adds cluster gravity inside the t-SNE so every topic forms one island, and (3) adds a 3-D **Star field** view of the same blocks that the user can fly through with WASD + mouse, toggled from the galaxy card.

## 1. Context

- Owner request (2026-10-12): "do both 1 and 2 [map follows topics, raise the cap] … explore the map a little bit more perhaps make it a star field so squash it in to 3 dimensions … explore with wasd or something cool make it look amazing … see this in a fun and interesting way".
- Measured on the live 30-day data (198 blocks), `verify/proto_k.py`, `verify/proto_gravity.py`, `verify/mapcheck.py`:
  - silhouette (cosine, centered vectors, average linkage) rises from 0.150 at k=10 to 0.156 at k=13 and 0.181 at k=18; with "no topic under 3 blocks" the best k is 13.
  - t-SNE without gravity: 8 of 13 topics split into separate map groups (2-D), 5 of 13 (3-D); nearest-dot-same-topic 0.93 / 0.95.
  - with in-loop centroid gravity (after early exaggeration): 0 of 13 split, nearest-same 1.00 in both 2-D and 3-D; too much gravity (0.01) breaks it again (5 split).

## 2. Scope

In: k selection (Rust), cluster gravity in the layout (Rust, 2-D and 3-D), 3-D star coordinates on each map point, a distinguishable colour rule for up to 20 topics, the Star field view (web), a Map / Star field toggle on the galaxy card.
Out: phone layout (owner ruling, 023), WebGL or new npm dependencies (canvas 2-D is enough for ≤ 2 000 stars), changes to naming, river, mirror, compass, moments beyond what more topics imply.

## 3. Journeys

1. Owner opens /stats → Topic galaxy shows ~13 topic islands, each one connected blob.
2. Owner clicks **Star field** → a dark 3-D field of glowing stars (one per block, coloured by topic, sized by hours) slowly orbits; a hint says how to fly.
3. Owner clicks into the field, flies with WASD, looks around by dragging, hovers a star to see the block, clicks a topic name to fly to it, presses R to reset, Esc to leave.

## 4. Requirements

| ID | Requirement |
|----|-------------|
| FR-01 | k = argmax silhouette (cosine, centered vectors) over k ∈ [MIN_TOPICS, min(MAX_TOPICS=20, ⌊n/5⌋)] among partitions whose smallest topic has ≥ MIN_TOPIC_BLOCKS=3 blocks; ties → smaller k; if none qualifies, the old rule clamp(round(√(n/2)), MIN_TOPICS, MAX_TOPICS). Deterministic. |
| FR-02 | The average-linkage merge history is computed once per request; every k is a cut of it (no re-clustering per k). |
| FR-03 | The layout takes the topic label of each vector; after early exaggeration each point is pulled toward its topic's current centroid (gravity), so no topic is split into map groups on a split-cluster fixture. |
| FR-04 | Layout is computed in 2-D (x, y in [0,1]², unchanged meaning) and 3-D (sx, sy, sz in [0,1]³, centred, aspect preserved); both deterministic. |
| FR-05 | `MapPoint` carries `sx, sy, sz`; Rust and TS contracts match. |
| FR-06 | Topic colours stay distinguishable up to 20 topics: golden-angle hue order with three lightness steps; the same rule everywhere (topicColor). |
| FR-07 | The galaxy card has a two-option toggle **Map / Star field** (buttons with aria-pressed); Map is the default; the choice is remembered per viewer (localStorage, try/catch). |
| FR-08 | Star field renders every map point as a star (topic colour, radius by √hours, glow), topic nebulae at topic centroids, topic labels, first-time twinkle and oddball ring, a deterministic far background of dust stars, depth fog, back-to-front order, DPR-aware canvas. |
| FR-09 | Controls when the field has focus: W/S forward/back, A/D strafe, E/Q up/down, Shift ×3 speed, arrow keys and pointer drag look, wheel dolly, R reset, Esc releases focus and closes the panel; keys never scroll the page while focused; nothing is captured when not focused. |
| FR-10 | Hovering a star shows its block (label, topic, day, hours); clicking pins an info panel; clicking a topic label or legend chip flies the camera to that topic (eased, instant under reduced motion). |
| FR-11 | Idle auto-orbit until first input; none under prefers-reduced-motion; the render loop stops when the field is off-screen, the tab is hidden, or nothing moves. |
| FR-12 | Accessibility: canvas has role="img" and a summary aria-label; the hint is linked by aria-describedby; Map view stays the full keyboard/screen-reader path; a no-canvas fallback message. |
| FR-13 | The Star field code is loaded only when the user opens it (dynamic import), so /stats does not ship it by default. |

## 5. Non-functional requirements

- /stats/topics at n=2 000 blocks: k selection + both layouts finish in ≤ 2× the current cluster+layout time (bench test, ignored by default, prints timings).
- Star field ≥ 50 fps with 2 000 stars on a laptop (manual check in Chrome).
- No new npm or crate dependencies.

## 6. Launch criteria

- Live copy (30 d): chosen k between 11 and 16; split topics on the 2-D map = 0 (`verify/mapcheck.py` style check on the API output); nearest-same ≥ 0.95.
- Chrome (Claude in Chrome when connected): Star field opens, flies with WASD, hover and click work, 0 console errors, dark and light.

## 7. Assumptions

- Silhouette with min size 3 is the k rule (measured best among simple rules on live data); gravity constant tuned by test on a split-cluster fixture, not by eye.
- Canvas 2-D (no three.js) is enough for ≤ 2 000 stars; sprites pre-rendered per topic colour.
- More topics means more naming calls (4 topics per call): ~4 calls instead of 3; cached afterwards.
- Merge/install of this PR is not part of the goal; the owner decides after review.

## 8. Open questions

None blocking.
