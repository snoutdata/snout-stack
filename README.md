# snout-stack

The whole SnoutData stack on one machine with Docker Compose: a Postgres project with auth, a REST
and GraphQL data API, file storage, Realtime and functions, behind one gateway on one port. The
same servers SnoutData Cloud runs, in the same arrangement, holding one project.

The JavaScript, Dart, Swift and Python client libraries your application may already use for this
kind of API work against it unchanged: point them at the gateway with the app key.

| Service | What it is |
|---|---|
| `db` | Postgres 18 with pgvector, PostGIS, pg_cron, pg_net, pg_graphql, SnoutTime and the rest of the project image's extensions |
| `auth` | [snout-auth](https://github.com/snoutdata/snout-auth): sign-up, sign-in, sessions, MFA, Google, GitHub and SAML |
| `rest` | [PostgREST](https://postgrest.org): the REST API over your schema; GraphQL is a Postgres function it calls |
| `storage` | [snout-storage](https://github.com/snoutdata/snout-storage): buckets and files, authorised by your row-level security |
| `realtime` | [snout-realtime](https://github.com/snoutdata/snout-realtime): broadcast, presence and database changes |
| `functions` | [snout-functions](https://github.com/snoutdata/snout-functions): TypeScript on Deno's web platform, an isolate per function |
| `gateway` | this repository: the one public port, the key check, and the routing |
| `objects` | an S3 API over a folder on this machine, where storage keeps files ([versitygw](https://github.com/versity/versitygw)) |
| `images` | [snout-images](https://github.com/snoutdata/snout-images): image resizing for storage |
| `push` | [snout-push](https://github.com/snoutdata/snout-push): push notifications to iPhone, Android and the web, at `/push/v1` |
| `metadata`, `setup`, `functions-deploy` | the shared servers' own small database, and two one-shot steps |

**Size (estimated):** about 200 MB of memory running, and about 2 GB of disk for the images.

## Quickstart

You need Docker with the Compose plugin (2.24 or later).

```sh
git clone https://github.com/snoutdata/snout-stack && cd snout-stack
docker run --rm ghcr.io/snoutdata/snout-stack:0.1.4 init > .env
docker compose up -d --wait
```

`init` writes every key and password the stack needs into `.env`, made for this stack alone.
There is no example key anywhere: without `.env`, `docker compose up` stops and says which value
is missing. Keep `.env` out of version control and back it up with your data; it holds the keys
your clients were given.

The API is at `http://localhost:8000`. The two keys are in `.env`:

- `APP_KEY` ships in your application. What it can reach is what your row-level security
  policies grant the `snout_app` and `snout_member` roles.
- `SERVER_KEY` bypasses row-level security. Keep it on your servers.

```js
const client = snout('http://localhost:8000', process.env.APP_KEY)
```

| Path | Service |
|---|---|
| `/v1/data/` | the data API |
| `/v1/graph` | GraphQL |
| `/v1/identity/` | auth |
| `/v1/files/` | storage |
| `/v1/live/` | Realtime (WebSocket, and `api/broadcast` over HTTP) |
| `/v1/fn/<name>` | your functions |

The database itself is on `127.0.0.1:5432`: database `<SNOUT_REF>`, user `<SNOUT_REF>_owner`,
password `POSTGRES_OWNER_PASSWORD`. The owner can do everything in its database, including
creating the extensions the image offers, and is not a superuser.

## Functions

A function is a folder: `functions/<name>/index.ts`, served at `/v1/fn/<name>`. Code shared
between functions goes in `functions/_shared/` and is imported by relative path. `npm:`, `jsr:`
and URL imports are resolved when you deploy, never on a request. `functions/hello` is an example.

```sh
docker compose run --rm functions-deploy     # after any change; the runtime picks it up at once
```

Every function gets `SNOUTDATA_URL`, `SNOUTDATA_APP_KEY` and `SNOUTDATA_SERVER_KEY`. Your
own secrets go in `functions/.env` (`NAME=value` lines), which reach the functions as variables
and are never copied into a bundle. A call needs the app or server key unless the function is
listed in `FUNCTIONS_NO_VERIFY_JWT` (a webhook's receiver).

The functions container sits on a network of its own with the gateway: a function reaches the
internet and this stack's API, and not the databases or the servers' admin ports.

## Containers

Your own Docker images, running all the time beside the project, each under
[gVisor](https://gvisor.dev) with its own memory and CPU limits and a network of its own:
`snout-containers`, in the `containers` profile (not on by default). It needs an engine on this
machine first: gVisor, a small runtime wrapper, a podman configuration and a rootless podman service,
which [snout-containers' README](https://github.com/snoutdata/snout-containers#the-engine) sets out
step by step. Then add `containers` to `COMPOSE_PROFILES`, set `SNOUT_CONTAINERS_RUNTIME_DIR` to that
user's runtime directory (`/run/user/<uid>`), and describe what to run in
`containers/manifests/<project>.json`:

```json
{
  "project": "local",
  "limits": { "memoryMb": 256, "cpuPercent": 50 },
  "containers": [
    { "name": "bridge", "image": "ghcr.io/you/bridge:1.0", "port": 8080, "hostPort": 46000,
      "env": { "DATABASE_URL": "postgres://…" } }
  ]
}
```

`containers/state/status.json` says how each one is, and `docker compose logs containers` why.
A container is stateless: anything it writes inside itself is lost on a restart, so it keeps what
matters in the database. Tested with rootless podman as the engine's user; on Docker the runner
needs that user's runtime directory mounted as shown in `compose.yaml`.

## Configuration

Everything is in `.env`. `init` writes the secrets; the rest have defaults and can be added.

| Variable | Default | |
|---|---|---|
| `API_EXTERNAL_URL` | `http://localhost:8000` | Where clients reach the gateway: put your public `https://` address here |
| `SITE_URL` | `http://localhost:3000` | Your application, where a user lands after a sign-in or confirmation link |
| `API_BIND`, `API_PORT` | `0.0.0.0`, `8000` | Where the gateway is published |
| `DB_BIND`, `DB_PORT` | `127.0.0.1`, `5432` | Where Postgres is published |
| `SNOUT_POD_IMAGE` | `ghcr.io/snoutdata/snoutpod-postgres:18` | The database image. A stack set up on Postgres 17 sets `ghcr.io/snoutdata/snoutpod-postgres:17` here and keeps it (see Upgrades) |
| `COMPOSE_PROFILES` | `objects,images,push` | Remove `objects` to keep files in your own S3; remove `images` to run without image resizing; remove `push` to run without push notifications; add `containers` to run your own images (Containers, above) |
| `S3_ENDPOINT`, `S3_BUCKET`, `S3_REGION` | the bundled store, `stack`, `us-east-1` | Your own S3 (AWS, R2, ...), with `S3_ACCESS_KEY`, `S3_SECRET_KEY` and `S3_CREATE_BUCKET=false`; make the bucket first |
| `S3_FORCE_PATH_STYLE` | `true` | |
| `STORAGE_FILE_SIZE_LIMIT` | `52428800` | The largest upload, in bytes |
| `AUTH_MAILER_AUTOCONFIRM` | `false` | Sign-ups are confirmed without a mail. Until you set up mail, nobody can confirm one |
| `AUTH_ANONYMOUS_USERS_ENABLED` | `false` | Guest sign-in: `guestSignIn()` signs in a user with no email or password, 30 per caller per hour |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, `SMTP_ADMIN_EMAIL`, `SMTP_SENDER_NAME` | none, `587` | The mail server for confirmations, links and codes |
| `AUTH_DISABLE_SIGNUP` | `false` | Refuse new users (invites still work) |
| `AUTH_URI_ALLOW_LIST` | none | Other redirect targets, comma separated globs |
| `AUTH_JWT_EXP` | `3600` | Access token lifetime, in seconds |
| `GOOGLE_ENABLED`, `GOOGLE_CLIENT_ID`, `GOOGLE_SECRET` | off | Sign in with Google; the redirect URI is `<API_EXTERNAL_URL>/auth/v1/callback` |
| `GITHUB_ENABLED`, `GITHUB_CLIENT_ID`, `GITHUB_SECRET` | off | The same for GitHub |
| `SAML_ENABLED`, `SAML_PRIVATE_KEY` | off | SAML single sign-on (snout-auth's README) |
| `REST_SCHEMAS` | `public, snout_graph` | Schemas the data API serves |
| `REST_DB_POOL` | `10` | The data API's connections |
| `FUNCTIONS_MEMORY_MB`, `FUNCTIONS_WALL_MS`, `FUNCTIONS_CPU_MS` | `256`, `60000`, `5000` | Each function's limits |
| `FUNCTIONS_NO_VERIFY_JWT` | none | Functions callable without a key, comma separated |
| `GATEWAY_CLIENT_ADDRESS_HEADER` | none | Behind a proxy, the header holding the caller's address (`x-real-ip`) |
| `DB_SHARED_BUFFERS`, `DB_WORK_MEM` | `128MB`, `4MB` | Postgres memory |
| `LOG_LEVEL` | `info` | |

Each server's own page lists the rest of its settings.

## Running it in production

- **TLS.** Put a TLS proxy in front of the gateway (Caddy, nginx, a load balancer), set
  `API_EXTERNAL_URL` to the `https://` address, `API_BIND=127.0.0.1` so only the proxy reaches
  the gateway, and `GATEWAY_CLIENT_ADDRESS_HEADER` to the header the proxy writes the caller's
  address into, so auth's per-caller limits see callers and not the proxy. Realtime needs the proxy
  to pass WebSocket upgrades.
- **Mail.** Set the `SMTP_*` values and leave `AUTH_MAILER_AUTOCONFIRM` off.
- **Backups.** The data is in four volumes: `db-data` (the project), `metadata-data`,
  `objects-data` (the files, unless they are in your own S3) and `functions-mount` (made again by
  `functions-deploy`). Back up `.env` with them. For the database, a dump while it runs:
  `docker compose exec db pg_dump -U snoutpod_admin -Fc <SNOUT_REF> > project.dump`.
- **Secrets.** `.env` is the only copy of the keys. Changing `JWT_SECRET` retires both keys and
  every session, so do it with new keys from `snout-stack init` and every client updated.
- **Upgrades.** Three commands, in the stack's folder:

  ```sh
  git pull
  docker compose pull
  docker compose up -d --wait
  ```

  `docker compose pull` is the step that is easy to miss. The database image is a moving tag
  (`:18`), and `up` never fetches a tag that is already on the machine, so without it the stack
  keeps the database image it was set up with, however many releases later. The servers bring
  their own schemas up to date when they start, and so does the database: each time it starts, it
  updates our own extensions (SnoutTime) in every database to the versions the image carries, with
  one line per update in `docker compose logs db`. Your data and every other extension stay as
  they are. **A Postgres major version is the exception:** a data directory written by 17 will
  not open in 18, so a stack set up on 17 keeps
  `SNOUT_POD_IMAGE=ghcr.io/snoutdata/snoutpod-postgres:17` in `.env`, and the commands above move
  it to the newest 17 image.
- **Exposure.** Only the gateway's port (and Postgres's, on this machine) is published. The
  servers' admin ports and the metadata database are reachable only inside the stack.

## Moving a project here

The `auth`, `storage` and `realtime` schemas are the ones the client libraries' servers have always
used, so a project from another server of the same API moves with its database: restore a dump
into `db` as the owner, copy its files into the bucket under the same keys, and give your clients
the new URL and keys (or keep the old ones by setting `JWT_SECRET`, `APP_KEY` and
`SERVER_KEY` in `.env` to the old values before the first `up`).

## The `snout-stack` binary

| Command | |
|---|---|
| `snout-stack init [--out <file>] [--force]` | Write a fresh `.env` (to standard output, or a file it will not overwrite without `--force`) |
| `snout-stack setup` | The one-shot that prepares the database and registers the project with storage and Realtime |
| `snout-stack gateway` | The front door |
| `snout-stack functions [--source <dir>] [--out <dir>]` | Lay `functions/` out for the runtime and bundle each function |

## Changelog

Newest first. To take a release, follow Upgrades above: each server is pinned by version in
`compose.yaml`, and your `.env` and volumes stay as they are.

### 2026-10-13: snout-containers 0.1.0, your own images

A new, optional server; nothing else moves. `snout-containers` runs your own Docker images all the
time, each under gVisor with its own limits and network, from a JSON file per project. Add
`containers` to `COMPOSE_PROFILES` once this machine has its engine (Containers, above).

### 2026-10-13: snout-realtime 0.1.9, handlers and Socket.IO

One server moves; `snout-stack` itself is unchanged at 0.1.4.

- **Realtime handlers.** `select snout_live.on('send', 'public.send_message(jsonb)', 'room:*')` makes
  a SQL function run when a client sends that event: as that user, under your row-level security,
  in one transaction, and what it returns is the reply (`channel.call()` in `@snoutdata/client`
  0.3.4). What it sends with `snout_live.emit()` goes out only if it commits.
- **Socket.IO.** The official `socket.io-client` connects unmodified at
  `path: '/v1/live/socket.io/'`, over long-polling and websocket. Your server's listeners are
  the handlers, the ack is the return value, and a room is a Realtime topic.
- **`snout_live.signal()` as the project's owner.** The owner role is granted `snout_live.signals` as
  the API roles are, so a broadcast from the owner's own SQL, triggers and jobs is delivered.

Nothing to do but pull: Realtime adds its handler table to the project database on its next
start. [Realtime](https://docs.snoutdata.com/stack/realtime) and
[Socket.IO](https://docs.snoutdata.com/stack/socketio) have the details.

### 2026-10-12: snout-realtime 0.1.6 and snout-functions 0.2.5

Two servers move; `snout-stack` itself is unchanged at 0.1.4.

- **snout-realtime 0.1.6.**
  - A channel joined without a `join_ref` is known by the join's `ref`, so `phx_close` and system
    messages carry it to a hand-written client.
  - A join whose `config.presence` leaves out `enabled` is sent `roster_state`.
  - A message on a channel the server closed is told why.
- **snout-functions 0.2.5.** What a function prints with `console.log` or `console.error` comes out
  as one JSON line naming the project and function, with the time and level, so
  `docker compose logs functions` says which function printed what. A line is cut at 4 KB, and a
  function printing faster than 50 lines a second (after a burst of 200) has the rest counted
  rather than written.

### 0.1.4, 2026-10-05: security fixes, and push

From a review of every server's code against a checklist of how each kind of server gets broken.
Update if you run an earlier version.

- **snout-auth 0.1.9.**
  - The token in a mailed confirmation, recovery, invite or magic link is keyed with your
    `JWT_SECRET`, so it can't be worked out from the six-digit code, and five wrong guesses spend
    a code through a link as well. Links mailed before the update stop working; the user asks
    for another.
  - A `redirect_to` holding a user name, a backslash or a control character is refused.
  - With `MAILER_AUTOCONFIRM` on, signing up again for an invited or unconfirmed address no
    longer signs in to that account without its password.
  - An address a sign-in provider has not verified never joins an existing account.
  - A SAML provider's users sign in only with an address in the domains registered for it.
- **snout-storage 0.2.3.**
  - A form field other than the file is limited to 1 MiB, and a form to 32 fields, so one upload
    can't exhaust the server's memory.
  - HTML is served as plain text, and SVG and XML with a policy that blocks script.
  - A signed download URL can't upload, and a signed upload URL can't download.
  - A copy is held to the destination bucket's allowed types.
- **snout-realtime 0.1.5.**
  - A connection that stops reading is closed after 30 seconds instead of queueing without end,
    and one that sends nothing for 60 seconds is closed.
  - Presence and message sizes are capped.
  - Presence on a private channel reaches only those your policies let read it.
- **snout-functions 0.2.4.** A function's CPU limit covers its whole run, and function code can't
  write files or follow links out of its folder.
- **The database image** (`snoutpod-postgres`): the extensions that run with elevated rights
  resolve every name from the system catalog only. An existing stack picks this up with the
  Upgrades steps.
- **Push notifications** (`snout-push` 0.1.2) are part of the stack, on by default;
  remove `push` from `COMPOSE_PROFILES` to run without it.

### 0.1.3, 2026-10-04

- snout-auth 0.1.6 (guest sign-in) and snout-realtime 0.1.4.

## Licence

[Apache License 2.0](./LICENSE). Security reports: [SECURITY.md](./SECURITY.md).
