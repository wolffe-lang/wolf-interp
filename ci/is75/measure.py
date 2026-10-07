import json,subprocess,sys
from datetime import datetime
def t(s): return datetime.strptime(s,"%Y-%m-%dT%H:%M:%SZ")
print("run\tevent\tsha\tjobs_ran\tjobs_listed\tslot_h\tspan_h\twidest_min\tmacos_jobs\tlong_jobs(>20m)")
for r in sys.argv[1:]:
    d=json.loads(subprocess.check_output(["gh","run","view",r,"-R","wolffe-lang/wolf-interp","--json","jobs,event,headSha"]))
    tot=0;wid=0;n=0;st=[];en=[];mac=0;lng=0
    for j in d["jobs"]:
        if not j.get("startedAt") or not j.get("completedAt") or j["completedAt"].startswith("0001") or j["conclusion"]=="skipped": continue
        m=(t(j["completedAt"])-t(j["startedAt"])).total_seconds()/60
        tot+=m;n+=1;wid=max(wid,m);st.append(t(j["startedAt"]));en.append(t(j["completedAt"]))
        if "macos" in j["name"]: mac+=1
        if m>20: lng+=1
    span=(max(en)-min(st)).total_seconds()/3600 if st else 0
    print(f'{r}\t{d["event"]}\t{d["headSha"][:7]}\t{n}\t{len(d["jobs"])}\t{tot/60:.2f}\t{span:.2f}\t{wid:.1f}\t{mac}\t{lng}')
