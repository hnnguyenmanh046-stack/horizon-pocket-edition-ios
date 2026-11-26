#!/usr/bin/env python3
"""Is this source's game as quick to load as the release before?

Two builds of the game for the desktop, the release before (OLD) and this source (NEW), each
with the game files its own preparer made, on the same machine, turn by turn:

- start:  from the program's start to its title screen;
- world:  from "open this world" to the player in it with every chunk around them loaded. The
          world is one the same game made in a start that is not counted, so no terrain is made
          in the time measured.

Each is taken ROUNDS times, OLD and NEW by turns so that the machine's mood falls on both, and
the middle value is compared. NEW may be slower by SLACK (a shared machine's noise); more fails.

    python3 tools/load_times.py --old-client OLD --old-resources RO --new-client NEW --new-resources RN --xvfb

It says nothing of a phone's own costs (its storage, its GPU's shader compiler): the engine's
work of reading the game files and a world is the same code there.
"""
import argparse
import os
import statistics
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_friends as tf  # noqa: E402
import world_upgrade as wu  # noqa: E402

ROUNDS = 7
# NEW's middle value may be this much over OLD's: a tenth, and no less than half a second.
SLACK = (1.10, 0.5)


def timed_start(side, home):
    began = time.monotonic()
    game = wu.Game(side, home)
    return game, time.monotonic() - began


def timed_world(game):
    began = time.monotonic()
    wu.open_world(game)
    return time.monotonic() - began


def one_round(side, home):
    game, start = timed_start(side, home)
    try:
        world = timed_world(game)
        wu.close_world(game)
    finally:
        game.stop()
    return start, world


def run(sides, work, rounds):
    times = {name: {'start': [], 'world': []} for name in sides}
    # Not counted: the world is made, and the game writes what it writes at a first start.
    for name, side in sides.items():
        one_round(side, work / name)
    for turn in range(rounds):
        for name, side in sides.items():
            start, world = one_round(side, work / name)
            times[name]['start'].append(start)
            times[name]['world'].append(world)
            print(f'round {turn + 1} {name}: start {start:.2f} s, world {world:.2f} s', flush=True)
    return times


def report(times):
    lines = ['| | release before | this source | difference |', '| --- | --- | --- | --- |']
    slower = []
    for what, label in (('start', 'Start to title screen'), ('world', 'Opening a saved world')):
        old = statistics.median(times['old'][what])
        new = statistics.median(times['new'][what])
        lines.append(f'| {label} | {old:.2f} s | {new:.2f} s | {(new - old) / old * 100:+.0f}% |')
        if new > old * SLACK[0] and new - old > SLACK[1]:
            slower.append(f'{label}: {old:.2f} s before, {new:.2f} s now')
    return lines, slower


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for side in ('old', 'new'):
        parser.add_argument(f'--{side}-client', type=Path, required=True)
        parser.add_argument(f'--{side}-resources', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=ROUNDS)
    parser.add_argument('--offscreen', action='store_true', help='run the games without a window')
    parser.add_argument('--xvfb', action='store_true', help='run the games under xvfb-run (headless Linux)')
    args = parser.parse_args()
    tf.Game.prefix = ['xvfb-run', '-a'] if args.xvfb else []
    tf.Game.extra = ['--offscreen'] if args.offscreen else []
    sides = {
        'old': (args.old_client.resolve(), args.old_resources.resolve()),
        'new': (args.new_client.resolve(), args.new_resources.resolve()),
    }
    # Short: Unix socket paths are limited to about 100 bytes.
    with tempfile.TemporaryDirectory(prefix='hlt-', dir='/tmp') as directory:
        times = run(sides, Path(directory), args.rounds)
    lines, slower = report(times)
    print('\n'.join(lines))
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        with open(summary, 'a', encoding='utf-8') as out:
            out.write(f'### Loading times (middle of {args.rounds} runs each)\n\n' + '\n'.join(lines) + '\n')
    if slower:
        sys.exit('This source loads slower than the release before:\n  ' + '\n  '.join(slower))
    print('Loading times passed: this source is no slower than the release before')


if __name__ == '__main__':
    main()
