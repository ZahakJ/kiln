# Kiln collaboration relay

`kiln-collab-server` relays edits between the people working on a shared Kiln document and keeps each shared document (a room) in a small database, so a late joiner gets the whole document and edits made offline merge when their author reconnects. It never needs the document's file: it holds the merged editing history.

## Run

```sh
cargo run --release -p kiln-collab-server -- --token "$(openssl rand -hex 24)"
```

| Flag | Default | |
|---|---|---|
| `--bind <ADDR>` | `127.0.0.1:7070` | Address to listen on |
| `--data <DIR>` | `./kiln-collab-data` | Directory of the room database (`rooms.redb`) |
| `--token <TOKEN>` | `KILN_COLLAB_TOKEN` | The shared token clients present |

Rooms are at `ws://<ADDR>/rooms/<ROOM>`; `GET /health` answers `ok`. A share link carries the room, the relay's address and the token (`?doc=<room>&server=<url>&token=<token>`).

With Docker, from this directory:

```sh
KILN_COLLAB_TOKEN=<token> docker compose up -d
```

or build the image from the repository root with `docker build -f apps/collab-server/Dockerfile -t kiln-collab-server .`.

## Security

The shared token admits anyone who holds it, to every room: it is meant for a team on its own network, not for the open internet. Serve the relay behind a TLS proxy (`wss://`), keep the token secret, and rotate it by restarting with a new one. For accounts or per-room access, implement `kiln_collab_server::Authenticate` and pass it to `Relay::new`.

## Storage

Every update is written to `rooms.redb` before it is merged and passed on; one that cannot be written is refused, and the sender sends it again on reconnecting. A room's updates are folded into a snapshot every 256 updates and when its last client leaves; back up the data directory to keep the rooms. A room gives each new person an id lane of its own and admits about a million over its life.
