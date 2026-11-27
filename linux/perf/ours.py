# usage: analyze.py <perf dir> <exe>   (prints our functions by samples)
import sys, bisect, collections, subprocess, re
d, exe = sys.argv[1], sys.argv[2]
maps = open(f'{d}/maps.txt').read().split()
base = None
for i, w in enumerate(maps):
    if re.fullmatch(r'[0-9a-f]+-[0-9a-f]+', w) and maps[i+2] == '00000000':
        base = int(w.split('-')[0], 16); break
syms = []
for l in subprocess.run(['x86_64-w64-mingw32-nm', '-C', '--defined-only', exe], capture_output=True, text=True).stdout.splitlines():
    p = l.split(' ', 2)
    if len(p) == 3 and p[1] in 'tT': syms.append((int(p[0], 16), p[2]))
syms.sort(); addrs = [a for a, _ in syms]
out = subprocess.run(['perf', 'script', '-i', f'{d}/perf.data', '-F', 'comm,ip,dso'], capture_output=True, text=True).stdout
c = collections.Counter(); comms = collections.Counter(); total = ours = 0
for l in out.splitlines():
    p = l.split()
    if len(p) < 3: continue
    total += 1
    try: ip = int(p[-2], 16)
    except ValueError: continue
    va = ip - base + 0x140000000
    if 0x140001000 <= va < 0x140c00000 and 'perf-' in p[-1]:
        ours += 1; i = bisect.bisect_right(addrs, va) - 1; c[syms[i][1]] += 1
    comms[p[0]] += 1
print('base', hex(base), 'samples', total, 'ours', ours)
print('by thread name:', comms.most_common(8))
for n, k in c.most_common(int(sys.argv[3]) if len(sys.argv) > 3 else 40):
    print(f'{k:6d} {100*k/max(ours,1):5.1f}%  {n[:140]}')
