import re, html
D='/Users/tomas/Desktop/Projects/worklog/.specs/020-tempo-estimate-per-person'
sprite=open(D+'/assets/art/sprite.svg').read().replace('style="display:none"','style="display:none" aria-hidden="true"')
def fmt(m):
    h,mm=divmod(m,60)
    return f"{h}h {mm}m" if h and mm else (f"{h}h" if h else f"{mm}m")
COL={'TP':'var(--p1)','JG':'var(--p2)','SB':'var(--p3)','HK':'var(--p4)'}
INK={'TP':'var(--p1-ink)','JG':'var(--p2-ink)','SB':'var(--p3-ink)','HK':'var(--p4-ink)'}
def icon(i,cls='st'): return f'<svg class="{cls}" aria-hidden="true"><use href="#{i}"/></svg>'
DAYS=['Mon 6','Tue 7','Wed 8','Thu 9']
def burn(est,people,pending,ticket,uid):
    n=len(DAYS); W,Hh=260,104; x0,x1,y0,y1=22,236,12,84
    cum=[]  # per person cumulative per day
    for _,_,d in people:
        c=[];t=0
        for v in d: t+=v; c.append(t)
        cum.append(c)
    tot=[sum(c[i] for c in cum) for i in range(n)]
    top=max(est,tot[-1]+pending)*1.08
    X=lambda i: x0+(x1-x0)*i/(n-1)
    Y=lambda m: y1-(y1-y0)*m/top
    parts=[]
    ey=Y(est)
    parts.append(f'<rect class="over-band" x="{x0}" y="{y0-4}" width="{x1-x0}" height="{ey-y0+4:.1f}"/>' if tot[-1]+pending>est else '')
    parts.append(f'<line class="grid-l" x1="{x0}" x2="{x1}" y1="{y1}" y2="{y1}"/>')
    base=[0]*n
    for k,(ini,_,_) in enumerate(people):
        upper=[base[i]+cum[k][i] for i in range(n)]
        pts=' '.join(f'{X(i):.1f},{Y(upper[i]):.1f}' for i in range(n))
        back=' '.join(f'{X(i):.1f},{Y(base[i]):.1f}' for i in reversed(range(n)))
        parts.append(f'<polygon class="area" style="fill:{COL[ini]}" points="{pts} {back}"/>')
        parts.append(f'<polyline class="edge" style="stroke:{COL[ini]}" points="{pts}"/>')
        base=upper
    if pending:
        parts.append(f'<line class="pend-line" x1="{X(n-1):.1f}" y1="{Y(tot[-1]):.1f}" x2="{X(n-1):.1f}" y2="{Y(tot[-1]+pending):.1f}"/><circle class="pend-dot" cx="{X(n-1):.1f}" cy="{Y(tot[-1]+pending):.1f}" r="3"/>')
    parts.append(f'<line class="est-line" x1="{x0}" x2="{x1}" y1="{ey:.1f}" y2="{ey:.1f}"/><text class="est-t" x="{x0-4}" y="{ey+3:.1f}" text-anchor="end">{fmt(est)}</text>')
    for i,d in enumerate(DAYS):
        parts.append(f'<text class="axis-t{" today" if i==n-1 else ""}" x="{X(i):.1f}" y="{y1+14}" text-anchor="middle">{d.split()[0] if i<n-1 else "Today"}</text>')
    parts.append(f'<line class="cross" x1="0" x2="0" y1="{y0-4}" y2="{y1}"/>')
    parts.append(f'<rect class="hit" x="{x0-12}" y="0" width="{x1-x0+24}" height="{y1+4}"/>')
    import json
    data={'x':[round(X(i),1) for i in range(n)],'days':DAYS,'people':[[nm,COL[ini],c] for (ini,nm,_),c in zip(people,cum)],'pending':pending,'est':est}
    rows=''.join(f'<tr><td>{d}</td>'+''.join(f'<td>{fmt(c[i])}</td>' for c in cum)+f'<td>{fmt(tot[i])}</td></tr>' for i,d in enumerate(DAYS))
    if pending: rows+=f'<tr><td>After sync</td>'+''.join(f'<td>{fmt(c[-1]+(pending if k==0 else 0))}</td>' for k,c in enumerate(cum))+f'<td>{fmt(tot[-1]+pending)}</td></tr>'
    hdr=''.join(f'<th>{html.escape(nm)}</th>' for _,nm,_ in people)
    label=f"Running total of hours on {ticket}: "+', '.join(f'{d} {fmt(t)}' for d,t in zip(DAYS,tot))+f'; estimate {fmt(est)}'
    return f'''<figure class="burn" data-burn='{json.dumps(data)}'>
  <figcaption class="burn-title"><span>Running total on this ticket</span></figcaption>
  <svg viewBox="0 0 {W} {Hh}" role="group" aria-roledescription="chart" tabindex="0" aria-label="{html.escape(label)}">{''.join(parts)}</svg>
  <div class="tip" aria-hidden="true"></div>
  <details class="tbl"><summary>Show as table</summary><table><thead><tr><th>Total by end of</th>{hdr}<th>Total</th></tr></thead><tbody>{rows}</tbody></table></details>
</figure>'''
def progress(est,people,pending=0,ticket='',uid=''):
    flat=[(ini,n,sum(d)) for ini,n,d in people]
    used=sum(m for _,_,m in flat); total=used+pending
    scale=max(est,total); left=est-total
    if total>est: tone,ic,word='over','state-over',f'{fmt(total-est)} over'
    elif total>=0.8*est: tone,ic,word='low','state-running-low',f'{fmt(left)} left'
    else: tone,ic,word='ok','state-on-track',f'{fmt(left)} left'
    segs=''.join(f'<i style="width:{m/scale*100:.2f}%;background:{COL[ini]}"></i>' for ini,_,m in flat)
    if pending: segs+=f'<i class="pend" style="width:{pending/scale*100:.2f}%"></i>'
    flag=est/scale*100
    end=' data-end' if flag>90 else ''
    over=f'<span class="overzone" style="left:{flag:.2f}%"></span>' if total>est else ''
    head_used=f'<b>{fmt(total)}</b> of {fmt(est)}'+(' <span class="after">once synced</span>' if pending else '')
    legend=''.join(f'<li><span class="ini" style="background:{COL[ini]};color:{INK[ini]}">{ini}</span>{html.escape(n)} <b>{fmt(m)}</b></li>' for ini,n,m in flat)
    if pending: legend+=f'<li>{icon("person-pending","pd")}This block, not in Tempo yet <b>+{fmt(pending)}</b></li>'
    vt=f"{fmt(total)} of {fmt(est)} estimate"+(" including this block" if pending else "")+f", {word}"
    return f'''<div class="est" data-tone="{tone}"><div class="est-grid">
 <div class="est-main">
  <div class="est-head"><span class="est-used">{head_used}</span><span class="est-state">{icon(ic)}{word}</span></div>
  <div class="bar" role="meter" aria-label="Hours on {ticket} against its estimate" aria-valuemin="0" aria-valuemax="{est*60}" aria-valuenow="{total*60}" aria-valuetext="{vt}">
    {over}<span class="fill">{segs}</span>
    <span class="flag"{end} style="left:{flag:.2f}%">{icon("est-marker","fl")}<span class="flag-l">{fmt(est)}</span></span>
  </div>
  <ul class="legend">{legend}</ul>
 </div>
 {burn(est,people,pending,ticket,uid)}
</div></div>'''
def block(rng,dur,desc,key,summ,inner,synced=False,path='~/Desktop/Work/genai-invoices',state=''):
    syn='<span class="pill synced-tag">✓ synced</span>' if synced else ''
    st=f'<p class="state" data-meta-ok>{state}</p>' if state else ''
    return f'''<div class="slot">{st}<article class="block" aria-label="{rng} · {key} · {dur}">
  <div class="block-time"><span class="range">{rng}</span><span class="duration">{dur}</span></div>
  <div class="block-body">
    <div class="block-description">{desc}</div>
    <div class="block-title-row"><span class="ticket-chip"><span class="key">{key}</span><span class="summary">{summ}</span></span>{syn}</div>
    {inner}
    <div class="block-meta"><span class="dir">{path}</span><span>12 events</span><span>2 commits</span></div>
  </div>
</article></div>'''
G1897=[('TP','You',[30,60,30,0]),('JG','Jón Geir',[60,30,60,0])]
blocks=[
 block('08:30–09:15','45m','Reviewed Jón\'s PR on the summary cache keys.','GENAI-1902','Prompt caching for the summary endpoint',
   progress(240,[('TP','You',[0,30,0,45]),('JG','Jón Geir',[45,60,30,0])],0,'GENAI-1902'),synced=True,state='synced · running low'),
 block('09:30–10:30','1h','Added retry with backoff around the Tempo POST.','GENAI-1911','Retry policy for Tempo sync timeouts',
   progress(360,[('TP','You',[0,0,60,0])],60,'GENAI-1911'),path='~/Desktop/Work/worklog',state='not synced · on track · only you'),
 block('13:00–14:00','1h','Wired the invoice parser to the new Bedrock model and fixed the date parsing.','GENAI-1897','Hook the invoice parser up to the new Bedrock model',
   progress(240,G1897,60,'GENAI-1897'),state='not synced · this block pushes the ticket over'),
 block('14:00–14:45','45m','Pairing on parser edge cases with Sigrún and Halldór.','GENAI-1920','Parser edge cases: credit notes and multi-currency',
   progress(480,[('TP','You',[15,30,0,45]),('JG','Jón Geir Friðbjörnsson',[60,60,60,0]),('SB','Sigrún Björk',[0,45,60,0]),('HK','Halldór Kári',[0,0,0,45])],0,'GENAI-1920'),synced=True,state='synced · four people'),
 block('15:00–15:45','45m','Deleted the three old export buckets after checking nothing reads them.','GENAI-1880','Clean up old S3 export buckets',
   '<p class="est-none">No estimate on this ticket yet. Set “Original estimate” on GENAI-1880 in Jira and the bar shows up here.</p>',synced=True,state='ticket has no estimate'),
 block('16:00–16:30','30m','Answered RL\'s questions on the parser rollout.','GENAI-1897','Hook the invoice parser up to the new Bedrock model',
   '<div class="est" aria-busy="true"><div class="est-head"><span class="est-used muted">Loading hours from Tempo…</span></div><div class="bar skel"></div></div>',state='loading'),
 block('16:30–17:00','30m','Wrote the rollout note for the parser.','GENAI-1897','Hook the invoice parser up to the new Bedrock model',
   '<div class="est" data-tone="warn"><p class="est-err" role="alert">Couldn\'t load hours for GENAI-1897 — Tempo didn\'t answer. Your blocks are safe. <button type="button" class="link">Try again</button></p></div>',state='Tempo failed'),
]
css=open('/Users/tomas/Desktop/Projects/worklog/.specs/020-tempo-estimate-per-person/.vary/build/v3.css').read()
page=f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Block estimate bar</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&family=Geist+Mono:wght@400;500;600&display=swap" rel="stylesheet">
<style>{css}</style>
</head>
<body>
{sprite}
<main>
  <header class="day-head">
    <div>
      <p class="eyebrow">Thursday</p>
      <h1>9 October</h1>
    </div>
    <div class="day-meta">
      <span><b>4h 15m</b> worked</span>
      <span class="muted">Tempo numbers from 09:42</span>
      <button class="theme" type="button" onclick="const r=document.documentElement;r.dataset.theme=r.dataset.theme==='dark'?'light':'dark'">Light / dark</button>
    </div>
  </header>
  <section class="list" aria-label="Blocks">
    {''.join(blocks)}
  </section>
</main>
<script>@@JS@@</script>
</body>
</html>
'''
page=page.replace("@@JS@@",open('/Users/tomas/Desktop/Projects/worklog/.specs/020-tempo-estimate-per-person/.vary/build/tip.js').read())
open(D+'/mock.html','w').write(page)
