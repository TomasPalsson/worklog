import json, sqlite3, numpy as np
from sklearn.cluster import AgglomerativeClustering
from sklearn.metrics import silhouette_score
from scipy.spatial.distance import pdist, squareform

d = json.load(open('/Users/tomas/.claude/jobs/491be4f8/tmp/live30.json'))
ids = [p['block_id'] for p in d['points']]
db = sqlite3.connect('file:/Users/tomas/.claude/jobs/491be4f8/tmp/wl/worklog.db?mode=ro', uri=True)
rows = dict(db.execute(f"select block_id, vec from block_embeddings where block_id in ({','.join(map(str, ids))})").fetchall())
ids = [i for i in ids if i in rows]
X = np.stack([np.frombuffer(rows[i], dtype='<f4') for i in ids]).astype(np.float64)
X /= np.linalg.norm(X, axis=1, keepdims=True); X = X - X.mean(0); X /= np.linalg.norm(X, axis=1, keepdims=True)
n = len(X)

def pick_k(X, kmax=20, min_size=3):
    best = (None, -1)
    for k in range(2, min(kmax, n // 5) + 1):
        lab = AgglomerativeClustering(n_clusters=k, metric='cosine', linkage='average').fit_predict(X)
        if np.bincount(lab).min() < min_size: continue
        s = silhouette_score(X, lab, metric='cosine')
        if s > best[1]: best = (k, s)
    return best

k, s = pick_k(X)
print('picked k', k, 'silhouette', round(s, 3))
lab = AgglomerativeClustering(n_clusters=k, metric='cosine', linkage='average').fit_predict(X)
print('sizes', sorted(np.bincount(lab), reverse=True))

def P_matrix(X, perp=30):
    D = squareform(pdist(X, 'cosine')) ** 2
    P = np.zeros((n, n)); logU = np.log(perp)
    for i in range(n):
        lo, hi, beta = 0, np.inf, 1.0
        Di = np.delete(D[i], i)
        for _ in range(60):
            p = np.exp(-Di * beta); sp = p.sum(); H = np.log(sp) + beta * (Di * p).sum() / sp
            if abs(H - logU) < 1e-5: break
            if H > logU: lo = beta; beta = beta * 2 if hi == np.inf else (beta + hi) / 2
            else: hi = beta; beta = (beta + lo) / 2
        P[i, np.arange(n) != i] = p / sp
    P = (P + P.T) / (2 * n)
    return np.maximum(P, 1e-12)

def tsne(P, dims, gravity, iters=750, seed=0):
    rng = np.random.default_rng(seed)
    Y = rng.normal(0, 1e-4, (n, dims)); V = np.zeros_like(Y); gains = np.ones_like(Y)
    for it in range(iters):
        ex = 12.0 if it < 250 else 1.0
        sq = (Y ** 2).sum(1); num = 1 / (1 + sq[:, None] + sq[None, :] - 2 * Y @ Y.T); np.fill_diagonal(num, 0)
        Q = np.maximum(num / num.sum(), 1e-12)
        PQ = (ex * P - Q) * num
        G = 4 * ((np.diag(PQ.sum(1)) - PQ) @ Y)
        if it >= 250 and gravity > 0:
            C = np.stack([Y[lab == c].mean(0) for c in range(k)])
            G += gravity * (Y - C[lab])
        mom = 0.5 if it < 250 else 0.8
        gains = (gains + 0.2) * ((G > 0) != (V > 0)) + gains * 0.8 * ((G > 0) == (V > 0)); gains = np.maximum(gains, 0.01)
        V = mom * V - 200 * gains * G; Y = Y + V; Y -= Y.mean(0)
    return Y

def agree(Y):
    D = squareform(pdist(Y)); np.fill_diagonal(D, np.inf); return (lab[D.argmin(1)] == lab).mean()
def spread(Y):
    D = squareform(pdist(Y)); m = D[np.triu_indices(n, 1)].mean()
    return float(np.mean([D[np.ix_(lab == c, lab == c)].mean() for c in range(k) if (lab == c).sum() > 1]) / m)
def parts(Y):
    # topics whose dots split into >1 group: single-link components with gap > 15% of map size
    from scipy.cluster.hierarchy import fcluster, linkage
    span = np.ptp(Y, 0).max(); split = 0
    for c in range(k):
        Yc = Y[lab == c]
        if len(Yc) < 2: continue
        comp = fcluster(linkage(Yc, 'single'), 0.15 * span, 'distance')
        split += len(set(comp)) > 1
    return split

P = P_matrix(X)
for dims in (2, 3):
    for g in (0.0, 0.002, 0.005, 0.01):
        Y = tsne(P, dims, g)
        print(f'dims={dims} gravity={g}: nearest-same={agree(Y):.2f} spread={spread(Y):.2f} split-topics={parts(Y)}/{k}')
