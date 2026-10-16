import sys, json, importlib
sys.path.insert(0, '/home/muth/kiln-brand/final/src')
O = '/home/muth/kiln-brand/final'
apps = sys.argv[1:] or ['write', 'grid', 'deck', 'notes', 'home', 'phone']
jobs = []
for a in apps:
    m = importlib.import_module('mock_' + a)
    for th in ('light', 'dark'):
        html = m.build(th)
        p = f'{O}/mockups/src/{a}-{th}.html'
        open(p, 'w').write(html)
        w, h = (390, 844) if a == 'phone' else (1440, 900)
        jobs.append({'in': p, 'out': f'{O}/mockups/{a}-{th}.png', 'w': w, 'h': h, 'scale': 2 if a == 'phone' else 1, 'transparent': False})
json.dump(jobs, open(f'{O}/work/mock-jobs.json', 'w'))
