# Smoke tests

End-to-end checks of Connect with a real console: a Forge on this computer (**A**) and the PS5
payload (**B**), paired and driven over their HTTP APIs. Each case is a job or route a user
reaches from the UI. Plain node (18 or newer), no dependencies, nothing personal in the files: every
address comes from the environment. The suite keeps to the critical paths (at most 30 cases) and
takes 10–15 minutes against a console.

Every script is in `scripts/`:

| File | What |
|---|---|
| `smoke.mjs` | the suite (26 cases, run in order; later ones use earlier ones' files) |
| `lib.mjs` | HTTP calls with retries, job start/wait, tree hashes |
| `gen-game.mjs` | a fake, deterministic game folder (≤ 1 GiB) |
| `dry-run.sh` | the suite against two Forges on this computer, no console |
| `nas.sh` | a NAS for cases 2 and 18–20: Samba in Docker, a guest share with a generated game and a writable `out` |
| `ftp.sh` | an FTP server for cases 2 and 21–23: vsftpd in Docker, user `forge`/`forge`, a generated game and a writable `out`; idle sessions closed after 20 s (`FTP_IDLE`) |
| `nfs.sh` | NFS servers for cases 2, 24 and 25: unfs3 (NFSv3, port 12049) and NFS-Ganesha (NFSv4.1, port 12050) in Docker, each exporting `/share` with a generated game and a writable `out` |

## Run on the console

1. Optional, for the NAS cases (18–20, skipped without it): `tests/scripts/nas.sh up` (Docker; port
   1445), then `export FORGE_NAS=<this computer's LAN IP>:1445` (the console must reach it) and the
   `FORGE_NAS_DIR` it prints. `tests/scripts/nas.sh down` stops it. For the FTP cases (21–23):
   `tests/scripts/ftp.sh up 2121 <this computer's LAN IP>` (port 2121, passive ports 21100–21110) and
   the two exports it prints; `tests/scripts/ftp.sh down`. For the NFS cases (24–25):
   `tests/scripts/nfs.sh up <this computer's LAN IP>` and the exports it prints;
   `tests/scripts/nfs.sh down`. Case 2 probes each server that is set. Only these containers: setup
   refuses a `FORGE_NAS`, `FORGE_FTP`, `FORGE_NFS3` or `FORGE_NFS4` that isn't this computer's address
   with the container's published port, so the suite never writes to a real NAS or server.
2. Build and load this branch's payload (`ps5/build.sh`, then send `target/ps5/ps5-dump-forge.elf`
   to the console's ELF loader) and start a Forge here from the same commit:
   `cargo build --release -p ps5-dump-forge-cli && target/release/ps5-dump-forge serve --root ~`.
   Both must be the same version (setup checks).
3. Run, with the console's address:

   ```sh
   FORGE_B=http://<console-ip>:8095 node tests/scripts/smoke.mjs
   node tests/scripts/smoke.mjs --list              # the cases
   FORGE_B=... node tests/scripts/smoke.mjs --only 1,3,8,10,11 --keep
   ```

   `--only` takes numbers and ranges; every case needs 1 (it pairs and makes the run folder on B), most
   need the push (3), and some use outputs of earlier cases in the same run: 6 and 7 need 5, 10 and 16
   need 8, 11 needs 10 (so 8 too). Each invocation makes new run folders, so a case can't use an
   earlier run's outputs: list the cases it needs with it. `--keep` keeps the folders for a look.
   Setup checks both Forges answer, run the same version and have their own key ids.

The console needs ~1 GB free in `/data`. The games are generated once into `A_WORK` and
reused. A run writes `forge-smoke-<timestamp>/` in `A_WORK` and in `B_PARENT`; both are deleted when
every case passes (`--keep` keeps them), kept when one fails.

| Variable | Default | |
|---|---|---|
| `FORGE_B` | (required) | the console's Forge, `http://<ip>:8095` |
| `FORGE_A` | `http://127.0.0.1:8095` | the Forge on this computer |
| `A_SHARE` | `/` | A's share root (a Forge shares its disk's root; peers' `flink://` paths are relative to it) |
| `A_WORK` | `$HOME/forge-smoke-work` | games and run folders on A; must be inside `A_SHARE` |
| `B_SHARE` | `/` | B's share root |
| `B_PARENT` | `/data` on a console | where B's run folder goes |
| `FORGE_NAS` | (unset: NAS cases skipped) | the NAS as both Forges reach it, `host:port` |
| `FORGE_NAS_DIR` | `$TMPDIR/forge-smoke-nas` | the share's folder here (`nas.sh`'s), to hash against |
| `FORGE_FTP` | (unset: FTP cases skipped) | the FTP server as both Forges reach it, `host:port` (`ftp.sh`'s) |
| `FORGE_FTP_DIR` | `$TMPDIR/forge-smoke-ftp` | the server's folder here (`ftp.sh`'s) |
| `FORGE_NFS3` | (unset: case 24 skipped) | the NFSv3 server as both Forges reach it, `host:port` (`nfs.sh`'s) |
| `FORGE_NFS4` | (unset: case 25 skipped) | the NFSv4.1 server, `host:port` (`nfs.sh`'s; its export is a Docker volume, read with `docker cp`) |
| `FORGE_NFS_DIR` | `$TMPDIR/forge-smoke-nfs` | the NFSv3 export's folder here (`nfs.sh`'s) |

The pairing stays when the run ends (case 26 forgets it and pairs again). Case 1 has one request
denied (5 denied or unanswered requests per address in 10 minutes lock that address out), so don't
loop the suite more than a few times in 10 minutes.

## Dry run without a console

```sh
cargo build --release -p ps5-dump-forge-cli
tests/scripts/dry-run.sh [--only ...] [--keep]
```

With the servers up: `FORGE_NAS=127.0.0.1:1445`, `FORGE_FTP=127.0.0.1:2121`,
`FORGE_NFS3=127.0.0.1:12049`, `FORGE_NFS4=127.0.0.1:12050` (or this computer's LAN IP, the FTP
server's passive address). `FORGE_BIN` picks another build (one not running elsewhere). Starts two
Forges with their own temporary homes (ports 18095/18096, FLINK 19120/19121; `DRY_RUN_PORT`,
`DRY_RUN_FLINK` move them), so the app's real pairings are untouched; both share `/`, the run's files
stay in the temporary folder. Case 2's probes also ask UDP 8094 on this computer (a Forge's discovery
port), as Add a device does.

## Cases

The games: `game-ampr` (AMPR, ~135 MB, 300 files and an 80 MiB `data/big.bin`), `game-1k` (1,000
small files), `game-lz4` (AMPR, ~17 MB), `game-small` (~13 MB), and each server's own (~40 MB).

| # | Case | Covers |
|---|---|---|
| 1 | pairing | B's user (over B's API) denies a request: nothing saved; allows one once both check numbers match: both sides online; the run folder on B |
| 2 | Pair by address probe | `connect/probe` (Connect → Add a device): A finds B and B finds A as Forges (name, FLINK port; on a console B's bare IP too), both find each test server as `smb`, `ftp` or `nfs` |
| 3 | push A → B | transfer job writing a peer folder (8 lanes; small files in one `put`) |
| 4 | pull B → A | transfer job reading a peer folder; sha256 of every file |
| 5 | B pulls 1k files from A | the console as the FLINK client, per-file cost |
| 6 | fpkg on B | 1k-file game into a `.pkg` on the console |
| 7 | fpkg on A from B | a peer folder (1k files) streamed into a `.pkg`, extracted back and hashed |
| 8 | convert A → B | `.ffpkg` and an fpkg written on the peer (`.part`, read-back verify, rename; the fpkg builder writes and reads back through FLINK) |
| 9 | convert on B from A | the console reads a peer game into an `.exfat`, full verify |
| 10 | edit on B | in place: add from `flink://`, rename, delete, `edit_check` |
| 11 | extract on B into A | the edited files written onto the peer, hashed |
| 12 | LZ4 pack on B | AMPR game packed into an `.ffpkg` on the console |
| 13 | LZ4 pack peer → peer | two-pass pack of `game-lz4` from a peer into a peer folder, unpacked and hashed |
| 14 | browser upload/download | the Transfer screen's This browser pane (upload, Download N files): `connect/upload` in 8 MiB chunks, 4 at once; `connect/download` |
| 15 | cancel a transfer | nothing half-written left on the peer |
| 16 | refusals | editing/deleting peer paths, a `.pkg` source on a peer, taken names, escaping the share root |
| 17 | job slots | a transfer (1k files) beside a convert; a transfer of the converting game refused |
| 18 | NAS saved | guest NAS saved on both, online, its shares and game listed over SMB |
| 19 | NAS games, refusals | the console converts (`.ffpkg`) and copies a NAS game; A copies one, hashed; no edit, a share root not deleted (NAS folders can be deleted now) |
| 20 | NAS outputs | A writes an `.exfat`, an fpkg (streamed: digested as written, nothing staged here) and a transfer onto the share's `out`; each converted back here and hashed against the source; forgotten on both |
| 21 | FTP saved, fpkg | FTP server saved on both with a login, online, listed; the console converts its game; A writes an fpkg onto it (the rewrite probe, `REST` + `STOR`), extracted and compared |
| 22 | FTP outputs | the console writes an `.exfat` and a transfer onto it, hashed; no edit, its root not deleted |
| 23 | FTP stale session | after the server closed both Forges' idle sessions (`421`), a transfer from it on each side logs in again; forgotten on both |
| 24 | NFSv3 | saved on both (no login), online, export and game listed; the console converts its game; A writes an `.ffpkg` onto `out`, hashed; no edit, the export not deleted; forgotten |
| 25 | NFSv4.1 | the same against Ganesha (the image fetched with `docker cp`) |
| 26 | forget, pair again | asking while B still has A refused (already paired); forgotten on both, paired again: one entry per side |

Failures print the job's error and its last log lines; the Forges' own logs have the rest (the
console's in `/data/dumpforge/serve-<pid>.txt`). Network calls are retried (6 tries, backoff) when the
connection drops, and a `start_job` whose reply was lost is looked up in the job list, never sent twice.
