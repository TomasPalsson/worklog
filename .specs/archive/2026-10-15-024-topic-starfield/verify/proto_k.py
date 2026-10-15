import json, sqlite3, numpy as np
from sklearn.cluster import AgglomerativeClustering
from sklearn.metrics import silhouette_score
from sklearn.manifold import TSNE

d = json.load(open('/Users/tomas/.claude/jobs/491be4f8/tmp/live30.json'))
ids = [p['block_id'] for p in d['points']]
live_topic = {p['block_id']: p['topic_id'] for p in d['points']}
db = sqlite3.connect('file:/Users/tomas/.claude/jobs/491be4f8/tmp/wl/worklog.db?mode=ro', uri=True)
rows = dict(db.execute(f"select block_id, vec from block_embeddings where block_id in ({','.join(map(str, ids))})").fetchall())
ids = [i for i in ids if i in rows]
X = np.stack([np.frombuffer(rows[i], dtype='<f4') for i in ids]).astype(np.float64)
X /= np.linalg.norm(X, axis=1, keepdims=True)
X = X - X.mean(0); X /= np.linalg.norm(X, axis=1, keepdims=True)
print('n', len(ids), 'dim', X.shape[1])
best = None
for k in range(4, 25):
    lab = AgglomerativeClustering(n_clusters=k, metric='cosine', linkage='average').fit_predict(X)
    s = silhouette_score(X, lab, metric='cosine')
    sizes = sorted(np.bincount(lab), reverse=True)
    print(f'k={k:2d} silhouette={s:.3f} largest={sizes[0]} singletons={sum(1 for z in sizes if z == 1)} small(<3)={sum(1 for z in sizes if z < 3)}')

def agree(Y, lab):
    from sklearn.neighbors import NearestNeighbors
    nn = NearestNeighbors(n_neighbors=2).fit(Y)
    _, idx = nn.kneighbors(Y)
    return (lab[idx[:, 1]] == lab).mean()

def spread(Y, lab):
    # mean within-topic distance / mean overall distance (lower = tighter islands)
    from scipy.spatial.distance import pdist, squareform
    D = squareform(pdist(Y)); m = D.mean()
    w = [D[np.ix_(lab == c, lab == c)].mean() for c in set(lab) if (lab == c).sum() > 1]
    return float(np.mean(w) / m)

for k in (10, 14, 18):
    lab = AgglomerativeClustering(n_clusters=k, metric='cosine', linkage='average').fit_predict(X)
    for dims in (2, 3):
        Y = TSNE(n_components=dims, perplexity=30, metric='cosine', init='pca', random_state=0).fit_transform(X)
        # cluster pull: move each point toward its topic centroid
        for pull in (0.0, 0.35, 0.6):
            C = np.stack([Y[lab == c].mean(0) for c in range(k)])
            Z = Y + pull * (C[lab] - Y)
            print(f'k={k} dims={dims} pull={pull}: nearest-same={agree(Z, lab):.2f} spread={spread(Z, lab):.2f}')
