#!/usr/bin/env python3
"""Does a player's game folder survive the move to a newer engine?

An app is installed over the one before it: the newer engine opens the worlds and reads the
settings the older one wrote. This plays that with two builds of the game for the desktop, the
release before (OLD) and this source (NEW), each with the game files its own preparer made:

1. old:      OLD makes a world, puts a block and a floor in it, gives its player items, stands
             them on the floor, changes some settings, saves and closes;
2. chunks:   every chunk of every region file OLD wrote reads back;
3. upgrade:  a copy of OLD's whole game folder is opened by NEW: the world is in its list and
             opens, the block is there, the player stands where they stood with the health they
             had, and the items are still theirs once NEW has saved;
4. settings: every setting OLD's files held is in the files NEW wrote, with the value it had;
5. chunks:   every chunk NEW wrote reads back.

    python3 tools/world_upgrade.py --old-client OLD --old-resources RO --new-client NEW --new-resources RN --xvfb

The other way round (a newer world opened by the older game) is not tried: a game does not
promise to read what a later one wrote.
"""
import argparse
import gzip
import json
import math
import shutil
import sys
import tempfile
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import test_friends as tf  # noqa: E402

SECTOR = 4096
WORLD = 'Upgrade'
ITEM = 'minecraft:netherite_ingot'
# Settings the newer game is right to write anew: what names the game's own version or state, not
# a choice of the player's. `file: {keys}`; a file named with None is not compared at all.
OWN = {
    'options.txt': {'version', 'startedCleanly', 'lastServer'},
}
# What a game folder holds that is no settings file.
NOT_SETTINGS = {'saves', 'logs', 'crash-reports', 'screenshots', 'resourcepacks', 'shaderpacks',
                'client.log', 'c.sock', 'horizon-control.json'}


def check_regions(world):
    """Every stored chunk of every region file: inside the file, and its bytes decompress to a
    compound. Returns how many chunks were read."""
    chunks = 0
    files = sorted(world.rglob('*.mca'))
    assert files, f'{world} has no region files'
    for path in files:
        data = path.read_bytes()
        if not data:
            continue
        assert len(data) >= 2 * SECTOR, f'{path}: {len(data)} bytes is shorter than a header'
        for index in range(1024):
            location = int.from_bytes(data[index * 4:index * 4 + 4], 'big')
            if location == 0:
                continue
            offset, sectors = (location >> 8) * SECTOR, (location & 0xFF) * SECTOR
            where = f'{path} chunk {index}'
            assert offset >= 2 * SECTOR and offset + sectors <= len(data), f'{where}: sectors outside the file'
            length = int.from_bytes(data[offset:offset + 4], 'big')
            kind = data[offset + 4]
            assert 1 <= length <= sectors - 4, f'{where}: length {length} in {sectors} bytes'
            payload = data[offset + 5:offset + 4 + length]
            if kind & 0x80:
                x, z = (int(v) for v in path.stem.split('.')[1:3])
                external = path.with_name(f'c.{x * 32 + index % 32}.{z * 32 + index // 32}.mcc')
                assert external.is_file(), f'{where}: {external.name} is missing'
                payload = external.read_bytes()
                kind &= 0x7F
            if kind == 1:
                body = gzip.decompress(payload)
            elif kind == 2:
                body = zlib.decompress(payload)
            elif kind == 3:
                body = payload
            else:
                assert kind == 4, f'{where}: unknown compression {kind}'
                body = None
            assert body is None or (body and body[0] == 10), f'{where}: not a compound'
            chunks += 1
    return chunks


def player_files(world):
    """The world's saved players, unpacked: level.dat (the owner's own, in singleplayer) and
    playerdata/."""
    found = {}
    for path in [world / 'level.dat', *sorted((world / 'playerdata').glob('*.dat')),
                 *sorted((world / 'players' / 'data').glob('*.dat'))]:
        if path.is_file():
            data = path.read_bytes()
            try:
                found[path.relative_to(world).as_posix()] = gzip.decompress(data)
            except OSError:
                found[path.relative_to(world).as_posix()] = data
    return found


def holds_item(world):
    """Whether a saved player of the world holds ITEM."""
    return any(ITEM.encode() in data for data in player_files(world).values())


def settings_of(home):
    """The game folder's settings: {file: {key: value}} for options.txt-like files (`key:value`
    lines) and JSON files (flattened to dotted keys)."""
    def flat(value, prefix, out):
        if isinstance(value, dict):
            for key, inner in value.items():
                flat(inner, f'{prefix}.{key}' if prefix else str(key), out)
        else:
            out[prefix] = value

    found = {}
    for path in sorted(home.iterdir()):
        if path.name in NOT_SETTINGS or not path.is_file():
            continue
        if path.suffix == '.json':
            try:
                values = {}
                flat(json.loads(path.read_text(encoding='utf-8')), '', values)
                found[path.name] = values
            except (ValueError, UnicodeDecodeError):
                continue
        elif path.suffix == '.txt':
            values = {}
            for line in path.read_text(encoding='utf-8', errors='replace').splitlines():
                key, colon, value = line.partition(':')
                if colon and key and ' ' not in key:
                    values[key] = value
            if values:
                found[path.name] = values
    return found


def lost_settings(before, after):
    """What `before` held that `after` lacks or holds otherwise, as lines."""
    lost = []
    for name, values in before.items():
        own = OWN.get(name, set())
        if own is None:
            continue
        if name not in after:
            lost.append(f'{name}: the file is gone')
            continue
        for key, value in values.items():
            if key in own:
                continue
            if key not in after[name]:
                lost.append(f'{name}: {key} (was {value!r}) is gone')
            elif after[name][key] != value:
                lost.append(f'{name}: {key} was {value!r}, is {after[name][key]!r}')
    return lost


def change_options(home):
    """Choices a player made, in the older game's options.txt: none of them a default."""
    path = home / 'options.txt'
    wanted = {'fov': '0.25', 'gamma': '0.75', 'guiScale': '3', 'renderDistance': '7',
              'maxFps': '90', 'bobView': 'false', 'key_key.jump': 'key.keyboard.j'}
    # A game that changed nothing yet has written no options.txt: the choices are then the file.
    lines = path.read_text(encoding='utf-8').splitlines() if path.is_file() else []
    seen = set()
    for i, line in enumerate(lines):
        key = line.partition(':')[0]
        if key in wanted:
            lines[i] = f'{key}:{wanted[key]}'
            seen.add(key)
    if not lines:
        lines = [f'{key}:{value}' for key, value in wanted.items()]
        seen = set(wanted)
    assert seen, f'{path} has none of {sorted(wanted)}'
    path.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return {key: wanted[key] for key in seen}


def open_world(game):
    game(f'open {WORLD} 0 creative')
    return game.wait('status', lambda v: v['player'] is not None and v['pending_chunks'] == 0, 240, 'the world open')


def close_world(game):
    game('disconnect')
    game.wait('screen', lambda v: v['screen'] == 'title', 180, 'the title after closing')


def block_is(game, pos, name, what):
    game.wait(f'getblock {pos[0]} {pos[1]} {pos[2]}', lambda v: v['block'] == name, 60, what)


def tails(games):
    for game in games:
        print(f'--- {game.home.name} log tail ---')
        print('\n'.join(game.stop().splitlines()[-60:]))


class Game(tf.Game):
    """A game in a folder of its own choosing: one that is there already is started in as it is,
    as an app is started again in its player's folder."""

    def __init__(self, side, home):
        client, resources = side
        home.mkdir(parents=True, exist_ok=True)
        self.home = home
        self.endpoint = home / 'c.sock'
        self.endpoint.unlink(missing_ok=True)
        self.log = (home / 'client.log').open('w+')
        self.process = tf.subprocess.Popen(tf.Game.prefix + [
            str(client), '--resources', str(resources), '--render-distance', '2', '--test-terrain',
            '--control-socket', str(self.endpoint), '--timeout-seconds', '900',
        ] + tf.Game.extra, cwd=home,
            env={**tf.game_environment({**tf.os.environ}, home), 'HORIZON_HOME': str(home)},
            stdout=self.log, stderr=tf.subprocess.STDOUT)
        tf.wait_for(self.endpoint, 'screen', lambda v: v['screen'] == 'title', 180, f'{home.name} title')


def run(old, new, work):
    made = []

    def game(side, home):
        g = Game(side, home)
        made.append(g)
        return g

    try:
        # 1. The older game: a start writes its settings, the player changes some, then plays.
        old_home = work / 'old'
        game(old, old_home).stop()
        chosen = change_options(old_home)
        play = game(old, old_home)
        open_world(play)
        at = play('status')['player']['position']
        floor = [math.floor(at[0]) + 3, math.floor(at[1]) + 6, math.floor(at[2])]
        block = [floor[0], floor[1] + 3, floor[2] + 2]
        play(f'command setblock {floor[0]} {floor[1]} {floor[2]} minecraft:glass')
        play(f'command setblock {block[0]} {block[1]} {block[2]} minecraft:diamond_block')
        block_is(play, block, 'minecraft:diamond_block', 'the block the older game placed')
        play(f'command give @s {ITEM} 7')
        stand = [floor[0] + 0.5, floor[1] + 1, floor[2] + 0.5]
        play(f'command tp @s {stand[0]} {stand[1]} {stand[2]}')
        stood = play.wait('status', lambda v: abs(v['player']['position'][0] - stand[0]) < 0.01
                          and abs(v['player']['position'][1] - stand[1]) < 0.01
                          and v['player']['on_ground'], 60, 'the player on the floor')['player']
        play('save')
        close_world(play)
        play.stop()
        saved = old_home / 'saves' / WORLD
        assert holds_item(saved), f'the older game saved no {ITEM}: {sorted(player_files(saved))}'
        before = settings_of(old_home)
        for key, value in chosen.items():
            assert before['options.txt'].get(key) == value,                 f'the older game did not keep {key}:{value} (has {before["options.txt"].get(key)!r})'
        print(f'old: a world with a block, a player with 7 {ITEM} at {stood["position"]}, '
              f'{len(chosen)} settings chosen ({", ".join(sorted(before))})', flush=True)
        # 2. What it wrote reads back.
        print(f'chunks: the older game wrote {check_regions(saved)} chunks, all readable', flush=True)
        # 3. The newer game, started in a copy of the whole folder.
        new_home = work / 'new'
        shutil.copytree(old_home, new_home,
                        ignore=shutil.ignore_patterns('session.lock', 'c.sock', 'client.log', 'logs'))
        final = game(new, new_home)
        opened = open_world(final)['player']
        block_is(final, block, 'minecraft:diamond_block', 'the block of the older game in the newer one')
        block_is(final, floor, 'minecraft:glass', 'the floor of the older game in the newer one')
        for axis in range(3):
            assert abs(opened['position'][axis] - stood['position'][axis]) < 0.05,                 f'the player stood at {stood["position"]} and is at {opened["position"]}'
        assert abs(opened['health'] - stood['health']) < 0.01,             f'the player had {stood["health"]} health and has {opened["health"]}'
        final('save')
        close_world(final)
        final.stop()
        after_world = new_home / 'saves' / WORLD
        assert holds_item(after_world), f'the newer game saved the player without their {ITEM}'
        print(f'upgrade: the newer game opened it: the blocks, the player at {opened["position"]} '
              f'with {opened["health"]} health, and their {ITEM}', flush=True)
        # 4. The settings.
        lost = lost_settings(before, settings_of(new_home))
        assert not lost, 'settings of the older game the newer one lost:\n  ' + '\n  '.join(lost)
        print(f'settings: every setting of {", ".join(sorted(before))} is as it was', flush=True)
        # 5. What the newer game wrote reads back.
        print(f'chunks: the newer game wrote {check_regions(after_world)} chunks, all readable', flush=True)
    except BaseException:
        tails(made)
        raise
    finally:
        for g in made:
            g.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for side in ('old', 'new'):
        parser.add_argument(f'--{side}-client', type=Path, required=True)
        parser.add_argument(f'--{side}-resources', type=Path, required=True)
    parser.add_argument('--offscreen', action='store_true', help='run the games without a window')
    parser.add_argument('--xvfb', action='store_true', help='run the games under xvfb-run (headless Linux)')
    args = parser.parse_args()
    tf.Game.prefix = ['xvfb-run', '-a'] if args.xvfb else []
    tf.Game.extra = ['--offscreen'] if args.offscreen else []
    old = (args.old_client.resolve(), args.old_resources.resolve())
    new = (args.new_client.resolve(), args.new_resources.resolve())
    # Short: Unix socket paths are limited to about 100 bytes.
    with tempfile.TemporaryDirectory(prefix='hup-', dir='/tmp') as directory:
        run(old, new, Path(directory))
    print('World upgrade passed: the world, its player and the settings came through')


if __name__ == '__main__':
    main()
