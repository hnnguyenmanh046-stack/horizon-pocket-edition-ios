# Litematica for Horizon 1.1.0

Schematics in the world, after Litematica by masa.

* Put `.litematic` or `.schem` files in `%APPDATA%\horizon\schematics` (the
  menu's **Open folder** button opens it). Litematica schematics from Java
  Edition and WorldEdit/Sponge `.schem` files (versions 1-3) work as they are.
* **K** opens the menu:
  * **Schematics**: click one to place it where you stand.
  * **Placement**: move it (±1 / ±10 per axis, or to you), turn it 0/90/180/270°,
    mirror it, see how far along the build is, unload it.
  * **Materials**: every item it needs, how many are still missing, and how
    many you have.
  * **Settings**: the overlay, layers, how far around you it shows, the
    on-screen status.
* The overlay outlines blocks: **blue** missing, **red** wrong block,
  **orange** right block in the wrong state, **pink** in the way (optional),
  plus a white box around the whole schematic and a yellow marker at its origin.
* **With a stick in your hand** (Litematica's tool): **right-click** moves the
  schematic to the block you look at, **Ctrl + scroll** nudges it the way you
  look, **Ctrl + right-click** turns it 90°.
* Hold **Left Alt** and scroll to move it the way you look. **Page Up** and **Page Down**
  change the layer (the first press switches to the layer you're on).
* Chat: `/litematica` or `/lm` with `list`, `load <name>`, `here`, `move <x> <y> <z>`,
  `rotate`, `mirror none|x|z`, `overlay off|on|wrong`, `layer <y>|all`, `unload`.

The placement is remembered and put back the next time you play.

Limits: blocks show as outlines, not see-through block models (Horizon has
no way yet for a mod to draw extra block models), and nothing is placed for
you. Missing counts cover loaded chunks only.
