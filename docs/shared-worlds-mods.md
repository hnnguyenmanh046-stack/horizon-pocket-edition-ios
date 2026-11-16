# Horizon PE: Shared Worlds + Mods

> **This page is about one edition.** Download it from the [README](../README.md#download). It is
> Horizon PE: Shared Worlds ([shared worlds and voice chat](shared-worlds.md)) with fifteen native
> mods compiled in, buttons for them, and defaults chosen for a phone.

**Contents:** [The mods](#the-mods-their-authors-and-licences) · [The buttons](#the-buttons) ·
[On a controller](#on-a-controller) · [What is set for you](#what-is-set-for-you) ·
[What was changed in three mods](#what-was-changed-in-three-mods) ·
[In a shared world](#in-a-shared-world) · [Updating and adding mods](#updating-and-adding-mods) ·
[Building it yourself](#building-it-yourself)

## The mods, their authors and licences

All fifteen are in the app from the first start. **Options > Mods** lists them: turn one off there,
or open it to change its settings and keys. Their sources, as built, are in the edition's branch:
[`native-mods/`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/shared-worlds-mods/native-mods).

| Mod | Version | What it does | Author | Licence | Source |
| --- | --- | --- | --- | --- | --- |
| Waymap | 0.2.1 | Waypoints you name and colour, in the world and on a minimap | Aron (screen model after Nirokx's horizon-waypoints) | MIT | [Lauch2/horizon-waymap-mod](https://github.com/Lauch2/horizon-waymap-mod) |
| Zoom | 2.0.0 | Hold a key to zoom in | Horizon | MIT | [horizon-rust/horizon-mods](https://github.com/horizon-rust/horizon-mods/tree/main/native/zoom) |
| Jade | 26.3.8 | Says what you are looking at | Snownee; Horizon port by Joeypc6 | CC BY-NC-SA 4.0 | [Joeypc6/Jade-Horizon](https://github.com/Joeypc6/Jade-Horizon) |
| AppleSkin | 0.1.3 | Hunger and saturation a food gives | sillysoufi | MIT | [soufied/AppleSkin-Horizon](https://github.com/soufied/AppleSkin-Horizon) |
| Armor Status | 1.0.0 | Armour and held items with their durability | CayMac | MIT | [CayMac97/armor-status](https://github.com/CayMac97/armor-status) |
| Status Effect HUD | 1.0.0 | Your effects and their time left | CayMac | MIT | [CayMac97/status-effect-hud](https://github.com/CayMac97/status-effect-hud) |
| Coordinates HUD | 2.0.0 | Your position and facing | Horizon | MIT | [horizon-rust/horizon-mods](https://github.com/horizon-rust/horizon-mods/tree/main/native/coords-hud) |
| TreeFall | 0.3.0 | Cut one log and the tree falls | d3grando | MIT | [d3grando/treefall](https://github.com/d3grando/treefall) |
| RightClickHarvest | 0.1.1 | Tap a ripe crop: picked and sown again | Domanffe | MIT | [Domanffe/rightharvest](https://github.com/Domanffe/rightharvest) |
| ShulkerPeek | 0.1.0 | A shulker box's contents in its tooltip | vstxx | MIT | [vstxx/shulkerpeek](https://github.com/vstxx/shulkerpeek) |
| Enchantment Hints | 0.1.0 | What each enchantment does, under its name | d3grando | MIT | [d3grando/enchantmenthints](https://github.com/d3grando/enchantmenthints) |
| BiomeCompass | 0.2.2 | A craftable compass that finds a biome | d3grando | MIT | [d3grando/biomecompass](https://github.com/d3grando/biomecompass) |
| SmartF3 | 0.1.0 | A tidier F3 debug screen | d3grando | MIT | [d3grando/smartf3](https://github.com/d3grando/smartf3) |
| ContinuityHorizon | 0.1.7 | Connected textures | tariksabanuz | MIT | [tariksabanuz/ContinuityHorizon](https://github.com/tariksabanuz/ContinuityHorizon) |
| Better Dynamic Lights | 1.0.3 | Held and dropped light sources light the world | DeportedToHell | MIT | [DeportedToHell/MCRE-Mods](https://github.com/DeportedToHell/MCRE-Mods) |

Six of them differ from their authors' releases here, each marked "Horizon PE" in its source:
Coordinates HUD, Armor Status and Enchantment Hints ([below](#what-was-changed-in-three-mods)), and
the starting values of TreeFall, Status Effect HUD and Jade
([What is set for you](#what-is-set-for-you)). Jade is shared under the same licence as it came
with (CC BY-NC-SA 4.0): not for commercial use.

## The buttons

Three buttons join the MCPE touch controls. Each is there while its mod is on, has the plate and
size of the top row's buttons, and can be moved, resized, recoloured, made see-through or hidden in
the layout editor (**H** > **Controls** > the editor), like every other.

| Button | Where it starts | What it does |
| --- | --- | --- |
| **Magnifying glass** | The left edge, under the **H** button | Zoom: zoomed in for as long as a finger is on it |
| **Map pin with three lines** | Right of the top row (after the camera button) | Waymap: opens the list of waypoints |
| **Map pin with a plus** | Right of that one | Waymap: adds a waypoint where you stand |

They press the mods' own keys: Zoom's (Z), Waymap's list (M) and Waymap's new waypoint (N). Change
a key in **Options > Mods** and its button follows. Held upright, the phone's top row makes room
for them; on an iPad it spreads out.

## On a controller

| Button | Does | Instead of |
| --- | --- | --- |
| D-pad **left** | Waypoints: Menu | Pick Block |
| D-pad **down** | Waypoints: Add Here | Advancements |
| **View** (held) | Zoom | Player list |

The three it replaces have no button until you give them one: **Options > Horizon Settings >
Controller Settings > Controller Binds**, where these three can be moved too.

## What is set for you

These are the starting values on a new install. Change any of them at any time; an app you
already had keeps the settings you chose.

| Where | Setting | Starts as |
| --- | --- | --- |
| Options > Horizon Settings > Extras | Dynamic lights | On |
| | Mouse Tweaks controls | On |
| | Sort buttons | On |
| Horizon menu (**H**) > Aiming | Virtual Mouse Speed | 140% |
| Options > Mods > TreeFall | Largest tree | 2048 logs |
| Options > Mods > Armor Status | Position | Hotbar left |
| | Box background | Off |
| Options > Mods > Jade > Configuration > Appearance | Overlay Position X | Left |
| | Overlay Position Y | Upper |
| Options > Mods > Status Effect HUD | HUD Position | Bottom-Left |

## What was changed in three mods

### Coordinates HUD: any height on the screen

So that nothing covers it, the box can sit at any height on the left edge.

- **Options > Mods > Coordinates HUD.** **Height** says where it is: `Automatic` (the top left
  corner, as the mod has it), or a number, the distance from the top of the screen. Tap it to switch
  between the two. **Move up 1**, **Move down 1**, **Move up 10** and **Move down 10** move it; the
  number changes as you tap.
- **In chat, while you watch it move:** `/coords y 40` puts it 40 from the top; `/coords y auto`
  gives it back to the corner.
- **Another phone:** set the same number there. It is in the game's own screen units, so it lands
  in the same place under the same things. The box never leaves the screen: a number too large
  for a smaller screen sits at the bottom.

### Armor Status: its size

**Options > Mods > Armor Status > Size** steps through 50%, 60%, 70%, 75%, 80%, 90%, 100%, 110%,
125% and 150%. The icons and their numbers shrink or grow together, and it stays against the
hotbar or its corner.

### Enchantment Hints: under the pointer, beside the item

- **Hints show under the virtual mouse** (and a real mouse), not only for an item picked by a tap.
  The mod used to add its lines in a place this game uses only for a tapped item; it now adds them
  where every item tooltip is built: slots, the creative tabs, the recipe book, chat.
- **A tapped item's tooltip is right beside it.** It used to stand at the edge of the inventory
  window; now it is next to the slot you picked, on the side with more room, as a mouse's is next
  to its pointer. This is so for every item's tooltip, hints or not.

## In a shared world

Friends on this edition and friends on Horizon PE: Shared Worlds play in the same shared world.
None of the fifteen adds a block or an item of its own, so nothing is missing for anyone.

| Works for you wherever you are | Works only while the **host** has it |
| --- | --- |
| Waymap, Zoom, Jade (what you look at), AppleSkin, Armor Status, Status Effect HUD, Coordinates HUD, ShulkerPeek, Enchantment Hints, SmartF3, ContinuityHorizon, Better Dynamic Lights | TreeFall, RightClickHarvest, BiomeCompass's crafting and search |

The second kind change the world itself, which is the host's game. When the world moves to a host
without them, trees and crops behave as in vanilla until it moves back. Waypoints are kept on your
own phone, for you.

## Updating and adding mods

- **Updates:** the game tells you when a newer release of this edition is out, and installs it
  through your sideloading app: the README's [Install](../README.md#install).
- **Removing a mod:** turn it off in **Options > Mods**. It stays in the app, doing nothing.
- **More mods, or newer versions of these:** that takes your own fork, as for any edition
  ([Native mods](../README.md#native-mods)): fork with all branches, and build from
  `shared-worlds-mods` with the mods you want.

## Building it yourself

In a fork, **Actions > ios > Run workflow**, **Use workflow from** `shared-worlds-mods`, tick **Make
and keep the unsigned .ipa**, and put this in **Native mods to build in**:

```
appleskin armor_status biomecompass connected_textures coords_hud dynamiclights enchantmenthints jade rightharvest shulker_peek smartf3 status_effect_hud treefall waymap zoom
```

A mod id that is a folder of `native-mods/` is built from that folder (these fifteen, as changed
here); any other is fetched from Horizon's registry. Add ids to the list for more mods.
