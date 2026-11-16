<p align="center">
  <a href="https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0"><img src="https://img.shields.io/badge/download-1.0.0-2EA043" alt="Download 1.0.0"></a>
  <a href="https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0-shared-worlds"><img src="https://img.shields.io/badge/download-1.0.0%20Shared%20Worlds-8250DF" alt="Download 1.0.0 Shared Worlds"></a>
  <a href="https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0-shared-worlds-mods"><img src="https://img.shields.io/badge/download-1.0.0%20Shared%20Worlds%20%2B%20Mods-BF8700" alt="Download 1.0.0 Shared Worlds + Mods"></a>
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F" alt="Minecraft Java 26.3">
  <img src="https://img.shields.io/badge/platform-iOS%20%26%20iPadOS%2016%2B-1C1C1E?logo=apple&logoColor=white" alt="iOS and iPadOS 16 or newer">
  <a href="https://github.com/ludovic111/horizon"><img src="https://img.shields.io/badge/port%20of-Horizon-F0795A" alt="Port of Horizon"></a>
  <a href="https://github.com/starling-client/starling"><img src="https://img.shields.io/badge/based%20on-Starling-2A2470" alt="Based on Starling"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-3B2F7E" alt="MIT license"></a>
</p>

# Horizon Pocket Edition (iOS)

**Minecraft: Java Edition 26.3 on iPhone and iPad, running natively.** Horizon Pocket Edition
("Horizon PE" on your home screen) is an iOS port of [Horizon](https://github.com/ludovic111/horizon),
Minecraft's Java game rewritten in Rust, based on [Starling](https://github.com/starling-client/starling),
Horizon's Android port. It plays the same worlds, rules and terrain as Java Edition, and draws with
Metal: no Java, no emulation.

> **Not an official Minecraft product.** Not approved by or associated with Mojang or Microsoft. The
> app contains no Minecraft code or assets: it downloads the game's files from Mojang on your phone.
> To play online you need your own copy of Minecraft: Java Edition.

<table>
  <tr>
    <td><img src="docs/media/sunset.jpg" alt="A sunset over a savanna"></td>
    <td><img src="docs/media/village.jpg" alt="A savanna village"></td>
    <td><img src="docs/media/underwater.jpg" alt="Under the sea"></td>
  </tr>
  <tr>
    <td><img src="docs/media/survival.jpg" alt="A survival base"></td>
    <td><img src="docs/media/night.jpg" alt="A night in the hills"></td>
    <td><img src="docs/media/nether.jpg" alt="The Nether"></td>
  </tr>
</table>

## Contents

| Start here | Playing | More |
| --- | --- | --- |
| [Download](#download) | [Controls](#controls) | [Help](#help) |
| [Which one should I get?](#which-one-should-i-get) | [Shader packs](#shader-packs) | [For developers](#for-developers) |
| [Features](#features) | [Shared worlds and voice chat](#shared-worlds-and-voice-chat) | [Credits](#credits) |
| [Install](#install) | [The built-in mods](#the-built-in-mods) | [License](#license) |
| | [Native mods](#native-mods) | |

## Download

There are three editions of Horizon PE. All three are the same game; each one adds to the one before.

| | **Horizon PE** | **Horizon PE: Shared Worlds** | **Horizon PE: Shared Worlds + Mods** |
| --- | --- | --- | --- |
| **Latest stable** | **[1.0.0](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0)** | **[1.0.0 Shared Worlds](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0-shared-worlds)** | **[1.0.0 Shared Worlds + Mods](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0-shared-worlds-mods)** |
| **Direct download** | [HorizonPE-1.0.0.ipa](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/download/v1.0.0/HorizonPE-1.0.0.ipa) | [HorizonPE-1.0.0-SharedWorlds.ipa](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/download/v1.0.0-shared-worlds/HorizonPE-1.0.0-SharedWorlds.ipa) | [HorizonPE-1.0.0-SharedWorlds-Mods.ipa](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/download/v1.0.0-shared-worlds-mods/HorizonPE-1.0.0-SharedWorlds-Mods.ipa) |
| **Source branch** | [`main`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/main) | [`shared-worlds`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/shared-worlds) | [`shared-worlds-mods`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/shared-worlds-mods) |

### Which one should I get?

| | Horizon PE | Shared Worlds | Shared Worlds + Mods |
| --- | :---: | :---: | :---: |
| The whole game: single player, servers, LAN, Play with Friends | Yes | Yes | Yes |
| Touch controls, controllers, keyboard and mouse | Yes | Yes | Yes |
| Shader packs, resource packs, skins, mods and native mods | Yes | Yes | Yes |
| Tells you when an update is out, and installs it | Its own | Its own | Its own |
| **Shared worlds:** a world your group owns, hosted by whoever is playing, with no server | No | **Yes** | **Yes** |
| **Backups** of shared worlds that anyone can ask to load | No | **Yes** | **Yes** |
| **Voice chat** in shared worlds, with a volume for each player | No | **Yes** | **Yes** |
| **15 quality of life mods built in**, ready on the first start | No | No | **Yes** |
| **Zoom and waypoint buttons** in the touch controls and on a controller | No | No | **Yes** |
| Asks for the microphone | Never | Only when you turn your microphone on | Only when you turn your microphone on |
| Needs a free Cloudflare account | No | Only for the one who shares a world | Only for the one who shares a world |

- **Get Horizon PE** if you play alone, on servers, or with friends through an invite code. It is
  the simplest build and has the fewest moving parts.
- **Get Horizon PE: Shared Worlds** if your group wants one world that any of you can open at any
  time, and to talk in it. Everything else in it is the same as Horizon PE.
- **Get Horizon PE: Shared Worlds + Mods** if you want that with a minimap of waypoints, zoom, a
  coordinates display, tree felling and eleven more mods already in the app: no fork, no token, no
  hour of building. [What is in it](#the-built-in-mods).

**Switching is safe.** All three editions are the same app to iOS: install any one over another and
your worlds, settings, accounts and mods stay. A shared world stays in its storage if you go back to
Horizon PE, and is there again when you return.

## Features

**The game**

- Minecraft: Java Edition 26.3, natively: the same worlds, terrain, mobs, redstone and rules.
- Single player, Java Edition servers, LAN worlds, and **Play with Friends** (an invite code, no
  server or port forwarding).
- Sign in with Microsoft or Ely.by, or play offline.
- Java Edition worlds open as they are: copy a world in through the Files app and play it.

**Made for a phone**

- Two touch styles, **MCPE** and **GoyControl**, with an editor that moves, resizes, recolours, hides
  and rebinds every button.
- Game controllers, keyboards and mice; gyroscope aiming; a virtual mouse for menus.
- Portrait and landscape, iPhone and iPad.
- Performance defaults chosen for your device, and a render resolution slider.
- **Update notices in the game**, for the edition you have, with an Update button.

**Looks and mods**

- Iris and OptiFine **shader packs**, translated to Metal on the phone. Starling Visuals is included.
- Resource packs and skins.
- A built-in **catalog** of mods, shader packs and resource packs.
- **Native mods** built into the app for you by GitHub: [Native mods](#native-mods).

**In the Shared Worlds editions**

- **Shared worlds:** one world for your group. Whoever opens it hosts it; when they leave it moves
  to someone still playing, and everyone follows by themselves.
- **Backups** every few minutes (you choose how often), kept for up to 26 weeks, loaded and undone
  from inside the game.
- **Voice chat:** off until you turn it on, a mute button in the touch controls, a volume and a mute
  for each player, a choice of microphone, and an optional noise filter.

**Only in the Shared Worlds + Mods edition**

- **15 mods built in:** Waymap, Zoom, Jade, AppleSkin, Armor Status, Status Effect HUD, Coordinates
  HUD, TreeFall, RightClickHarvest, ShulkerPeek, Enchantment Hints, BiomeCompass, SmartF3,
  ContinuityHorizon and Better Dynamic Lights.
- **Buttons for them:** a zoom button and two waypoint buttons in the touch controls, placed and
  styled like the others, and the same three on a controller.
- **Set up for a phone:** the mods' displays start where they don't cover each other, and dynamic
  lights, the Mouse Tweaks controls and the sort buttons are on from the first start.

## Install

**You need** an iPhone or iPad on iOS 16 or newer (iPhone 8 and later), about 2 GB free, and a
sideloading app: [SideStore](https://sidestore.io), [AltStore](https://altstore.io),
[Sideloadly](https://sideloadly.io) (from a computer) or [LiveContainer](https://github.com/LiveContainer/LiveContainer).

1. **Download the `.ipa`** of the edition you want from [Download](#download).
2. **Install it** with your sideloading app. It signs the app with your Apple ID; with a free Apple ID,
   refresh it every 7 days (SideStore and AltStore do it for you). Nothing is lost.
3. **Start it.** Sign in with Microsoft (the account that owns Java Edition) or Ely.by, or play
   offline. The first start downloads the game's files from Mojang, so stay on Wi-Fi.

**Updating:** the game tells you. When a newer release of **your edition** is out, the title screen
shows a card: **Update** downloads it and opens the share sheet, where you pick your sideloading app
(or Save to Files). It installs over the app; worlds, settings, accounts and mods stay. **Tomorrow**
asks again in a day. An app built in your own fork (for native mods) is told how to update the fork
instead.

**Switching edition:** install the other edition's `.ipa` over the app. Nothing is lost.

**Want native mods?** They need the app built in your own fork instead of a release:
[Native mods](#native-mods), step 1.

**Your files** are in the Files app under **On My iPhone > Horizon PE**: `saves` (worlds),
`resourcepacks`, `shaderpacks`, `mods`, `screenshots` and `logs`. Open a world, pack, skin or mod
(`.zip`, `.mrpack`, `.hmod`, `.hnmod`, `.png`) with Horizon PE from Files or Safari to add it.

<details>
<summary><b>Had a build from before the rename?</b></summary>

Older builds used the app ID `dev.horizon.starling`; now it is `dev.horizon.pocketedition`, so iOS
installs the new one as a separate app. Start the new app once, then in the Files app copy `saves`
(and `resourcepacks`, `shaderpacks` if you like) from the old app's **Horizon PE** folder into the new
one's (the nearly empty one), and delete the old app. Sign in again in the new app.

</details>

## Controls

Two touch styles: **MCPE** (the default, like Minecraft Pocket Edition) and **GoyControl** (labelled
buttons you place and bind yourself). A keyboard, mouse or game controller works too; the touch
controls step aside while you use one.

<p align="center"><img src="docs/media/controls-mcpe.jpg" alt="Horizon PE in a world with the MCPE touch controls"></p>

| Button (MCPE) | What it does |
| --- | --- |
| **Pad** (bottom left) | Move. Put a finger down anywhere on the left side and drag |
| **Sword** / **crosshair** | Attack (break, hit; hold to keep breaking) / Use (place, eat, open) |
| **Up arrow** / **down arrow** | Jump / sneak. Up and down while swimming or flying; sneak gets off a mount |
| **Double arrow** | Sprint: tap on, tap off. Turn **Toggle Sprint** off in the Horizon menu to hold it instead |
| **...** (by the hotbar) | Inventory |
| **Top row** | Debug screen (F3), chat, pause, Options, camera view (F5) |
| **H** (top left) | The Horizon menu: touch settings and the layout editor |
| **...** (top left) | F3, F5, Chat and Keys, plus the F3 combinations |

Drag anywhere else to look around; tap a hotbar slot to select it.

**In an inventory or chest:** tap an item to pick it up, tap a slot to put it down. The sneak button
becomes **Shift** (its arrow fills in when on): with Shift on, tapping an item **quick moves** it.
You can keep walking with the inventory open (**Inventory Walk Controls**). With the virtual mouse
(turn on **Virtual Mouse Button** in the Horizon menu), **Mouse** shows a pointer for menus, and
**LMB** / **RMB** click with it; they quick move too while Shift is on. **Back** (top right) closes
any screen.

<p align="center"><img src="docs/media/controls-goycontrol.jpg" alt="Horizon PE in a world with the GoyControl touch controls"></p>

**GoyControl** labels every button: **PRI** / **SEC** (attack / use), **JUMP**, **SNEAK**, **SPRINT**,
**INV**, **DROP**, **SWAP** (hands), **CHAT**, **TAB** (player list), **DEBUG** (F3), **3RD** (camera
view), **ESC** and **MOUSE**. The sliders button is the Horizon menu; drag it where you like.

**The Horizon menu** (**H**, or the sliders button) is where the touch settings are:

- **Controls:** switch MCPE / GoyControl, presets, the layout editor (move, resize, recolour, hide or
  rebind any button), look sensitivity, invert Y.
- **Aiming:** aim mode, where the movement pad can start, virtual mouse speed, gyroscope aiming.
- **Buttons:** button size, the Swap and Shield buttons, left-handed layout, **Toggle Sprint**,
  **Inventory Walk Controls**.
- **Recommended** has the most used of these in one place; **Advanced** has vibration and auto-jump.

Minecraft's own **Options > Controls** (key binds, and Sneak / Sprint toggles for a keyboard) is
separate.

## Shader packs

Iris and OptiFine shader packs work (Complementary, BSL, MakeUp, Photon and more). Install them from
the catalog on the **Shader Packs** screen, or put the `.zip` in `shaderpacks` in Files. **Starling
Visuals**, made for phones, comes with the app.

**Turning a pack on takes a few seconds, and that's normal:** every program in it is translated for
Metal and compiled, once, as it loads. Big packs take longer and cost more frame rate and battery;
Starling Visuals and the **Auto** profile are the light options.

## Shared worlds and voice chat

*In [Horizon PE: Shared Worlds and Shared Worlds + Mods](#download).* The full guide, with every
detail, is **[docs/shared-worlds.md](docs/shared-worlds.md)**.

A shared world belongs to your group, not to one of you. Whoever opens it while nobody is playing
hosts it on their own phone, and friends join them with no server. When the host leaves, the world
moves to a player who is still in it, and everyone else follows by themselves. A copy of the world
lives in a storage bucket your group owns, which is how the next host gets the newest version.

**Set it up once (about 10 minutes, one person, on a phone)**

1. Make a free [Cloudflare](https://dash.cloudflare.com) account, turn on **R2 Object Storage** and
   create a bucket. R2 asks for a payment card; a group's playing stays far inside the free 10 GB.
2. Create an **API token** with **Object Read & Write** on that bucket.
3. In the game: **Shared Worlds** on the title screen, **Share a World...**, pick the world, paste the
   endpoint, the bucket's name and the two keys, then **Share World**.
4. Open the world in the list, **Copy World Invite**, and send it to your friends privately. They tap
   **Add with Invite...**, **Paste**, **Add World**.

The game's own **How to Set Up...** page has these steps too. The invite holds the bucket's key:
send it only to friends you trust with the world.
[Step by step](docs/shared-worlds.md#set-up-the-storage-once-about-10-minutes-on-your-phone)

**Playing**

| You want to | Do this |
| --- | --- |
| Play | **Start** when nobody is playing (you host), **Join** when someone is |
| Stop hosting | Save and Quit, and keep the app open until the upload finishes. The world moves to another player |
| Hand the world over | Game Menu > **Friends** > **Give Host to ...** (a guest can **Ask to Host**) |
| Let someone in who doesn't have the world | Game Menu > **Friends** shows an invite code, with **Copy Code** |
| Go back in time | Shared Worlds > the world > **Backups...** > pick one. The player who shared the world approves it; **Undo Last Load** brings back what it replaced |
| Change how often backups are made | The world's Backups page, **Backups: every ... min** (2 to 60; the player who shared the world) |

If the host's phone dies, the others wait about a minute and a half, then one of them takes the
world from its last copy. Nothing is damaged.
[More about playing](docs/shared-worlds.md#playing) ·
[How friends reach the host](docs/shared-worlds.md#reaching-the-host) ·
[Backups](docs/shared-worlds.md#backups)

**Voice chat**

| | |
| --- | --- |
| **Off until you turn it on** | Joining a world never plays anyone's voice and never touches your microphone. A gold line in chat says when others are using it |
| **Turn it on** | Game Menu > **Friends** > **Voice Chat** > **Voice Chat: On**. You hear everyone, wherever they are in the world. Your microphone is still off |
| **Your microphone** | The microphone button at the left of the top row of the touch controls: red while muted. Place it in the controls editor like the others |
| **Each player** | A volume slider from 0 to 200 % and a **Mute** button, remembered for that player in every world |
| **Which microphone** | **Mic** on the Voice Chat page. Automatic keeps Bluetooth headphones in full quality by using the device's own microphone |
| **Noise Filter** | Turns down steady noise (a fan, traffic) in your microphone. Off unless you turn it on |
| **Turns itself off** | When your headphones disconnect, so nobody is suddenly heard out loud |

[More about voice chat](docs/shared-worlds.md#voice-chat)

**Good to know:** 8 players at most; shared worlds are between phones running a Shared Worlds
edition (friends on PC are planned for later); friends who aren't signed in can join and host under
their name. Friends with and without the built-in mods play in the same world, but **TreeFall,
RightClickHarvest and BiomeCompass act on the world itself, so they work only while the host has
them**: [more](docs/shared-worlds-mods.md#in-a-shared-world).

## The built-in mods

*Only in [Horizon PE: Shared Worlds + Mods](#download).* The full guide is
**[docs/shared-worlds-mods.md](docs/shared-worlds-mods.md)**.

Fifteen native mods are compiled into this edition, so they work from the first start. Turn any of
them off, or change its settings, in **Options > Mods**.

| Mod | What it does |
| --- | --- |
| **Waymap** | Waypoints you name and colour, shown in the world and on a minimap |
| **Zoom** | Hold to zoom in |
| **Jade** | Says what you are looking at: the block or mob, its health, what is in it |
| **AppleSkin** | Shows how much hunger and saturation a food gives |
| **Armor Status** | Your armour and held items with their durability, beside the hotbar |
| **Status Effect HUD** | Your potion effects with their time left |
| **Coordinates HUD** | Your position and the way you face |
| **TreeFall** | Cut the bottom log and the whole tree falls |
| **RightClickHarvest** | Tap a grown crop to harvest and replant it |
| **ShulkerPeek** | See inside a shulker box from its tooltip |
| **Enchantment Hints** | Says under each enchantment what it does |
| **BiomeCompass** | A compass that finds a biome |
| **SmartF3** | A tidier debug screen |
| **ContinuityHorizon** | Connected textures: glass and bookshelves join up |
| **Better Dynamic Lights** | A held torch lights the way |

**The buttons**

| Button | Where | What it does | On a controller |
| --- | --- | --- | --- |
| **Magnifying glass** | Left of the top row | Tap to zoom in, tap again to zoom out (or hold) | Hold **View** |
| **Pin with lines** | Right of the top row | Opens the list of waypoints | D-pad **left** |
| **Pin with a plus** | Right of that | Adds a waypoint where you stand | D-pad **down** |

Move, resize, recolour or hide them in the layout editor like every other button. They press the
keys the mods use (Z, M and N unless you change them in **Options > Mods**).

**Changed for this edition:** Coordinates HUD can sit at any height on the screen (its settings
page says the number, to copy to another phone); Armor Status has a **Size** setting; Enchantment
Hints show under the virtual mouse's pointer as well as for a tapped item, and a tapped item's
tooltip is right beside it. [Every default and change](docs/shared-worlds-mods.md)

## Native mods

*The Shared Worlds + Mods edition already has [fifteen](#the-built-in-mods): this is for adding
your own, to any edition.*

Native mods change the game's code, and iOS only runs code an app shipped with. So a native mod is
**built into the app**: GitHub compiles a new `.ipa` with the mods you pick, you install it over your
app, and your worlds stay. Builds run in the GitHub repository the app came from, and only its owner
can start them, so you need **your own fork** and the app built from it.

**1. Your own copy (once)**

1. On [GitHub](https://github.com) (signed in), open this repository and tap **Fork**. For a
   Shared Worlds edition, untick **Copy the `main` branch only**. Then **Create fork**. Keep it
   public: then GitHub's build machines are free.
2. In your fork, open **Actions** and enable workflows (GitHub asks once).
3. **Actions > ios > Run workflow**. Under **Use workflow from**, leave `main` for Horizon PE, or pick
   `shared-worlds` or `shared-worlds-mods` for those editions (for the last, also put its fifteen
   mods in **Native mods to build in**: [the list](docs/shared-worlds-mods.md#building-it-yourself)).
   Tick **Make and keep the unsigned .ipa**, run it.
   In about an hour the run has a `horizon-pe-ios-<number>` artifact: install that `.ipa`
   ([Install](#install)). The app remembers the branch it was built from, and builds its mods there.

**2. A token (once a year)**

1. In the game, **Options > Mods > Make a token**. GitHub's token form opens, filled in (name, one
   year, **Actions: Read and write**).
2. Under **Repository access** pick **Only select repositories** and choose your fork.
3. **Generate token**, copy it (GitHub shows it once), and in the game tap **Paste GitHub token**.
   It stays in your phone's Keychain and only goes to GitHub.

**3. Build**

1. **Options > Mods > Catalog**: install the native mods you want.
2. On the Mods page, tap **Build on GitHub**, and keep playing.
3. In 30 to 60 minutes the title screen says **Mods ready: install**. Tap it, then **Install...**,
   and pick your sideloading app in the share sheet.

To **remove** a native mod, remove it and build again, or just turn it off in **Options > Mods**.
To **update**, tap **Sync fork > Update branch** on your fork's GitHub page (on the branch your app
was built from), then build again.

<details>
<summary><b>If a build fails</b></summary>

- **"GitHub refused (403)" or "(404)":** the token isn't for the repository this app was built from.
  Make it for your fork with **Actions: Read and write**, and use the app built from your fork.
- **"Mods build failed":** open the run in your fork's **Actions** tab to see why. Usually a mod made
  for another game version; build again without it.
- **Token expired** (after a year): **Make a token**, then **Paste new token**.
- **Nothing starts:** workflows aren't enabled in your fork (step 1.2), or the fork doesn't have the
  branch your app was built from (step 1.1).
- **Without the game:** **Actions > ios > Run workflow** with mod ids in **Native mods to build in**
  (for example `wrench fps_overlay`).

</details>

Every mod is checked against the registry's size and SHA-256 before it is built, and every build is
started once in the iOS Simulator before you get it. Builds are kept in your fork's Actions for 30
days. Not every mod in the catalog builds for this port yet: a mod made for another version of the
game can fail, and the build says which. More detail: [docs/ios.md](docs/ios.md#native-mods).

## Help

| Problem | What to do |
| --- | --- |
| **The app won't open after a week** | A free Apple ID signs apps for 7 days. Refresh it in SideStore or AltStore, or install the `.ipa` again. Nothing is lost |
| **Low frame rate or a hot phone** | Lower the render distance (Options > Video Settings), use Starling Visuals or no shader pack, and close other apps |
| **The touch layout is a mess** | Horizon menu > Controls > presets > **Default** |
| **Friends hear an echo of themselves in voice chat** | Use headphones, or turn your microphone off while you aren't talking |
| **Something crashed or looks wrong** | Options > Diagnostics > **Export diagnostic ZIP**, then [open an issue](https://github.com/shopifygav222/horizon-pocket-edition-ios/issues/new/choose) with it, the edition you use, and what you were doing |

## For developers

No Mac needed: GitHub builds the app.

| Branch | What it is | Release |
| --- | --- | --- |
| [`main`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/main) | Horizon PE | `v1.0.0` |
| [`shared-worlds`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/shared-worlds) | `main` plus shared worlds and voice chat | `v1.0.0-shared-worlds` |
| [`shared-worlds-mods`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/shared-worlds-mods) | `shared-worlds` plus fifteen native mods (in its `native-mods/`), their buttons and defaults | `v1.0.0-shared-worlds-mods` |

| Workflow | What it does |
| --- | --- |
| [`ios.yml`](.github/workflows/ios.yml) | Compiles the game for iOS, lints it, prepares the game files, bundles the app (checking it carries none of Mojang's files) and starts it in the iOS Simulator. Dispatched with **Make and keep the unsigned .ipa**, it keeps the app |
| [`ios-guard.yml`](.github/workflows/ios-guard.yml) | Formatting, and Starling's desktop and Android builds still compile |
| [`native-mods-check.yml`](.github/workflows/native-mods-check.yml) | Builds every native mod of the catalog against this source, one job each |
| `shared-worlds.yml`, `shared-worlds-e2e.yml`, `voice.yml` | On the `shared-worlds` branches: the shared worlds and voice crates' tests, and two games sharing one world end to end |

On a Mac with Xcode: `rustup target add aarch64-apple-ios`, `python3 tools/ios_prepare.py` (needs Java
25), then `brew install xcodegen && cd packaging/ios && xcodegen generate` and open
`HorizonPE.xcodeproj`.

[docs/ios.md](docs/ios.md) covers the iOS app in depth; [IOS-SYNC.md](IOS-SYNC.md) lists the Starling
version this is based on, the repository layout, every change to Starling's files, and how to take in
a new Starling release. The rest of `docs/` is Horizon's and Starling's own documentation.

## Credits

- **[Horizon](https://github.com/ludovic111/horizon)** by Ludovic and contributors: Minecraft: Java
  Edition rewritten in Rust, the game this app ports.
- **[Starling](https://github.com/starling-client/starling)** by [JuanLuci](https://github.com/JuanLuci)
  and [Vasysik](https://github.com/Vasysik): Horizon's Android port, which this app is based on,
  with its touch controls, accounts, catalog, skins and Starling Visuals. The world screenshots above
  are Starling's.
- **[horizon-iOS](https://github.com/meehandaniel4/horizon-iOS)**: the first native mod builds for iOS
  on GitHub Actions.
- **[Opus](https://opus-codec.org)** and **[RNNoise](https://github.com/xiph/rnnoise)** (Xiph.Org):
  the voice codec and the noise filter of the Shared Worlds editions.
- **The mods of the Shared Worlds + Mods edition** and their authors: Waymap (Aron, after Nirokx's
  horizon-waypoints), Zoom and Coordinates HUD (Horizon), Jade (Snownee; Horizon port by Joeypc6),
  AppleSkin (sillysoufi), Armor Status and Status Effect HUD (CayMac), TreeFall, Enchantment Hints,
  BiomeCompass and SmartF3 (d3grando), RightClickHarvest (Domanffe), ShulkerPeek (vstxx),
  ContinuityHorizon (tariksabanuz) and Better Dynamic Lights (DeportedToHell). Their sources and
  licences: [docs/shared-worlds-mods.md](docs/shared-worlds-mods.md#the-mods-their-authors-and-licences).

## License

MIT, as Horizon and Starling: see [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
