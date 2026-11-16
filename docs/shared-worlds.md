# Shared worlds and voice chat

> **Only in Horizon PE: Shared Worlds.** Download it from the
> [README](../README.md#download). The standard Horizon PE doesn't have these features.

**Contents:** [What a shared world is](#what-a-shared-world-is) ·
[Set up the storage](#set-up-the-storage-once-about-10-minutes-on-your-phone) · [Playing](#playing) ·
[Reaching the host](#reaching-the-host) · [Backups](#backups) · [Voice chat](#voice-chat) ·
[Good to know](#good-to-know)

## What a shared world is

A shared world belongs to your group, not to one of you. Whoever opens it while nobody is
playing hosts it on their own phone; friends join them with no server. When the host leaves,
the world moves to a player who is still in it, and everyone else follows by themselves. The
title screen's **Shared Worlds** (where Realms was) lists your shared worlds and how many are in
each one right now.

The world always runs on the host's phone, as single player does, so it plays as fast. A copy
of it lives in a storage bucket your group owns, which is how the next host gets the newest
version. Sign in with a **Microsoft account** if you can: the host's game checks it, so your
inventory, pets and stats stay yours whoever hosts, and nobody can play as you. Friends who
aren't signed in (Play Offline) can join and host too, under their name: their things are kept
by that name, so anyone with the world invite could use the same name.

## Set up the storage once (about 10 minutes, on your phone)

The world's copy goes in **Cloudflare R2**: 10 GB free, which holds many worlds. R2 asks for a
payment card when you turn it on; nothing is charged within the free amounts, and a group's
playing stays far inside them.

1. Open **dash.cloudflare.com** in your phone's browser and sign up (free).
2. Go to **R2 Object Storage**, turn it on, then **Create bucket**. Any name (`horizon-worlds`),
   location Automatic.
3. In R2, open **Manage API tokens** (API, Manage API tokens) and **Create API token**: permission
   **Object Read & Write**, applied to that bucket only. Create it, and keep the page open: it shows
   the **Access Key ID**, the **Secret Access Key** and the **S3 endpoint** (`https://...r2.cloudflarestorage.com`)
   once.
4. In the game: **Shared Worlds**, **Share a World...**, pick the world, then paste the endpoint, the
   bucket's name and the two keys (copy each from the browser, then **Paste** in the game), and
   **Share World**. The game first checks that the bucket keeps a world safe, then uploads it.
5. Open the world in the list, **Copy World Invite**, and send it to your friends in a private
   message. They tap **Add with Invite...**, **Paste**, **Add World**.

The invite holds the bucket's key: anyone who has it can play and change the world. Send it only
to the friends you play with. A second world can go in the same bucket: the game remembers it.

## Playing

- **Start** when nobody is playing: you host. **Join** when someone is: you join them.
- **Leaving as the host:** Save and Quit. The world uploads (a moment; keep the app open until it
  says so) and passes to a player who is still in it. Their game takes it and the others follow.
- **The Friends button** (Game Menu), for everyone in the world: who hosts, who is in it and how
  each one connects, anything going on (a slow route and why, a backup to load), the buttons to
  answer, **Give Host to** a player (the host) or **Ask to Host** (a guest), and **Copy Invite**:
  the world's invite, for a friend to add the world.
- **Give Host / Ask to Host:** the host's world closes and opens on the other player's phone in a
  few seconds, and everyone follows by themselves.
- **While you host,** the world's copy updates every 5 minutes (or as often as the world's owner
  chose), and whenever your app goes to the background, without stopping play.
- **If the host's app goes to the background,** the list says so at once ("in the background"),
  and nobody is shown playing.
- **If the host's phone dies or loses its connection,** the others wait about half a minute in case
  it comes back, then one of them takes the world from its last copy (at most one backup interval old, 5 minutes unless the owner chose otherwise). Nothing is damaged. What the old host did after that copy stays on their phone, as
  a separate singleplayer world named "(my own progress)".
- **If your phone can't reach the storage while you host,** the world pauses until it can, so
  nobody can take it over while you keep playing.

## Reaching the host

Each friend's game connects straight to the host's whenever it can, which is the fastest way:

1. **Directly:** on the same Wi-Fi or hotspot, through a router that opens a port (the game asks
   it by UPnP or NAT-PMP), or over a VPN such as Tailscale.
2. **Both at once:** when the host can't be called into (UPnP is off, or the host is on mobile
   data), the two games reach each other through **bore.pub**, a free public tunnel, and from
   there both dial the other at the same moment. Over IPv6 (most mobile networks and many home
   connections have it) and through most home routers, the two connections meet and the game goes
   straight between the phones: that friend leaves bore.pub at once.
3. **UDP, only when TCP found nothing:** the two games try once more over UDP, the kind of
   connection video calls use, which gets through far more routers and mobile networks. Each
   sends small signed probes to the other at the same time; when they meet, QUIC (the
   connection layer browsers use) carries the game straight between the phones, encrypted with
   the world's own key as well as QUIC's. This adds a few seconds to that friend's join, and
   nothing at all for anyone who connected directly before it.
4. **bore.pub:** only when none of that works (both behind a carrier's shared address that gives
   a new port for every destination) does a friend play through the tunnel. It can be slower,
   and it is run by volunteers. That friend and the host are told. The world stays encrypted
   between the host and each friend either way.

The Friends screen (Game Menu, Friends) always says how each connection goes: the host sees
every player as "(direct)", "(direct, UDP)" or "(bore.pub)", and each guest sees its own.

Faster for everyone: a host on home Wi-Fi with UPnP on in the router; or, when the host is behind
the tunnel, **Give Host** to a player marked "(faster)", whose network takes direct connections.

## Backups

A copy of the world goes up every 5 minutes while it is played, and when its host leaves. The
player who shared the world can choose how often on its Backups page (**Backups: every ... min**:
2, 5, 10, 15, 30 or 60 minutes): shorter loses less if a host's phone dies, longer pauses the
world less often for the save. Every host follows the choice within a minute. The
storage keeps, as backups: the last 20 copies (the last hour or so of play), one a day for 30
days, and one a week for 26 weeks. Copies share what didn't change, so each backup costs only
the blocks that did. The whole bucket (every world in it) is kept under 8 GB, well inside R2's
free 10 GB: when it would go over, the oldest backups go first (never the newest few, the world
as it is now, or the copy a load could be undone to).

- **Load a backup:** Shared Worlds, the world, **Backups...**, pick one. Anyone in the world can
  ask; the player who shared it approves it (Backups..., **Approve Load**, or in the world: Game
  Menu, Friends). Their own requests need no approval.
- **When it loads:** at once if nobody is playing. If someone is, their game warns everyone,
  closes the world, loads the backup and opens it again; the others join again by themselves.
- **Undo:** the copy a load replaced is always kept: **Undo Last Load** brings it back, the
  same way (asked, then approved).
- What was played after a backup is never lost by loading it: it stays a backup itself.

## Voice chat

Talk to the players of a shared world from inside the game, without a call running beside it.

- **Off until you turn it on.** Joining a world never plays anyone's voice and never touches
  your microphone. When others are in the world, a gold line in chat says voice chat is there;
  when one of them turns it on, another says who.
- **Turn it on:** Game Menu, **Friends**, **Voice Chat**, **Voice Chat: On**. You hear everyone who
  has it on, wherever they are in the world. Your microphone is still off.
- **Your microphone:** the microphone button at the left of the top row of the touch controls
  (red while muted), or **Microphone** on the Voice Chat page. The first time, iOS asks whether
  the game may use the microphone. Move, resize or hide the button in the controls editor like
  the others.
- **Each player's volume:** on the Voice Chat page every player has a slider from 0 to 200 % and
  a **Mute** button; **Voices** sets everyone together. What you choose is remembered for that
  player in every world.
- **Which microphone:** **Mic** on the same page. Automatic uses a wired headset's microphone when
  one is plugged in, and otherwise the device's own, so Bluetooth headphones keep playing in
  full quality. A Bluetooth headset's own microphone can be chosen, at a call's sound quality
  for everything (that is how Bluetooth works).
- **Noise Filter** (the foot of the Voice Chat page) turns down a room's steady noise, a fan or
  traffic, in your microphone. It is off unless you turn it on: it uses a little more battery.
- **Who is talking** shows in the top left corner while voice chat is on.
- **It turns itself off** when your headphones disconnect, so nobody is suddenly heard out loud;
  your microphone goes off when a call or Siri takes the sound, and when the game leaves the
  screen. It follows the world to its next host by itself.
- **While your microphone is on,** the ring/silent switch doesn't silence the game: iOS treats a
  live microphone like a call. It does again a few seconds after you mute.
- **What it costs:** nothing while nobody talks, and about 60 kbit/s for each person talking.
  Voices travel on the connection the game already has, encrypted like the world.

## Good to know

- 8 players at most in a world.
- Removing a world from your list leaves it in the storage for the others; your copy becomes a
  normal singleplayer world.
- The game's own **How to Set Up...** page (on Share a World's storage page) has the setup steps
  too, with an **Open Cloudflare** button.
- Shared worlds are between phones running the Shared Worlds edition. Friends on PC (official
  Horizon) are planned for later.
