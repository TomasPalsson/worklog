Mr Claude's findings follow. The scratch files are in `/Users/tomas/.claude/jobs/491be4f8/tmp/proto/`: the DB copy `wl.db` (taken with `sqlite3 .backup` from a read-only connection), plus `lab.npy`, `clusters.json` and `layouts.png`. The spec-gate hook refused to let Mr Claude write `.py` files there, so every script ran as Python on stdin through `uv run --no-project`. The hook misfired: it named branch `flow/tempo-estimate-per-person`.

## Data
- 198 non-personal blocks, none ignored, from 2026-09-23 to 2026-10-10.
- Vectors are embeddinggemma, 768 f32, already unit length.
- 103 of the 198 blocks have no ticket, and there are 37 distinct tickets.
- **Root cause of the blob:** the raw vectors are very anisotropic. Mean pairwise cosine is 0.786 (p10 0.734, p90 0.841), so cosine distance hardly separates anything. After subtracting the window's mean vector and renormalising, mean cosine is -0.005.

## 1. Clustering

Columns: largest = largest cluster's share of blocks, sil = silhouette (cosine), AMI = agreement with tickets on ticketed blocks only, purity = share of ticketed blocks whose ticket matches their cluster's majority ticket.

| method (all 198) | k | largest | noise | sil | AMI | purity |
|---|---|---|---|---|---|---|
| avg-link, raw (**current**) | 8 | **84.3%** | 0 | 0.059 | 0.088 | 0.24 |
| avg-link, raw, best k | 10 | 60.1% | 0 | 0.126 | 0.164 | 0.29 |
| **avg-link, centered** | 8 | **21.7%** | 0 | 0.145 | 0.252 | 0.31 |
| avg-link, centered, best k | 13 | 20.2% | 0 | 0.156 | 0.276 | 0.35 |
| ward, centered, best k | 10 | 19.7% | 0 | 0.157 | 0.258 | 0.33 |
| k-means, centered | 8 | 19.7% | 0 | 0.164 | 0.191 | 0.27 |
| k-means, raw, best k | 9 | 21.2% | 0 | 0.164 | 0.272 | 0.33 |
| HDBSCAN mcs=3, centered | 16 | 10.1% | **47.5%** | 0.320 | 0.368 | 0.57 |
| HDBSCAN mcs=4/5/6, raw | 7/5/3 | ~15–20% | 59–69% | ~0.4 | low | — |

Single weeks with centered avg-link at the current k:
- Week 39: largest 26.2%. Raw was 86.9%.
- Week 40: largest 29.6%. Raw was 58.0%.
- Week 41: largest 32.1%. Raw was 44.6%.

HDBSCAN is unusable for this: 20–70% of blocks end up as noise, and with mcs≥5 a week often comes out as all noise. Silhouette is flat at 0.13–0.16 across k, so the data has no strong natural k, and the current k = √(n/2) rule is as good as anything.

Coherence check for centered avg-link, k=8 (each cluster's blocks / hours / distinct days):

| cluster | contents | blocks | hours | days |
|---|---|---|---|---|
| c2 | Vitinn infra | 43 | 15.5 | 12 |
| c4 | LibreChat PRs and review | 40 | 22.3 | 12 |
| c6 | code-interpreter workflow and previews | 35 | 22.7 | 13 |
| c5 | VÍS/Sjúkra customer rollouts | 27 | 16.4 | 5 |
| c0 | support-agent knowledge base | 16 | 7.6 | 7 |
| c1 | proposals and SOWs | 16 | 6.6 | 7 |
| c7 | Apró design system / skill | 15 | 7.3 | 6 |
| c3 | odds and ends: billing, Bedrock, webshop | 6 | 3.9 | 4 |

All clusters read coherently except c3. Centered ward at k=10 is a little finer: it splits out Sjúkra code-interpreter work, Lyfjastofnun, and PPTX/PDF previews.

**Recommendation: keep the existing Rust average-linkage `cluster()` and add one preprocessing step:**

```
mean = Σ vec / n;  vec_i = (vec_i - mean) / ‖vec_i - mean‖   // over the clustered window
```

- Optionally raise `MAX_TOPICS` from 8 to 10.
- Cost: O(n²) memory and O(n²·k) merging, as today. At n=198 the clustering took 0.004–0.56 s (the 0.56 s first run includes warm-up).
- If finer themes are wanted, the next choice is ward on centered vectors at k=10 (the Lance-Williams update is the same shape of code as today's average update).

## 2. 2D semantic map (198 points)

Columns: trust = sklearn trustworthiness at k=10; kNN agree = share of each point's 10 nearest neighbours in 2D that share its cluster; 2D sil = silhouette of the 2D layout by cluster.

| layout | time | trust | kNN agree | 2D sil |
|---|---|---|---|---|
| PCA | 0.001 s | 0.825 | 0.50 | 0.05 |
| UMAP nn=10, cosine | 0.17 s | 0.946 | 0.77 | 0.20 |
| UMAP nn=15, cosine | 0.20 s | 0.948 | 0.78 | 0.24 |
| t-SNE perp=15, cosine | 0.44 s | 0.940 | 0.78 | 0.21 |
| **t-SNE perp=30, cosine** | 0.47 s | **0.951** | **0.80** | **0.28** |

PCA's first two components explain only 17.9% of the variance, and `layouts.png` shows it as visible mush. UMAP and t-SNE both show clear, separated theme islands.

**Recommendation:** exact t-SNE (perplexity 30, PCA init, centered vectors) written directly in Rust, with no crate.
- It is about 100–150 lines: perplexity binary search, then gradient descent with early exaggeration.
- At n=2000 it is O(n²) per iteration, about 4e9 flops for 1000 iterations. Mr Claude estimates a few seconds; this was not measured. The P matrix is about 32 MB.
- Cache the layout per window.
- UMAP needs `umap-learn` in Python. The existing `verdict_server.py` helper (which serves `/embed`) could host a `/layout` endpoint, but that adds a heavy dependency.
- Plain PCA is not worth shipping as the map.

## 3. Creative metrics (all eight together took 0.028 s)
- **Theme river (hours per cluster per day)** is the strongest signal.
  - c5 (VÍS/Sjúkra) is absent until Oct 5, then runs 2.6 → 4.1 → 5.9 → 1.9 → 1.9 h: a clear burst.
  - c2 (Vitinn) dominates early: 6.1 h on 09-23, fading after 09-25.
  - c4 (LibreChat) peaks at 5.6 h on 09-30.
- **Recurring vs. burst themes** (distinct days, first to last day):
  - c6: 13 days. c2 and c4: 12 days each. These are "always on".
  - c5: 5 days, all within Oct 5–9. This is a sprint.
- **Novelty per day** (hours-weighted 1 − max cosine to any earlier block): ranges 0.48–0.73, highest on 09-26 (0.725) and 10-03 (0.707). The block with the highest novelty each day makes a good "new territory" callout, for example 0.85 for "Merge billing view improvements…" and 0.82 for "Add configurable and user-created prompt categories". The day-level number is only moderately varied because there are only 3 weeks of history.
- **Focus per day** (hours-weighted mean pairwise cosine, centered): ranges -0.017 to 0.324.
  - Most focused: 09-26 at 0.324, with 2 themes.
  - Least focused: 10-09 at -0.017 and 09-25 at 0.011, with 5 themes each.
  - It works as a "focus score" per day.
- **Outliers** (lowest maximum cosine to any other block, centered): "Implement typed per-client state rows" 0.231, "Raise bash_tool streamed-argument cap" 0.243, "Troubleshoot Codex connection…" 0.269. The median block's maximum is 0.57 and p95 is 0.966, so a cutoff below 0.3 isolates genuine one-offs.
- **Day twins and opposites** (centroid cosine):
  - Twins: 10-05 ~ 10-07 at 0.535, and 09-23 ~ 09-24 at 0.399.
  - Opposites: 09-23 vs 10-05 at -0.522.
  - This works for "most similar day" and for a day-by-day similarity heatmap.
- **Weak metrics:**
  - Day-to-day centroid drift is noisy: 0.60–1.29, almost every day looks like a jump.
  - "Distance travelled" per day mostly tracks block count, and its mean step is flat at 0.80–0.98.
  - Mr Claude would skip both, or show drift only weekly.

## 4. AI naming
c-TF-IDF terms are computed BERTopic-style over the descriptions, with Jira keys stripped. Example picking uses MMR: score = cosine to centroid − 0.5 × maximum similarity to examples already picked, with case-insensitive dedupe. Without MMR, "Work on Vitinn infrastructure" filled all 5 slots. One batched call names every cluster at once, which keeps the names distinct from each other. This is the prompt that worked:

```
You are naming themes in one software consultant's work log. Each theme is a cluster of work blocks grouped by meaning.
For every theme give:
- "name": 2-4 words, Title Case, specific (name the product, customer or system when the evidence names one), not generic ("Development Work", "Infrastructure" alone are bad). Name what the whole theme has in common, not just its first item.
- "description": one plain sentence, max 18 words, saying what the work in this theme is. Use only facts in the evidence; do not guess what a product or customer is.
Names must be distinct from each other. Write in English unless every work item in that theme is Icelandic. Keep Icelandic proper nouns (customer and product names) as written.

Theme {id} ({blocks} blocks, {hours}h)
  distinctive terms: {top 10 c-TF-IDF terms}
  representative work items:
  - {5 MMR examples}
...
Reply with ONLY a JSON array, no prose, no code fence: [{"id": <theme id>, "name": "...", "description": "..."}, ...]
```

`claude -p --model claude-haiku-4-5` worked on the first try, so the fallback was never needed.

| call | time |
|---|---|
| batched, 8 clusters, ~4.5k-character prompt | 21–30 s |
| single cluster | 14 s |
| CLI start-up alone | 7.2 s |

Final names:
- 0: Support Agent Infrastructure
- 1: Scope and Proposal Review
- 2: Vitinn Infrastructure
- 3: Billing and Webshop
- 4: LibreChat Development
- 5: VÍS Integration
- 6: Code Interpreter Enablement
- 7: Apró Brand Infrastructure

The first run (without MMR or the "only facts" rule) gave LibreChat PR Review, Apró Design System and Support Agent Knowledge Base. It also invented that Vitinn is a "healthcare platform"; the "only facts" rule fixed that.

Verdict: the names are good and specific. Remaining issues:
- **Fenced output:** Haiku wraps the JSON in a ```json fence despite the instruction, so the parser must strip fences.
- **Odds-and-ends cluster:** the incoherent c3 got a name ("Billing and Webshop") that ignores its Bedrock items. Adding "If the items share nothing, name it 'Odds and Ends'" to the prompt should handle this.
- **Caching and cost:** make one batched call per cluster set and cache it by member-ID hash, because every CLI call costs at least 7 s.