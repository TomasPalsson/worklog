import json
d=json.load(open('/Users/tomas/.claude/jobs/491be4f8/tmp/live30.json'))
P=d['points'];T={t['id']:t['name'] for t in d['topics']}
print(sorted(P[0].keys()))
xk,yk=('x','y') if 'x' in P[0] else ('cx','cy')
tk='topic' if 'topic' in P[0] else 'topic_id'
tot=0;tsame=0
for tid,name in T.items():
  pts=[p for p in P if p[tk]==tid]
  if not pts: continue
  same=0
  for p in pts:
    nn=min((q for q in P if q is not p),key=lambda q:(q[xk]-p[xk])**2+(q[yk]-p[yk])**2)
    same+= nn[tk]==tid
  tot+=len(pts);tsame+=same
  print(f"{name:32s} dots={len(pts):3d} nearest-dot-same-topic={round(100*same/len(pts))}%")
print("overall", round(100*tsame/tot), "%")
