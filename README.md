<p align="center">
  <a href="https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0"><img src="https://img.shields.io/badge/download-1.0.0-2EA043" alt="Download 1.0.0"></a>
  <a href="https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0-shared-worlds"><img src="https://img.shields.io/badge/download-1.0.0%20Shared%20Worlds-8250DF" alt="Download 1.0.0 Shared Worlds"></a>
  <a href="https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/tag/v1.0.0-shared-worlds-mods"><img src="https://img.shields.io/badge/download-1.0.0%20Shared%20Worlds%20%2B%20Mods-BF8700" alt="Download 1.0.0 Shared Worlds + Mods"></a>
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F" alt="Minecraft Java 26.3">
  <img src="https://img.shields.io/badge/platform-iOS%20%26%20iPadOS%2016%2B-1C1C1E?logo=apple&logoColor=white" alt="iOS and iPadOS 16 or newer">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-3B2F7E" alt="MIT license"></a>
</p>

# Horizon Pocket Edition (iOS)

**Minecraft: Java Edition 26.3 on iPhone and iPad, running natively.** No Java, no emulation.

Horizon PE is a port of [Horizon](https://github.com/ludovic111/horizon) (Minecraft's Java game
rewritten in Rust), based on [Starling](https://github.com/starling-client/starling), Horizon's
Android port. It plays the same worlds, servers and rules as Java Edition.

> **Not an official Minecraft product.** Not approved by or associated with Mojang or Microsoft.
> The app contains no Minecraft code or assets: it downloads the game's files from Mojang on your
> device. To play online you need your own copy of Minecraft: Java Edition.

<table>
  <tr>
    <td><img src="docs/media/sunset.jpg" alt="A sunset over a savanna"></td>
    <td><img src="docs/media/village.jpg" alt="A savanna village"></td>
    <td><img src="docs/media/underwater.jpg" alt="Under the sea"></td>
  </tr>
</table>

**Contents:** [Download](#download) · [Install](#install) · [Controls](#controls) ·
[Mods](#mods) · [Shader packs](#shader-packs) · [Shared worlds](#shared-worlds-and-voice-chat) ·
[Help](#help) · [For developers](#for-developers) · [Credits](#credits)

## Download

There are three editions. Each one is the edition before it, plus more.

| | Horizon PE | Shared Worlds | Shared Worlds + Mods |
| --- | :---: | :---: | :---: |
| **Download** | **[1.0.0](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/download/v1.0.0/HorizonPE-1.0.0.ipa)** | **[1.0.0](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/download/v1.0.0-shared-worlds/HorizonPE-1.0.0-SharedWorlds.ipa)** | **[1.0.0](https://github.com/shopifygav222/horizon-pocket-edition-ios/releases/download/v1.0.0-shared-worlds-mods/HorizonPE-1.0.0-SharedWorlds-Mods.ipa)** |
| The whole game: single player, servers, LAN, Play with Friends | Yes | Yes | Yes |
| Touch controls, controllers, keyboard and mouse | Yes | Yes | Yes |
| Shader packs, resource packs, skins, the mod catalog | Yes | Yes | Yes |
| A world your group shares, with no server | | Yes | Yes |
| Backups of shared worlds | | Yes | Yes |
| Voice chat in shared worlds | | Yes | Yes |
| 15 mods built in, with their own buttons | | | Yes |

- **Horizon PE:** you play alone, on servers, or with friends by invite code.
- **Shared Worlds:** your group wants one world any of you can open, and to talk in it.
- **Shared Worlds + Mods:** the same, with a minimap, zoom, coordinates and twelve more mods ready
  from the first start.

Switching is safe: install any edition over another and your worlds, settings and accounts stay.

## Install

You need an iPhone or iPad on iOS 16 or newer, about 2 GB free, and a sideloading app:
[SideStore](https://sidestore.io), [AltStore](https://altstore.io),
[Sideloadly](https://sideloadly.io) or [LiveContainer](https://github.com/LiveContainer/LiveContainer).

1. **Download** the `.ipa` of your edition from the table above.
2. **Install it** with your sideloading app. With a free Apple ID, refresh it every 7 days
   (SideStore and AltStore do this for you). Nothing is lost.
3. **Start it** on Wi-Fi: the first start downloads the game's files from Mojang. Sign in with
   Microsoft or Ely.by, or play offline.

**Updating.** When a newer release of your edition is out, the title screen shows a card. Tap
**Update** and pick your sideloading app. It installs over the app; nothing is lost.

**Your files** are in the Files app under **On My iPhone > Horizon PE**: `saves` (worlds),
`resourcepacks`, `shaderpacks`, `mods`, `screenshots`, `logs`. To add a world, pack, skin or mod,
open its file with Horizon PE from Files or Safari.

## Controls

Touch controls show by themselves, and step aside while you use a controller, keyboard or mouse.

<p align="center"><img src="docs/media/controls-mcpe.jpg" alt="Horizon PE in a world with the touch controls"></p>

| Control | What it does |
| --- | --- |
| **Pad** (bottom left) | Move |
| **Drag** anywhere else | Look around |
| **Sword** | Attack and break. **Hold it** to keep mining, and to keep hitting a monster each time your weapon is ready |
| **Crosshair button** | Use and place. **Hold it** with blocks to keep building; every block goes on the same side as the first, so a bridge, wall or pillar stays straight |
| **Up arrow** / **down arrow** | Jump / sneak (tap on, tap off) |
| **Double arrow** | Sprint (tap on, tap off) |
| **Hotbar** | Tap a slot, or slide along it and lift on the one you want |
| **...** by the hotbar | Inventory |
| **Top row** | Debug screen, chat, pause, options, camera view |
| **H** (top left) | The Control Hub |

**The Control Hub** (the **H** button) holds every touch setting:

- **Controls:** the layout editor (move, resize, recolour, hide or rebind any button), presets,
  look sensitivity.
- **Aiming:** aim mode, gyroscope aiming, the virtual mouse.
- **Buttons:** button size, **hotbar size**, left-handed layout, Toggle Sprint.

In an inventory, tap an item to pick it up and tap a slot to put it down. The sneak button becomes
**Shift**: with it on, tapping an item moves it across at once.

## Mods

Open the **Catalog** from the title screen or **Options > Mods**. It has mods, shader packs and
resource packs. Tap a row to read its full description.

Every mod is marked with its kind, and you can filter by it:

| Kind | What it is | How it is added |
| --- | --- | --- |
| **CONTENT** | Blocks, items and recipes described as data | Tap **Install**. It works at once |
| **NATIVE** | A mod with code of its own | Tap **Install**, then build it into the app (below) |

### Adding a native mod

iOS only runs code an app shipped with, so a native mod is built into the app. The game does this
for you: no computer and no GitHub account.

1. Install the mods you want from the catalog, then close it. The Mods page opens.
2. Tap **Build**, and keep playing. It takes 30 to 60 minutes, or no time if someone built the
   same mods before.
3. When the game says the build is ready, open **Options > Mods**, tap **Install...** and pick
   your sideloading app. It installs over your app; nothing is lost.

- **Remove a mod:** turn it off in **Options > Mods**. It stops at once; **Build** again to take
  it out of the app.
- **Update an app that has mods:** when a newer version is out, the Mods page says so. Tap
  **Build** to get it with your mods.
- **Limits:** six new builds a day for one device. The build service is sent the game's version,
  the mods you picked and a random number; it keeps no account.

### The 15 built-in mods

*Only in the Shared Worlds + Mods edition.* They work from the first start. Turn any off, or
change its settings, in **Options > Mods**.

| Mod | What it does |
| --- | --- |
| **Waymap** | Waypoints and a minimap |
| **Zoom** | Zoom in |
| **Jade** | Says what you are looking at |
| **AppleSkin** | Shows how much hunger a food gives |
| **Armor Status** | Your armour and its durability |
| **Status Effect HUD** | Your potion effects and their time left |
| **Coordinates HUD** | Your position and the way you face |
| **TreeFall** | Cut the bottom log and the whole tree falls |
| **RightClickHarvest** | Tap a grown crop to harvest and replant it |
| **ShulkerPeek** | See inside a shulker box from its tooltip |
| **Enchantment Hints** | Says what each enchantment does |
| **BiomeCompass** | A compass that finds a biome |
| **SmartF3** | A tidier debug screen |
| **ContinuityHorizon** | Connected textures |
| **Better Dynamic Lights** | A held torch lights the way |

Three buttons come with them: **zoom** (left edge), **waypoint list** and **add waypoint** (top
row). On a controller: hold **View**, D-pad **left**, D-pad **down**.
[The full guide](docs/shared-worlds-mods.md)

## Shader packs

Iris and OptiFine shader packs work (Complementary, BSL, MakeUp, Photon and more). Install them
from the catalog, or put the `.zip` in `shaderpacks` in Files. **Starling Visuals**, made for
phones, comes with the app.

Turning a pack on takes a few seconds while it is prepared for your device. Big packs cost frame
rate and battery.

## Shared worlds and voice chat

*In the Shared Worlds editions.* [The full guide](docs/shared-worlds.md)

A shared world belongs to your group. Whoever opens it first hosts it on their device; when they
leave, it moves to someone still playing. A copy lives in storage your group owns, so the next
host always has the newest version.

**Set it up once** (one person, about 10 minutes, on a phone):

1. Make a free [Cloudflare](https://dash.cloudflare.com) account, turn on **R2 Object Storage**
   and create a bucket.
2. Create an **API token** with **Object Read & Write** on that bucket.
3. In the game: **Shared Worlds > Share a World...**, pick the world and paste the bucket's
   details.
4. **Copy World Invite** and send it to your friends. They tap **Add with Invite...**.

The invite holds the bucket's key: send it only to friends you trust with the world.

| You want to | Do this |
| --- | --- |
| Play | **Start** when nobody is playing, **Join** when someone is |
| Stop hosting | Save and Quit, and keep the app open until the upload finishes |
| Hand the world over | Game Menu > **Friends** > **Give Host to ...** |
| Go back in time | Shared Worlds > the world > **Backups...** |
| Talk | Game Menu > **Friends** > **Voice Chat** > **On**, then the microphone button in the top row |

Voice chat is off until you turn it on, and your microphone is off until you tap its button. Each
player has a volume and a mute. Up to 8 players.

## Help

| Problem | What to do |
| --- | --- |
| The `.ipa` is "corrupt" when installing | Download it again in Safari. Its name must end in `.ipa`; if it ends in `.zip`, rename it, don't unzip it |
| The app won't open after a week | A free Apple ID signs apps for 7 days. Refresh it in your sideloading app |
| Low frame rate or a hot device | Lower the render distance, and use Starling Visuals or no shader pack |
| The touch layout is a mess | Control Hub > Controls > presets > **Default** |
| A swipe at the bottom edge leaves the game | The first swipe only lights up the home bar; it takes a second one to leave. For a full lock use iOS's Guided Access |
| A build of mods failed | One of the mods did not fit this version. Turn it off and build again |
| Something crashed or looks wrong | Options > Diagnostics > **Export diagnostic ZIP**, then [open an issue](https://github.com/shopifygav222/horizon-pocket-edition-ios/issues/new/choose) with it |

## For developers

No Mac is needed: GitHub builds the app.

| Branch | Edition |
| --- | --- |
| [`main`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/main) | Horizon PE |
| [`shared-worlds`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/shared-worlds) | `main` plus shared worlds and voice chat |
| [`shared-worlds-mods`](https://github.com/shopifygav222/horizon-pocket-edition-ios/tree/shared-worlds-mods) | `shared-worlds` plus the fifteen mods |

- **Build the app:** run the [`ios`](.github/workflows/ios.yml) workflow with **Make and keep the
  unsigned .ipa**.
- **The iOS app in depth:** [docs/ios.md](docs/ios.md).
- **What differs from Starling, and how to take in a new Starling release:**
  [IOS-SYNC.md](IOS-SYNC.md).
- **Run your own mod build service** for a fork:
  [packaging/mod-builds-worker](packaging/mod-builds-worker/README.md).

## Credits

- **[Horizon](https://github.com/ludovic111/horizon)** by Ludovic and contributors: Minecraft:
  Java Edition rewritten in Rust, the game this app ports.
- **[Starling](https://github.com/starling-client/starling)** by
  [JuanLuci](https://github.com/JuanLuci) and [Vasysik](https://github.com/Vasysik): Horizon's
  Android port, which this app is based on. The world screenshots are Starling's.
- **[horizon-iOS](https://github.com/meehandaniel4/horizon-iOS)**: the first native mod builds for
  iOS on GitHub Actions.
- **[Opus](https://opus-codec.org)** and **[RNNoise](https://github.com/xiph/rnnoise)**: the voice
  codec and noise filter of the Shared Worlds editions.
- **The built-in mods' authors:**
  [their names, sources and licences](docs/shared-worlds-mods.md#the-mods-their-authors-and-licences).

## License

MIT, as Horizon and Starling: see [LICENSE](LICENSE) and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
