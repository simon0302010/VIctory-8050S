# usage: bythread.py <dir> <exe> <base hex> <comm-or-tid substring>...
import sys, bisect, collections, subprocess
d, exe, base = sys.argv[1], sys.argv[2], int(sys.argv[3], 16)
want = sys.argv[4:]
syms = []
for l in subprocess.run(['x86_64-w64-mingw32-nm', '-C', '--defined-only', exe], capture_output=True, text=True).stdout.splitlines():
    p = l.split(' ', 2)
    if len(p) == 3 and p[1] in 'tT': syms.append((int(p[0], 16), p[2]))
syms.sort(); addrs = [a for a, _ in syms]
out = subprocess.run(['perf', 'script', '-i', f'{d}/perf.data', '-F', 'comm,tid,ip,sym,dso'], capture_output=True, text=True).stdout
for w in want:
    c = collections.Counter(); n = 0
    for l in out.splitlines():
        p = l.split()
        if len(p) < 4 or (w != p[0] and w != p[1]): continue
        n += 1
        ip, dso = p[2], p[-1]
        sym = ' '.join(p[3:-1])
        if 'perf-' in dso:
            va = int(ip, 16) - base + 0x140000000
            if 0x140001000 <= va < 0x140c00000:
                sym = 'ours: ' + syms[bisect.bisect_right(addrs, va) - 1][1]
            else: sym = 'title code'
        else:
            sym = dso.strip('()').split('/')[-1] + ': ' + (sym if sym and sym != '[unknown]' else ip)
        c[sym[:110]] += 1
    print(f'== {w}: {n} samples ({n/20/1999*100:.0f}% of a core)')
    for s, k in c.most_common(18): print(f'  {k:6d} {100*k/n:5.1f}%  {s}')
