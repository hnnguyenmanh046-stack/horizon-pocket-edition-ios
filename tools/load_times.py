#!/usr/bin/env python3
"""Is this source's game as quick to load as the release before?

Builds of the game for the desktop, the release before (OLD), this source (NEW) and, given,
the engine this source is built on as its makers released it (ENGINE), each with the game files
its own preparer made, on the same machine, turn by turn:

- start:  from the program's start to its title screen;
- world:  from "open this world" to the player in it with every chunk around them loaded. The
          world is one the same game made in a start that is not counted, so no terrain is made
          in the time measured.

Each is taken ROUNDS times, by turns so that the machine's mood falls on all, and the middle
value is compared. NEW may be slower by SLACK (a shared machine's noise) than what it is held to;
more fails. With ENGINE that is the engine: what the port adds to it must cost nothing, and the
release before is beside it to be read (an engine's own change of pace is its makers'). Without,
it is the release before.

    python3 tools/load_times.py --old-client OLD --old-resources RO --new-client NEW --new-resources RN --xvfb

It says nothing of a phone's own costs (its storage, its GPU's shader compiler): the engine's
work of reading the game files and a world is the same code there.
"""
import argparse
import json
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


def traced(side, home):
    """One more start of each, not counted, with the game's own timeline written
    (HORIZON_STARTUP_TRACE): which named step took how long, on which thread."""
    trace = home / 'startup-trace.jsonl'
    os.environ['HORIZON_STARTUP_TRACE'] = str(trace)
    try:
        one_round(side, home)
    finally:
        del os.environ['HORIZON_STARTUP_TRACE']
    threads, steps, marks = {}, [], []
    for line in trace.read_text(encoding='utf-8', errors='replace').splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if event.get('ph') == 'M':
            threads[event['tid']] = event['args']['name']
        elif event.get('ph') == 'X':
            steps.append(event)
        else:
            marks.append(event)
    log = [line for line in (home / 'client.log').read_text(encoding='utf-8', errors='replace').splitlines()
           if ' ms' in line and 'Horizon:' in line]
    return threads, steps, marks, log


def timeline(name, threads, steps, marks, log):
    print(f'--- {name}: steps of 10 ms and more, in the order they began (ms since the start)')
    for event in sorted(steps, key=lambda e: e['ts']):
        if event['dur'] >= 10_000:
            print(f"{event['ts'] / 1000:9.0f} {event['dur'] / 1000:8.0f} ms  "
                  f"{threads.get(event['tid'], event['tid'])}: {event['name']}")
    print(f'--- {name}: marks')
    for event in sorted(marks, key=lambda e: e.get('ts', 0)):
        print(f"{event.get('ts', 0) / 1000:9.0f}  {event.get('name')}")
    print(f'--- {name}: what the game logged')
    for line in log:
        print('   ', line)


def compare(old, new):
    total = {}
    for side, (_, steps, _, _) in (('old', old), ('new', new)):
        for event in steps:
            total.setdefault(event['name'], {'old': 0, 'new': 0})[side] += event['dur'] / 1000
    print('--- steps whose time differs by 20 ms and more (old, new, ms; all threads added up)')
    for name, t in sorted(total.items(), key=lambda item: item[1]['old'] - item[1]['new']):
        if abs(t['new'] - t['old']) >= 20:
            print(f"{t['old']:8.0f} {t['new']:8.0f} {t['new'] - t['old']:+8.0f}  {name}")


NAMES = {'old': 'release before', 'engine': 'the engine alone', 'new': 'this source'}


def report(times):
    held_to = 'engine' if 'engine' in times else 'old'
    others = [name for name in times if name != 'new']
    lines = ['| | ' + ' | '.join(NAMES[name] for name in others) + ' | this source | '
             + ' | '.join(f'against {NAMES[name]}' for name in others) + ' |',
             '| --- |' + ' --- |' * (2 * len(others) + 1)]
    slower = []
    for what, label in (('start', 'Start to title screen'), ('world', 'Opening a saved world')):
        middle = {name: statistics.median(times[name][what]) for name in times}
        new = middle['new']
        lines.append(f'| {label} | ' + ' | '.join(f'{middle[name]:.2f} s' for name in others)
                     + f' | {new:.2f} s | '
                     + ' | '.join(f'{(new - middle[name]) / middle[name] * 100:+.0f}%' for name in others)
                     + ' |')
        base = middle[held_to]
        if new > base * SLACK[0] and new - base > SLACK[1]:
            slower.append(f'{label}: {base:.2f} s for {NAMES[held_to]}, {new:.2f} s for this source')
    return lines, slower


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for side in ('old', 'new'):
        parser.add_argument(f'--{side}-client', type=Path, required=True)
        parser.add_argument(f'--{side}-resources', type=Path, required=True)
    parser.add_argument('--engine-client', type=Path)
    parser.add_argument('--engine-resources', type=Path)
    parser.add_argument('--rounds', type=int, default=ROUNDS)
    parser.add_argument('--offscreen', action='store_true', help='run the games without a window')
    parser.add_argument('--xvfb', action='store_true', help='run the games under xvfb-run (headless Linux)')
    args = parser.parse_args()
    tf.Game.prefix = ['xvfb-run', '-a'] if args.xvfb else []
    tf.Game.extra = ['--offscreen'] if args.offscreen else []
    sides = {'old': (args.old_client.resolve(), args.old_resources.resolve())}
    if args.engine_client:
        sides['engine'] = (args.engine_client.resolve(), args.engine_resources.resolve())
    sides['new'] = (args.new_client.resolve(), args.new_resources.resolve())
    # Short: Unix socket paths are limited to about 100 bytes.
    with tempfile.TemporaryDirectory(prefix='hlt-', dir='/tmp') as directory:
        times = run(sides, Path(directory), args.rounds)
        try:
            traces = {name: traced(side, Path(directory) / name) for name, side in sides.items()}
            for name, trace in traces.items():
                timeline(name, *trace)
            for name in traces:
                if name != 'new':
                    print(f'=== {NAMES[name]} against this source')
                    compare(traces[name], traces['new'])
        except Exception as error:  # the timeline explains; it does not decide
            print(f'no timeline: {error!r}')
    lines, slower = report(times)
    print('\n'.join(lines))
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        with open(summary, 'a', encoding='utf-8') as out:
            out.write(f'### Loading times (middle of {args.rounds} runs each)\n\n' + '\n'.join(lines) + '\n')
    if slower:
        sys.exit('This source loads slower than it is held to:\n  ' + '\n  '.join(slower))
    print('Loading times passed: this source is no slower than it is held to')


if __name__ == '__main__':
    main()
