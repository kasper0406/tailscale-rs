import os,sys,subprocess,json,itertools,time
from pathlib import Path
binary,label=sys.argv[1:3]
out=Path(os.environ.get('RESULTS_JSONL', 'tcp-workaround-30-results.jsonl'))
rows=[(25,b,q,w,n,d,15) for b,q,w,n,d in itertools.product((150,250),(64,128,256),(256,1024),(1,8),('down','up'))]
rows += [(r,0,0,w,n,d,30) for (r,w),n,d in itertools.product(((0,256),(40,4096)),(1,8),('down','up'))]
extra = [(0,0,0,4096,n,d,30) for n,d in itertools.product((1,8),('down','up'))]
rows += extra
for i,(r,b,q,w,n,d,secs) in enumerate(rows):
    env=dict(os.environ,RTT_MS=str(r),BW_MBPS=str(b),QUEUE_KB=str(q),TCP_BUF_KB=str(w),LATENCY='1')
    t=time.time()
    p=subprocess.run(['nice','-n','10',binary,str(secs),str(n),d],env=env,capture_output=True,text=True,timeout=secs+600)
    row=dict(label=label,rtt=r,bw=b,queue=q,window=w,flows=n,direction=d,seconds=secs,rc=p.returncode,stdout=p.stdout,stderr=p.stderr,wall=time.time()-t)
    for field in p.stdout.split():
        if '=' in field:
            k,v=field.split('=',1)
            try: row[k]=float(v)
            except ValueError: pass
    with out.open('a') as f:f.write(json.dumps(row)+'\n')
    print(f'{label} {i+1}/{len(rows)} RTT={r} BW={b} Q={q} W={w} N={n} {d}: '+p.stdout.strip(),flush=True)
    if p.returncode: raise SystemExit(p.stderr)
