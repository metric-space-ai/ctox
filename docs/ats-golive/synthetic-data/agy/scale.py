#!/usr/bin/env python3
"""Scale: generate many rich batches via agy (gemini-3.5-flash) in parallel,
then inject each onto ninja. Generation (agy) is the slow part -> parallelised;
injection runs on ninja via ssh."""
import json, os, re, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor, as_completed

WS = "/tmp/agy-synth-ws"
AGY = os.path.expanduser("~/.local/bin/agy")
KEY = f"{WS}/ninja_key"
HOST = "ctox@217.182.134.181"
MODEL = "Gemini 3.5 Flash (Medium)"
PROMPT_BASE = open(f"{WS}/genprompt.txt").read()

THEMES = [
    ("Logistik & Lager (Staplerfahrer, Kommissionierer, Lagerist)", "Hamburg/Bremen/Niedersachsen"),
    ("Pflege & Gesundheit (Pflegefachkraft, Altenpfleger, MFA)", "München/Augsburg"),
    ("Handwerk SHK & Elektro (Anlagenmechaniker, Elektroniker)", "Köln/Düsseldorf"),
    ("Produktion & Industrie (Maschinenbediener, Industriemechaniker)", "Stuttgart/Mannheim"),
    ("Office & Verwaltung (Bürokaufleute, Sachbearbeiter, Buchhaltung)", "Frankfurt/Wiesbaden"),
    ("IT & Engineering (Fachinformatiker, Systemadministrator)", "Berlin/Potsdam"),
    ("Gastronomie & Hotel (Koch, Servicekraft, Rezeption)", "Düsseldorf/Köln"),
    ("Bau & Tiefbau (Maurer, Bauhelfer, Polier)", "Leipzig/Dresden"),
    ("Transport & Fahrdienst (Berufskraftfahrer CE, Busfahrer)", "Dortmund/Essen"),
    ("Metall & Schweißen (Schweißer, Schlosser, Zerspaner)", "Bochum/Duisburg"),
    ("Einzelhandel & Verkauf (Verkäufer, Filialleiter, Kassierer)", "Hannover/Braunschweig"),
    ("Reinigung & Facility (Gebäudereiniger, Hausmeister)", "Nürnberg/Fürth"),
]

def gen_batch(i):
    sector, region = THEMES[i % len(THEMES)]
    bid = f"agy-b{i:03d}"
    prompt = (PROMPT_BASE + f"\n\nBATCH_ID = {bid}\nTHEME = {sector}, Region {region}."
              f"\nUse names and companies DIFFERENT from a typical batch; vary first+last names widely "
              f"(German and migrant-background), no two candidates alike. Batch index {i}.")
    try:
        p = subprocess.run([AGY, "--model", MODEL, "--print", prompt],
                           stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=240)
        t = p.stdout.strip()
        t = re.sub(r'^```(json)?', '', t); t = re.sub(r'```$', '', t.strip())
        d = json.loads(t)
        d["batch"] = bid
        n = len(d.get("candidates", []))
        if n == 0:
            return (bid, 0, "no candidates")
        path = f"{WS}/{bid}.json"
        json.dump(d, open(path, "w"), ensure_ascii=False)
        return (bid, n, path)
    except Exception as e:
        return (bid, 0, f"ERR {str(e)[:80]}")

def inject(path):
    bid = os.path.basename(path).replace(".json", "")
    subprocess.run(["scp", "-i", KEY, "-P", "22012", "-o", "BatchMode=yes", path, f"{HOST}:~/"],
                   capture_output=True, timeout=60)
    r = subprocess.run(["ssh", "-i", KEY, "-p", "22012", "-o", "BatchMode=yes", HOST,
                        f"bash -l -c 'export PATH=$HOME/.local/bin:$PATH; python3 ~/ats_inject.py ~/{bid}.json'"],
                       capture_output=True, text=True, timeout=300)
    try:
        return json.loads(r.stdout.strip().splitlines()[-1])["counts"]
    except Exception:
        return {"_err": r.stdout[-200:] + r.stderr[-200:]}

def main():
    n_batches = int(sys.argv[1]) if len(sys.argv) > 1 else 6
    conc = int(sys.argv[2]) if len(sys.argv) > 2 else 3
    t0 = time.time()
    gens = []
    with ThreadPoolExecutor(max_workers=conc) as ex:
        futs = {ex.submit(gen_batch, i): i for i in range(n_batches)}
        for f in as_completed(futs):
            bid, n, info = f.result()
            print(f"[gen {int(time.time()-t0):4d}s] {bid}: {n} cand  {'' if n else info}", flush=True)
            if n:
                gens.append(info)
    print(f"--- generated {len(gens)}/{n_batches} valid batches in {int(time.time()-t0)}s; injecting ---", flush=True)
    tot = {}
    for path in sorted(gens):
        c = inject(path)
        for k, v in (c or {}).items():
            if isinstance(v, int):
                tot[k] = tot.get(k, 0) + v
        print(f"[inject] {os.path.basename(path)}: {c}", flush=True)
    print("=== TOTAL ===", json.dumps(tot), flush=True)

if __name__ == "__main__":
    main()
