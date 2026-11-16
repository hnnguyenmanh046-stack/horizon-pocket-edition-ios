# The native mods in this folder

`wrench` is Starling's own example. The other fifteen are the mods Horizon PE: Shared Worlds + Mods
is built with (`docs/shared-worlds-mods.md`): each folder is its author's bundle for Minecraft 26.3
as published in Horizon's mod registry (https://horizon-rust.github.io/horizon-mods), unpacked.
`tools/ios_native_mods.py` builds a mod id from its folder here before it asks the registry, so the
edition is built from exactly these sources.

| Folder | Mod, version | Author | Licence | Source |
| --- | --- | --- | --- | --- |
| `appleskin` | AppleSkin 0.1.3 | sillysoufi | MIT | https://github.com/soufied/AppleSkin-Horizon |
| `armor_status` | Armor Status 1.0.0 | CayMac | MIT | https://github.com/CayMac97/armor-status |
| `biomecompass` | BiomeCompass 0.2.2 | d3grando | MIT | https://github.com/d3grando/biomecompass |
| `connected_textures` | ContinuityHorizon 0.1.7 | tariksabanuz | MIT | https://github.com/tariksabanuz/ContinuityHorizon |
| `coords_hud` | Coordinates HUD 2.0.0 | Horizon | MIT | https://github.com/horizon-rust/horizon-mods/tree/main/native/coords-hud |
| `dynamiclights` | Better Dynamic Lights 1.0.3 | DeportedToHell | MIT (its `LICENSE`) | https://github.com/DeportedToHell/MCRE-Mods |
| `enchantmenthints` | Enchantment Hints 0.1.0 | d3grando | MIT | https://github.com/d3grando/enchantmenthints |
| `jade` | Jade 26.3.8 | Snownee; Horizon port by Joeypc6 | CC BY-NC-SA 4.0 | https://github.com/Joeypc6/Jade-Horizon |
| `rightharvest` | RightClickHarvest 0.1.1 | Domanffe | MIT | https://github.com/Domanffe/rightharvest |
| `shulker_peek` | ShulkerPeek 0.1.0 | vstxx | MIT | https://github.com/vstxx/shulkerpeek |
| `smartf3` | SmartF3 0.1.0 | d3grando | MIT | https://github.com/d3grando/smartf3 |
| `status_effect_hud` | Status Effect HUD 1.0.0 | CayMac | MIT | https://github.com/CayMac97/status-effect-hud |
| `treefall` | TreeFall 0.3.0 | d3grando | MIT | https://github.com/d3grando/treefall |
| `waymap` | Waymap 0.2.1 | Aron; screen model after Nirokx's horizon-waypoints (its `NOTICE.md`) | MIT | https://github.com/Lauch2/horizon-waymap-mod |
| `zoom` | Zoom 2.0.0 | Horizon | MIT | https://github.com/horizon-rust/horizon-mods/tree/main/native/zoom |

## Changed here

Nine of them are not as their authors released them. Every change is marked "Horizon PE" in the
source it is in.

| Mod | Change |
| --- | --- |
| Coordinates HUD | The box's height on the screen is a setting (`y`): the settings page's Height and Move buttons, and `/coords y <number>` or `/coords y auto`. It starts 58 from the top (the mod's own: the corner) |
| Armor Status | A Size setting (50% to 150%), starting at 80%; it starts left of the hotbar without its box (the mod's own: bottom right, with it) |
| Zoom | The starting zoom is 6x (the mod's own: 4x) |
| Waymap | The list panel on the HUD starts off (the mod's own: on). A tap on a text field of the waypoint editor brings a phone's keyboard up (`native_mod_screen_typing`, this edition's), and the first letter typed is no longer dropped when it is the key that opened the screen. Each world has its own waypoints (`native_world_key`: a world of this device by its folder, a shared world by its id): the mod's own kept one list for every world of the device |
| SmartF3 | Its copy of the game's HUD (`smartf3_hud`) has every step the game's has: the mod's own left out the touch controls, so F3 took every button off a phone's screen |
| Enchantment Hints | The hints are added in the item tooltip's hook point (`native_tooltip`) instead of `stations::Game::stack_tooltip`, so every tooltip has them, the one under the pointer too |
| TreeFall | Largest tree starts at 2048 logs (the mod's own: 256) |
| Status Effect HUD | It starts bottom left (the mod's own: top right) |
| Jade | The overlay starts at the left of the screen, in its upper part (Jade's own: the centre, the top) |

Jade is licensed CC BY-NC-SA 4.0 (https://creativecommons.org/licenses/by-nc-sa/4.0/): it is
shared here, changed as said above, under that same licence, and not for commercial use. The MIT
mods are shared under the MIT licence, which their sources name:

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software
> and associated documentation files (the "Software"), to deal in the Software without
> restriction, including without limitation the rights to use, copy, modify, merge, publish,
> distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions: The above copyright notice
> and this permission notice shall be included in all copies or substantial portions of the
> Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
> INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
> PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR
> ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
> ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

Copyright in each mod is its author's, as named above.
