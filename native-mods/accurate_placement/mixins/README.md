# Accurate Placement

A native mod for [Horizon](https://playhorizon.dev). Hold right click with a block and every new spot you look at gets a
block at once, however fast you move.

Vanilla places a block every four ticks while the use key is held: move fast and spots are skipped, move slowly and
extra blocks stack up. This mod places on each **fresh** target instead: a block you were not already looking at and did
not just place.

- **A new spot, a block**: bridging, walls, floors and sweeps along a row.
- **Staring at one spot places nothing more.**
- **A single click stays a single block**: after the first click it waits out vanilla's four ticks, unless you turn the
  view (10 degrees), then places what you looked at meanwhile.
- **Building backwards**: the block you just placed counts as fresh again once you have stepped a full block back.
- Blocks that react to a click (doors, chests, buttons, levers...) get vanilla's click; scaffolding on scaffolding climbs
  as in vanilla.

It works **only in your own worlds** (singleplayer and worlds you host). On a remote server it does nothing: the game places at its normal rate, so it cannot trip a server's anticheat.

This version has no settings screen or toggle key: remove the mod to turn it off.

## Credits

- [Accurate Block Placement: Reborn](https://github.com/KadTheHunter/AccurateBlockPlacement-Reborn)
  ([Modrinth](https://modrinth.com/mod/accurate-block-placement-reborn)), MIT, by Clayborn, Flourick, schwar and
  KadTheHunter: the original idea and placement rules.
- [horizon-accurate-block-placement](https://github.com/Lutziano888/horizon-accurate-block-placement) by Lutziano888
  (MIT): the first port to Horizon, which this mod is an update of. Its Horizon registry entry is archived (it was made
  for the earlier native API), so this one has its own id.
- This update for the current Horizon Nightly: foudretbr.

## Licence

MIT, see [LICENSE](LICENSE), which keeps all the original notices.
