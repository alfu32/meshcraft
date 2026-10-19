#!/usr/bin/env python3
"""Exercise an isolated X11 viewport and check retained process memory.

Requires xdotool and Linux /proc. Use a dedicated Xvfb test window, not a user's
active document. Pointer coordinates target the 1280x800 default test layout.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pid', required=True, type=int)
    parser.add_argument('--window', required=True)
    parser.add_argument('--display', required=True)
    parser.add_argument('--updates', type=int, default=600)
    parser.add_argument('--delay', type=float, default=0.06)
    parser.add_argument('--max-growth-mib', type=float, default=64.)
    args = parser.parse_args()
    if args.updates < 80 or args.delay <= 0 or args.max_growth_mib <= 0:
        parser.error('use at least 80 updates, a positive delay and a positive growth bound')
    env = dict(os.environ, DISPLAY=args.display)

    def xdo(*command):
        subprocess.run(['xdotool', *map(str, command)], env=env, check=True)

    def sample(step):
        status = dict(line.split(':', 1) for line in
                      Path(f'/proc/{args.pid}/status').read_text().splitlines() if ':' in line)
        memory = dict(line.split(':', 1) for line in
                      Path('/proc/meminfo').read_text().splitlines() if ':' in line)
        rss = int(status['VmRSS'].split()[0])
        print(json.dumps({'updates': step, 'rss_kib': rss,
                          'system_shared_kib': int(memory['Shmem'].split()[0])}), flush=True)
        return rss

    xdo('windowfocus', args.window)
    xdo('mousemove', '--window', args.window, 500, 500)
    time.sleep(1)
    sample(0)
    warm_rss = None
    for step in range(1, args.updates + 1):
        xdo('mousemove', '--window', args.window,
            330 + (step * 17) % 350, 360 + (step * 13) % 300)
        time.sleep(args.delay)
        if step % 40 == 0:
            rss = sample(step)
            if warm_rss is None:
                warm_rss = rss
    time.sleep(1)
    final_rss = sample(args.updates)
    growth_mib = (final_rss - warm_rss) / 1024.
    print(json.dumps({'growth_after_warmup_mib': growth_mib,
                      'limit_mib': args.max_growth_mib}), flush=True)
    return 0 if growth_mib <= args.max_growth_mib else 1


if __name__ == '__main__':
    raise SystemExit(main())
