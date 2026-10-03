# snout-stack

The whole SnoutData stack on one machine with Docker Compose: a Postgres project with auth, a REST
and GraphQL data API, file storage, Realtime and functions, behind one gateway on one port. The
same servers SnoutData Cloud runs, in the same arrangement, holding one project.

The JavaScript, Dart, Swift and Python client libraries your application may already use for this
kind of API work against it unchanged: point them at the gateway with the anon key.

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
| `metadata`, `setup`, `functions-deploy` | the shared servers' own small database, and two one-shot steps |

**Size (estimated):** about 200 MB of memory running, and about 2 GB of disk for the images.

## Quickstart

You need Docker with the Compose plugin (2.24 or later).

```sh
git clone https://github.com/snoutdata/snout-stack && cd snout-stack
docker run --rm ghcr.io/snoutdata/snout-stack:0.1.3 init > .env
docker compose up -d --wait
```

`init` writes every key and password the stack needs into `.env`, made for this stack alone.
There is no example key anywhere: without `.env`, `docker compose up` stops and says which value
is missing. Keep `.env` out of version control and back it up with your data; it holds the keys
your clients were given.

The API is at `http://localhost:8000`. The two keys are in `.env`:

- `ANON_KEY` ships in your application. What it can reach is what your row-level security
  policies grant the `anon` and `authenticated` roles.
- `SERVICE_ROLE_KEY` bypasses row-level security. Keep it on your servers.

```js
const client = createClient('http://localhost:8000', process.env.ANON_KEY)
```

| Path | Service |
|---|---|
| `/rest/v1/` | the data API |
| `/graphql/v1` | GraphQL |
| `/auth/v1/` | auth |
| `/storage/v1/` | storage |
| `/realtime/v1/` | Realtime (WebSocket, and `api/broadcast` over HTTP) |
| `/functions/v1/<name>` | your functions |

The database itself is on `127.0.0.1:5432`: database `<SNOUT_REF>`, user `<SNOUT_REF>_owner`,
password `POSTGRES_OWNER_PASSWORD`. The owner can do everything in its database, including
creating the extensions the image offers, and is not a superuser.

## Functions

A function is a folder: `functions/<name>/index.ts`, served at `/functions/v1/<name>`. Code shared
between functions goes in `functions/_shared/` and is imported by relative path. `npm:`, `jsr:`
and URL imports are resolved when you deploy, never on a request. `functions/hello` is an example.

```sh
docker compose run --rm functions-deploy     # after any change; the runtime picks it up at once
```

Every function gets `SNOUTDATA_URL`, `SNOUTDATA_ANON_KEY` and `SNOUTDATA_SERVICE_ROLE_KEY`. Your
own secrets go in `functions/.env` (`NAME=value` lines), which reach the functions as variables
and are never copied into a bundle. A call needs the anon or service key unless the function is
listed in `FUNCTIONS_NO_VERIFY_JWT` (a webhook's receiver).

The functions container sits on a network of its own with the gateway: a function reaches the
internet and this stack's API, and not the databases or the servers' admin ports.

## Configuration

Everything is in `.env`. `init` writes the secrets; the rest have defaults and can be added.

| Variable | Default | |
|---|---|---|
| `API_EXTERNAL_URL` | `http://localhost:8000` | Where clients reach the gateway: put your public `https://` address here |
| `SITE_URL` | `http://localhost:3000` | Your application, where a user lands after a sign-in or confirmation link |
| `API_BIND`, `API_PORT` | `0.0.0.0`, `8000` | Where the gateway is published |
| `DB_BIND`, `DB_PORT` | `127.0.0.1`, `5432` | Where Postgres is published |
| `SNOUT_POD_IMAGE` | `ghcr.io/snoutdata/snoutpod-postgres:18` | The database image. A stack set up on Postgres 17 sets `ghcr.io/snoutdata/snoutpod-postgres:17` here (see Upgrades) |
| `COMPOSE_PROFILES` | `objects,images` | Remove `objects` to keep files in your own S3; remove `images` to run without image resizing |
| `S3_ENDPOINT`, `S3_BUCKET`, `S3_REGION` | the bundled store, `stack`, `us-east-1` | Your own S3 (AWS, R2, ...), with `S3_ACCESS_KEY`, `S3_SECRET_KEY` and `S3_CREATE_BUCKET=false`; make the bucket first |
| `S3_FORCE_PATH_STYLE` | `true` | |
| `STORAGE_FILE_SIZE_LIMIT` | `52428800` | The largest upload, in bytes |
| `AUTH_MAILER_AUTOCONFIRM` | `false` | Sign-ups are confirmed without a mail. Until you set up mail, nobody can confirm one |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASS`, `SMTP_ADMIN_EMAIL`, `SMTP_SENDER_NAME` | none, `587` | The mail server for confirmations, links and codes |
| `AUTH_DISABLE_SIGNUP` | `false` | Refuse new users (invites still work) |
| `AUTH_URI_ALLOW_LIST` | none | Other redirect targets, comma separated globs |
| `AUTH_JWT_EXP` | `3600` | Access token lifetime, in seconds |
| `GOOGLE_ENABLED`, `GOOGLE_CLIENT_ID`, `GOOGLE_SECRET` | off | Sign in with Google; the redirect URI is `<API_EXTERNAL_URL>/auth/v1/callback` |
| `GITHUB_ENABLED`, `GITHUB_CLIENT_ID`, `GITHUB_SECRET` | off | The same for GitHub |
| `SAML_ENABLED`, `SAML_PRIVATE_KEY` | off | SAML single sign-on (snout-auth's README) |
| `REST_SCHEMAS` | `public, graphql_public` | Schemas the data API serves |
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
- **Upgrades.** Pull this repository's new `compose.yaml` and `docker compose up -d`. The servers
  bring their own schemas up to date when they start. **A Postgres major version is the
  exception:** a data directory written by 17 will not open in 18, so a stack set up on 17 keeps
  `SNOUT_POD_IMAGE=ghcr.io/snoutdata/snoutpod-postgres:17` in `.env`. To move it to 18, dump the
  database, set up a new stack, and restore into it.
- **Exposure.** Only the gateway's port (and Postgres's, on this machine) is published. The
  servers' admin ports and the metadata database are reachable only inside the stack.

## Moving a project here

The `auth`, `storage` and `realtime` schemas are the ones the client libraries' servers have always
used, so a project from another server of the same API moves with its database: restore a dump
into `db` as the owner, copy its files into the bucket under the same keys, and give your clients
the new URL and keys (or keep the old ones by setting `JWT_SECRET`, `ANON_KEY` and
`SERVICE_ROLE_KEY` in `.env` to the old values before the first `up`).

## The `snout-stack` binary

| Command | |
|---|---|
| `snout-stack init [--out <file>] [--force]` | Write a fresh `.env` (to standard output, or a file it will not overwrite without `--force`) |
| `snout-stack setup` | The one-shot that prepares the database and registers the project with storage and Realtime |
| `snout-stack gateway` | The front door |
| `snout-stack functions [--source <dir>] [--out <dir>]` | Lay `functions/` out for the runtime and bundle each function |

## Licence

[Apache License 2.0](./LICENSE). Security reports: [SECURITY.md](./SECURITY.md).
