# Horizon PE's mod builds: setting the service up

An iPhone runs no code its app did not ship with, so a catalog mod is compiled into the app. That
build runs on GitHub (this repository's `mod-build` workflow) and takes about half an hour. This
service is what lets a player start one from inside the game with no GitHub account, no fork and no
token: the app asks the service, the service asks GitHub.

It is one small Cloudflare Worker (`worker.js`, about 250 lines) on Cloudflare's free plan. It keeps
no accounts and no record of who asked for what. Its only stored data are counters (how many builds
a device and everyone started today), which expire after two days.

You set it up once. It takes about ten minutes.

## What you need

- The GitHub account that owns the repository the app is built from.
- A free Cloudflare account: <https://dash.cloudflare.com/sign-up>.
- Node.js on a computer (for Cloudflare's `wrangler` tool).

## 1. Make the GitHub token

The service starts builds with a token that can do that and nothing else.

1. Open <https://github.com/settings/personal-access-tokens/new> (a *fine-grained* token).
2. **Repository access**: *Only select repositories*, and pick the app's repository.
3. **Permissions**, under *Repository permissions*: **Actions: Read and write**. Leave everything
   else at *No access* (GitHub adds *Metadata: Read-only* by itself).
4. Give it an expiry you will remember to renew (a year at most), generate it, and copy it. It is
   shown once.

With only this permission the token cannot read or change code, releases, settings or secrets. The
worst someone who stole it could do is start and cancel workflow runs in this one repository.

## 2. Put the Worker on Cloudflare

In a terminal, in this folder (`packaging/mod-builds-worker`):

```sh
npx wrangler login
npx wrangler kv namespace create LIMITS
```

The second command prints an `id`. Put it in `wrangler.toml` in place of
`PUT-THE-NAMESPACE-ID-HERE`. If you build from a fork, also set `REPO` there to `you/your-fork`.
Then:

```sh
npx wrangler secret put GITHUB_TOKEN
npx wrangler deploy
```

The first asks for the token from step 1 (paste it). The second prints the service's address, like
`https://horizon-pe-mod-builds.<your-name>.workers.dev`. Opening it in a browser shows
`{"service":"horizon-pe-mod-builds","api":1}`.

## 3. Tell the app where the service is

The address is built into the app when GitHub builds it:

1. In the repository on GitHub: **Settings > Secrets and variables > Actions > Variables**.
2. **New repository variable**, named `HORIZON_MOD_BUILDS_URL`, with the address from step 2 (no
   slash at the end).

Every build made after that knows the service, including the builds the service itself starts. An
app built without the variable shows no Build button and says that this build has no build service.

## Limits, and changing them

`wrangler.toml` has them: `PER_DEVICE_PER_DAY` (6), `PER_DAY` (300) and `MAX_MODS` (24). Change a
number and run `npx wrangler deploy` again. A build that already exists is found, not started again,
and counts for nothing: only new builds are counted.

GitHub keeps each finished build for seven days. After that, asking for the same mods builds it
again.

## If something goes wrong

| What the game says | What it means |
|---|---|
| "The build service is not set up" | `REPO`, `GITHUB_TOKEN` or the `LIMITS` store is missing: steps 1 and 2 |
| "GitHub did not start the build (401)" or "(403)" | The token expired, or lacks *Actions: Read and write* on the repository |
| "This app's version has no builds with mods" | The app's release tag has no `mod-build` workflow (a release older than 1.1.0), or the tag was deleted |
| "This device has started 6 builds today" | The per-device limit; tomorrow there are more |
| "The build failed at: ..." | The build itself failed at that step. The run is on the repository's **Actions** page, under **mod-build**, named `mod-build <key>` |

To turn the service off, delete the Worker in Cloudflare's dashboard (or `npx wrangler delete`) and
revoke the token on GitHub. The game then says the service can't be reached; nothing else changes.

## Checking it without the game

```sh
node --test worker.test.mjs
```

runs the Worker against a pretend GitHub. Against the real one, with a release tag that has the
workflow and mods from the catalog:

```sh
curl -s -X POST https://<address>/v1/build \
  -d '{"tag":"v1.1.0","mods":["zoom@1.0.0"],"device":"0123456789abcdef0123456789abcdef"}'
curl -s https://<address>/v1/build/<the key it answered with>
```
