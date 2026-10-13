# CHK001: on the API output, count topics split into separate map groups and
# nearest-dot-same-topic, for the flat map (x, y) and the star field (sx, sy, sz).
# Run: uv run --with numpy --with scipy python split_check.py <report.json>
import json, sys
import numpy as np
from scipy.cluster.hierarchy import fcluster, linkage
from scipy.spatial.distance import pdist, squareform

d = json.load(open(sys.argv[1]))
P = d["points"]
lab = np.array([p["topic_id"] for p in P])
for name, keys in (("map 2-D", ("x", "y")), ("stars 3-D", ("sx", "sy", "sz"))):
    Y = np.array([[p[k] for k in keys] for p in P], dtype=float)
    D = squareform(pdist(Y)); np.fill_diagonal(D, np.inf)
    same = (lab[D.argmin(1)] == lab).mean()
    span = np.ptp(Y, 0).max(); split = []
    for t in sorted(set(lab)):
        Yt = Y[lab == t]
        if len(Yt) > 1 and len(set(fcluster(linkage(Yt, "single"), 0.15 * span, "distance"))) > 1:
            split.append(int(t))
    print(f"{name}: topics={len(set(lab))} nearest-same={same:.2f} split={len(split)} {split}")
