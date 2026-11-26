#!/usr/bin/env node
// Connect smoke tests: a Forge on this computer (A) and one on the console (B), driven over their
// HTTP APIs. See tests/README.md. Plain node (>= 18), no dependencies.
//
//   FORGE_B=http://<console>:8095 node tests/scripts/smoke.mjs [--list] [--only 3,5-7] [--keep]
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { call, flink, hashFile, hashTree, remoteSizes, request, run, same, sizeTree, sleep, start, wait } from './lib.mjs';

const env = process.env;
const A = (env.FORGE_A ?? 'http://127.0.0.1:8095').replace(/\/$/, '');
const B = (env.FORGE_B ?? '').replace(/\/$/, '');
const HERE = path.dirname(new URL(import.meta.url).pathname);
const RUN = `forge-smoke-${new Date().toISOString().replace(/\D/g, '').slice(0, 14)}`;

const args = process.argv.slice(2);
const keep = args.includes('--keep');
const only = (() => {
  const i = args.indexOf('--only');
  if (i < 0) return null;
  const set = new Set();
  for (const part of (args[i + 1] ?? '').split(',')) {
    const [a, b = a] = part.split('-').map(Number);
    for (let n = a; n <= b; n++) set.add(n);
  }
  return set;
})();

/** Shared state the tests fill in order. */
const ctx = {};
const tests = [];
const test = (name, fn) => tests.push({ n: tests.length + 1, name, fn });
const ok = (cond, what) => { if (!cond) throw new Error(what); };
class Skip extends Error {}

// ---- setup -------------------------------------------------------------------------------------

async function setup() {
  ok(B, 'set FORGE_B to the console Forge, e.g. FORGE_B=http://<console-ip>:8095');
  const [sa, sb] = [await call(A, 'session'), await call(B, 'session')];
  ctx.sa = sa;
  ctx.sb = sb;
  ok(sa.platform === 'host', `FORGE_A (${A}) must be a Forge on this computer; it says ${sa.platform}`);
  ctx.ps5 = sb.platform === 'ps5';
  if (!ctx.ps5) console.log(`  note: FORGE_B is a ${sb.platform} Forge, not a console (a dry run)`);
  // Where each side's files live, and each side's share root (what flink:// paths are relative to).
  // A Forge shares its disk's root with paired devices (a Windows PC: its home folder's drive).
  ctx.aShare = env.A_SHARE ?? '/';
  ctx.aWork = path.resolve(env.A_WORK ?? path.join(os.homedir(), 'forge-smoke-work'));
  ctx.bShare = env.B_SHARE ?? '/';
  ctx.bParent = env.B_PARENT ?? (ctx.ps5 ? '/data' : null);
  ok(ctx.bParent, 'FORGE_B is not a console: set B_PARENT (a folder there for the run)');
  ctx.aRun = path.join(ctx.aWork, RUN);
  ctx.bRun = path.posix.join(ctx.bParent, RUN);
  const sta = await call(A, 'connect/state');
  const stb = await call(B, 'connect/state');
  ctx.keyA = sta.device.key_id;
  ctx.keyB = stb.device.key_id;
  ctx.flinkPort = stb.device.port;
  ok(!sta.device.listen_error, `A can't listen for FLINK: ${sta.device.listen_error}`);
  ok(!stb.device.listen_error, `B can't listen for FLINK: ${stb.device.listen_error}`);
  ok(sa.version === sb.version, `A is ${sa.version}, B is ${sb.version}: load this build on the console`);
  ok(ctx.keyA !== ctx.keyB, 'A and B have the same key id (one connect folder shared?)');
  // The NAS, FTP and NFS servers (tests/scripts/{nas,ftp,nfs}.sh): each address as both Forges
  // reach it, and its folder here. Only those Docker containers: never a real device on the network.
  ctx.nas = docker(env.FORGE_NAS, 'FORGE_NAS', 'forge-smoke-nas', 445, 'tests/scripts/nas.sh up');
  ctx.nasDir = env.FORGE_NAS_DIR || path.join(os.tmpdir(), 'forge-smoke-nas');
  ctx.ftp = docker(env.FORGE_FTP, 'FORGE_FTP', 'forge-smoke-ftp', 21, 'tests/scripts/ftp.sh up');
  ctx.ftpDir = env.FORGE_FTP_DIR || path.join(os.tmpdir(), 'forge-smoke-ftp');
  ctx.ftpIdle = ctx.ftp ? ftpIdle() : null;
  ctx.nfs3 = docker(env.FORGE_NFS3, 'FORGE_NFS3', 'forge-smoke-nfs3', 2049, 'tests/scripts/nfs.sh up');
  ctx.nfs4 = docker(env.FORGE_NFS4, 'FORGE_NFS4', 'forge-smoke-nfs4', 2049, 'tests/scripts/nfs.sh up');
  ctx.nfsDir = env.FORGE_NFS_DIR || path.join(os.tmpdir(), 'forge-smoke-nfs');
  // The games, generated once into A_WORK and reused (deterministic).
  ctx.game = gen('game-ampr', ['--ampr', '--files', '300', '--assets-mib', '48', '--big-mib', '80']);
  ctx.game1k = gen('game-1k', ['--files', '1000', '--assets-mib', '4', '--big-mib', '0', '--title-id', 'PPSA99902', '--name', 'Forge Smoke 1k']);
  ctx.lz4 = gen('game-lz4', ['--ampr', '--files', '40', '--assets-mib', '12', '--big-mib', '4', '--title-id', 'PPSA99907', '--name', 'Forge Smoke LZ4']);
  ctx.small = gen('game-small', ['--files', '40', '--assets-mib', '4', '--big-mib', '8', '--title-id', 'PPSA99906', '--name', 'Forge Smoke Small']);
  ctx.extra = path.join(ctx.aWork, 'extra');
  if (!fs.existsSync(ctx.extra)) {
    fs.mkdirSync(path.join(ctx.extra, 'sub'), { recursive: true });
    fs.writeFileSync(path.join(ctx.extra, 'readme.txt'), 'added by an edit\n');
    fs.writeFileSync(path.join(ctx.extra, 'sub/blob.bin'), Buffer.alloc(3 << 20, 7));
  }
  // This run's folders: A's locally, B's by a transfer of an empty skeleton (no mkdir route).
  for (const d of ['pulled', 'extracted', 'out']) fs.mkdirSync(path.join(ctx.aRun, d), { recursive: true });
  // `extra` under this run's own name, for transfers onto shared servers (never replace or remove
  // a folder there this run didn't write).
  ctx.runExtra = path.join(ctx.aRun, `${RUN}-extra`);
  fs.cpSync(ctx.extra, ctx.runExtra, { recursive: true });
  const skel = path.join(ctx.aWork, 'skel', RUN);
  for (const d of ['in', 'cancel', 'upload', 'slots']) fs.mkdirSync(path.join(skel, d), { recursive: true });
  ctx.skel = skel;
  console.log(`  A ${A} (${sa.version}), B ${B} (${sb.version}, ${sb.platform}); run ${RUN}`);
}

/** `addr` (`host:port`, from `name`) when it is the Docker container `container` on this computer:
 * the host one of this computer's addresses and the port the one Docker publishes for
 * `inner`; null when unset. Anything else stops the run. */
function docker(addr, name, container, inner, how) {
  if (!addr) return null;
  const [host, port] = addr.split(/:(?=\d+$)/);
  let published = '';
  try {
    published = execFileSync('docker', ['port', container, `${inner}/tcp`], { encoding: 'utf8' });
  } catch {
    throw new Error(`${name}=${addr}: the ${container} container isn't running (${how})`);
  }
  const ports = [...new Set([...published.matchAll(/:(\d+)$/gm)].map((m) => m[1]))];
  ok(ports.includes(port), `${name}=${addr}: ${container} publishes ${inner} on ${ports.join(', ') || 'nothing'}, not ${port}`);
  const mine = Object.values(os.networkInterfaces()).flat().map((i) => i.address);
  ok(mine.includes(host) || host === 'localhost', `${name}=${addr}: ${host} isn't this computer (the smoke suite uses only ${container})`);
  return addr;
}

/** vsftpd's idle_session_timeout in the FTP container (ftp.sh sets it low for the stale-session
 * case); null when the container is an older one without it. */
function ftpIdle() {
  const conf = execFileSync('docker', ['exec', 'forge-smoke-ftp', 'cat', '/etc/vsftpd/vsftpd.conf'], { encoding: 'utf8' });
  const m = conf.match(/^idle_session_timeout=(\d+)$/m);
  return m ? Number(m[1]) : null;
}

/** The game `name` in A_WORK: reused when there, else generated beside it under this run's name
 * and renamed into place. A folder there without `sce_sys/param.json` stops the run (never removed). */
function gen(name, opts) {
  const dir = path.join(ctx.aWork, name);
  if (fs.existsSync(path.join(dir, 'sce_sys/param.json'))) return dir;
  ok(!fs.existsSync(dir), `${dir} is there but isn't a generated game: remove it or set A_WORK`);
  const part = `${dir}.${RUN}.part`;
  execFileSync(process.execPath, [path.join(HERE, 'gen-game.mjs'), part, ...opts], { stdio: 'inherit' });
  fs.renameSync(part, dir);
  return dir;
}

const fA = (abs) => flink(ctx.keyA, ctx.aShare, abs);
const fB = (abs) => flink(ctx.keyB, ctx.bShare, abs);
const bPath = (...p) => path.posix.join(ctx.bRun, ...p);
const transfer = (paths, output) => request({ transfer: paths, output });
const devices = async (base) => (await call(base, 'connect/state')).devices;

/** Waits until `base` lists `key` online. */
async function online(base, key, secs = 20) {
  for (let i = 0; i < secs; i++) {
    if ((await devices(base)).some((d) => d.key_id === key && d.online)) return;
    await sleep(1000);
  }
  throw new Error(`${base} doesn't see ${key} online after ${secs} s`);
}

// ---- tests -------------------------------------------------------------------------------------

/** A asks B to pair; B's user (played over B's API) answers `allow` once both show one check
 * number. The request's outcome (`state`: paired, denied, ...). */
async function pairVia(allow) {
  await call(A, 'connect/pair', { address: ctx.bAddr });
  let answered = false;
  for (let i = 0; i < 300; i++) {
    const out = (await call(A, 'connect/state')).outgoing;
    if (!answered && out?.check) {
      const req = (await call(B, 'connect/state')).pending.find((r) => r.check === out.check);
      if (req) {
        await call(B, 'connect/approve', { id: req.id, allow });
        answered = true;
      }
    }
    if (out && !['asking', 'waiting'].includes(out.state)) return out;
    await sleep(300);
  }
  throw new Error('the pairing request never ended');
}

test('pairing: B denies a request (nothing saved), then allows one (both sides, online)', async () => {
  const host = new URL(B).hostname;
  ctx.bAddr = `${host}:${ctx.flinkPort}`;
  // A clean start: an earlier run's pairing would be refused as already paired.
  await call(A, 'connect/forget', { key_id: ctx.keyB });
  await call(B, 'connect/forget', { key_id: ctx.keyA });
  const denied = await pairVia(false);
  ok(denied.state === 'denied', `a denied request ended ${denied.state}: ${denied.error ?? ''}`);
  ok(!(await devices(A)).some((d) => d.key_id === ctx.keyB), 'denied, yet B is saved on A');
  ok(!(await devices(B)).some((d) => d.key_id === ctx.keyA), 'denied, yet A is saved on B');
  const paired = await pairVia(true);
  ok(paired.state === 'paired', `an allowed request ended ${paired.state}: ${paired.error ?? ''}`);
  ok(paired.device?.key_id === ctx.keyB, `paired with ${paired.device?.key_id}, expected ${ctx.keyB}`);
  await online(A, ctx.keyB);
  await online(B, ctx.keyA);
  // This run's folders on B, from an empty skeleton (there is no mkdir route).
  await run(A, transfer([ctx.skel], fB(ctx.bParent)));
});

test('Pair by address: the probe finds each Forge and every test server (SMB, FTP, NFS)', async () => {
  // B as A dials it, A as B saved it; on a console B's bare IP too (Forge's own ports).
  const a = (await devices(B)).find((d) => d.key_id === ctx.keyA)?.address;
  ok(a, 'B has no address for A');
  const servers = [[ctx.nas, 'smb'], [ctx.ftp, 'ftp'], [ctx.nfs3, 'nfs'], [ctx.nfs4, 'nfs']].filter(([at]) => at);
  const asks = [[A, ctx.bAddr, 'forge'], [B, a, 'forge'], ...servers.flatMap(([at, kind]) => [[A, at, kind], [B, at, kind]])];
  if (ctx.ps5) asks.push([A, new URL(B).hostname, 'forge']);
  const got = await Promise.all(asks.map(([base, address]) => call(base, 'connect/probe', { address })));
  asks.forEach(([base, address, kind], i) => {
    const p = got[i];
    ok(p.kind === kind, `${base} probing ${address}: ${JSON.stringify(p)}, expected ${kind}`);
    if (kind === 'forge') ok(p.name && p.port, `${base} probing ${address}: no name or port`);
  });
  ok(got[0].port === ctx.flinkPort, `B's FLINK port probed as ${got[0].port}, not ${ctx.flinkPort}`);
});

test('push: a transfer job on A sends a game folder to B', async () => {
  await run(A, transfer([ctx.game, ctx.lz4, ctx.extra], fB(bPath('in'))));
  same(sizeTree(ctx.game), await remoteSizes(B, bPath('in/game-ampr')), 'sizes on B');
  same(sizeTree(ctx.lz4), await remoteSizes(B, bPath('in/game-lz4')), 'sizes on B (LZ4 game)');
});

test('pull: a transfer job on A fetches it back, byte for byte', async () => {
  await run(A, transfer([fB(bPath('in/game-ampr'))], path.join(ctx.aRun, 'pulled')));
  same(hashTree(ctx.game), hashTree(path.join(ctx.aRun, 'pulled/game-ampr')), 'round trip');
});

test('B pulls 1,000 small files from A (B as the FLINK client)', async () => {
  await run(B, transfer([fA(ctx.game1k)], bPath('in')));
  same(sizeTree(ctx.game1k), await remoteSizes(B, bPath('in/game-1k')), '1k files on B');
});

test('fpkg on B: the 1k-file game into a .pkg on the console', async () => {
  const out = bPath('out-1k.pkg');
  const r = await run(B, request({ source: bPath('in/game-1k'), format: 'pkg', output: out }));
  ok(r.report.files >= 1_000, `the .pkg holds ${r.report.files} files`);
  const found = await call(B, 'inspect', { path: out });
  ok(found.title_id === 'PPSA99902', `inspect: ${JSON.stringify(found).slice(0, 200)}`);
});

test('fpkg on A from B: a peer folder streamed into a local .pkg', async () => {
  const out = path.join(ctx.aRun, 'out/from-peer-1k.pkg');
  await run(A, request({ source: fB(bPath('in/game-1k')), format: 'pkg', output: out }));
  const found = await call(A, 'inspect', { path: out });
  ok(found.title_id === 'PPSA99902', `inspect: ${JSON.stringify(found).slice(0, 200)}`);
  // ...and back out of it: the .pkg extracted to a folder matches the generated game.
  const back = path.join(ctx.aRun, 'out/from-pkg');
  await run(A, request({ source: out, format: 'folder', output: back }));
  // The builder writes its own param.json and adds a keystone; every game file comes back.
  const got = hashTree(back);
  ok(got['sce_sys/param.json'] && got['sce_sys/keystone'], 'no param.json or keystone in the .pkg');
  const want = hashTree(ctx.game1k);
  for (const k of ['sce_sys/param.json', 'sce_sys/keystone']) { delete want[k]; delete got[k]; }
  same(want, got, '.pkg extracted');
});

test('convert on A into B: a local game written as an .ffpkg and an fpkg on the peer', async () => {
  const out = bPath('game.ffpkg');
  await run(A, request({ source: ctx.game, format: 'ffpkg', output: fB(out) }));
  const found = await call(B, 'inspect', { path: out });
  ok(found.title_id === 'PPSA99901', `inspect on B: ${JSON.stringify(found).slice(0, 200)}`);
  // The fpkg builder writes through the peer too (its spool and self-check read back there).
  const pkg = bPath('game.pkg');
  await run(A, request({ source: ctx.game, format: 'pkg', output: fB(pkg) }));
  const inPkg = await call(B, 'inspect', { path: pkg });
  ok(inPkg.title_id === 'PPSA99901', `inspect .pkg on B: ${JSON.stringify(inPkg).slice(0, 200)}`);
});

test('convert on B from A: a peer game into an .exfat on the console', async () => {
  const out = bPath('game.exfat');
  await run(B, request({ source: fA(ctx.game), format: 'exfat', output: out, full_verify: true }));
  const found = await call(B, 'inspect', { path: out });
  ok(found.title_id === 'PPSA99901', `inspect on B: ${JSON.stringify(found).slice(0, 200)}`);
});

test('edit on B in place: add files from A (flink://), rename, delete', async () => {
  const req = request({
    source: bPath('game.ffpkg'),
    format: 'ffpkg',
    edits: [
      { op: 'add', from: fA(ctx.extra), to: 'data/extra' },
      { op: 'rename', from: 'data/a b.txt', to: 'data/renamed.txt' },
      { op: 'delete', path: 'data/zero.bin' },
    ],
  });
  const check = await call(B, 'edit_check', { request: req });
  ok(check.in_place.length === 0, `edit_check in place: ${check.in_place.join('; ')}`);
  await run(B, req);
});

test('extract on B into A: the edited files land on the peer, byte for byte', async () => {
  const out = path.join(ctx.aRun, 'extracted');
  await run(B, request({ source: bPath('game.ffpkg'), format: 'folder', output: fA(out), extract: ['data/extra', 'data/renamed.txt'] }));
  same(hashTree(ctx.extra), hashTree(path.join(out, 'extra')), 'added then extracted');
  ok(fs.readFileSync(path.join(out, 'renamed.txt'), 'utf8') === 'a name with a space\n', 'renamed.txt content');
  // The delete took: extracting the deleted file is refused.
  await run(B, request({ source: bPath('game.ffpkg'), format: 'folder', output: fA(out), extract: ['data/zero.bin'] }), { expectError: /zero\.bin/ });
});

test('LZ4 pack on B: the AMPR game packed into an .ffpkg on the console', async () => {
  const out = bPath('packed.ffpkg');
  await run(B, request({ source: bPath('in/game-ampr'), format: 'ffpkg', lz4: 'pack', output: out }));
  const found = await call(B, 'inspect', { path: out });
  ok(found.lz4?.packed?.volumes >= 1, `not packed: ${JSON.stringify(found.lz4).slice(0, 300)}`);
});

test('LZ4 pack on A, B to B: a peer game packed into a folder on the peer, then unpacked', async () => {
  const out = bPath('packed-folder');
  await run(A, request({ source: fB(bPath('in/game-lz4')), format: 'folder', lz4: 'pack', output: fB(out) }));
  const found = await call(B, 'inspect', { path: out });
  ok(found.lz4?.packed?.volumes >= 1, `not packed: ${JSON.stringify(found.lz4).slice(0, 300)}`);
  // Unpacked back here: the assets are the generated ones (the runtime and index are Forge's).
  const back = path.join(ctx.aRun, 'out/unpacked');
  await run(A, request({ source: fB(out), format: 'folder', lz4: 'unpack', output: back }));
  const want = hashTree(ctx.lz4);
  const got = hashTree(back);
  for (const k of Object.keys(got)) if (k.startsWith('fakelib/') || k === 'ampr_emu.index') delete got[k];
  same(want, got, 'pack + unpack');
});

test("browser upload to B (8 MiB chunks, 4 at once) and download back", async () => {
  const size = 20 * 1024 * 1024 + 4321;
  const data = Buffer.alloc(size);
  for (let i = 0; i + 4 <= size; i += 4) data.writeUInt32LE((i * 2654435761) >>> 0, i);
  const dir = bPath('upload');
  const base = `${B}/api/connect/upload?dir=${encodeURIComponent(dir)}&name=${encodeURIComponent('up load.bin')}&id=smoke${Date.now()}`;
  const offsets = [];
  for (let at = 0; at < size; at += 8 << 20) offsets.push(at);
  let next = 0;
  let done;
  const lane = async () => {
    while (next < offsets.length) {
      const at = offsets[next++];
      const res = await fetch(`${base}&offset=${at}&size=${size}`, { method: 'POST', body: data.subarray(at, Math.min(at + (8 << 20), size)) });
      const body = await res.json();
      ok(res.ok, `chunk ${at}: ${body.error}`);
      if (body.done) done = body.path;
    }
  };
  await Promise.all([lane(), lane(), lane(), lane()]);
  ok(done, 'no chunk completed the upload');
  const res = await fetch(`${B}/api/connect/download?path=${encodeURIComponent(done)}`);
  ok(res.ok, `download: HTTP ${res.status}`);
  ok(Buffer.from(await res.arrayBuffer()).equals(data), 'the downloaded file differs from the upload');
});

test('cancel a transfer to B: nothing half-written stays on the peer', async () => {
  const dir = bPath('cancel');
  const id = await start(A, transfer([path.join(ctx.game, 'data/big.bin')], fB(dir)));
  let cancelled = false;
  const r = await wait(A, id, { onTick: async (j) => {
    if (!cancelled && j.progress?.done > 0) { cancelled = true; await call(A, 'cancel_job', { id }); }
  } });
  if (r.ok) console.log('      note: the transfer finished before the cancel landed');
  else ok(/cancel/.test(r.error), `ended with: ${r.error}`);
  for (let i = 0; ; i++) {
    const names = (await call(B, 'list_dir', { path: dir })).entries.map((e) => e.name);
    const left = names.filter((n) => r.ok ? n !== 'big.bin' : true);
    if (!left.length) break;
    ok(i < 15, `left on B after the cancel: ${left.join(', ')}`);
    await sleep(1000);
  }
});

/** `fn` must fail, as an HTTP error or a job that ends with an error, matching `re`. */
async function refused(what, re, fn) {
  let r;
  try { r = await fn(); } catch (e) { return ok(re.test(e.text ?? e.message), `${what}: "${e.text ?? e.message}" doesn't match ${re}`); }
  if (r && typeof r === 'number') r = await wait(A, r);
  ok(r && r.ok === false, `${what}: was not refused`);
  ok(re.test(r.error), `${what}: "${r.error}" doesn't match ${re}`);
}

test('refusals: peer edits, deletes, a .pkg source, taken names, escapes', async () => {
  const img = fB(bPath('game.ffpkg'));
  await refused('edit a peer image', /peer|another device|edit/i, () => start(A, request({ source: img, format: 'ffpkg', edits: [{ op: 'delete', path: 'data/big.bin' }] })));
  await refused('delete on a peer', /./, () => call(A, 'delete_path', { path: img }));
  await refused('.pkg source on a peer', /copy it to this device/i, () => start(A, request({ source: fB(bPath('game.pkg')), format: 'folder', output: path.join(ctx.aRun, 'out/x') })));
  await refused('transfer onto a taken name', /exists/i, () => start(A, transfer([ctx.game], fB(bPath('in')))));
  await refused('a path out of the share root', /./, () => call(A, 'list_dir', { path: `flink://${ctx.keyB}/../../..` }));
});

test('job slots: a transfer runs beside a convert; a clash with its paths is refused', async () => {
  const heavy = await start(A, request({ source: ctx.game, format: 'ffpfsc', ffpfsc_level: 9, full_verify: true, output: path.join(ctx.aRun, 'out/slots.ffpfsc') }));
  await refused('a transfer of the converting game', /in use by another job/, () => start(A, transfer([ctx.game], fB(bPath('slots')))));
  const side = await start(A, transfer([ctx.game1k], fB(bPath('slots'))));
  const jobs = (await call(A, 'jobs')).jobs;
  const both = [heavy, side].every((id) => !jobs.find((j) => j.id === id)?.done);
  if (!both) console.log('      note: the convert ended before the transfer started; overlap not seen');
  for (const id of [heavy, side]) {
    const r = await wait(A, id);
    ok(r.ok, `job ${id}: ${r.error}`);
  }
});

/** The NAS game's `smb://` path; skips the case without a NAS. */
function nasGame() {
  if (!ctx.nas) throw new Skip('set FORGE_NAS (tests/scripts/nas.sh up)');
  return `smb://${ctx.nas}/games/game-nas`;
}

test('NAS: saved on both Forges (guest), online, its share and game listed', async () => {
  nasGame();
  for (const [base, at] of [[A, 'nasA'], [B, 'nasB']]) {
    ctx[at] = (await call(base, 'connect/nas_save', { name: 'Smoke NAS', host: ctx.nas })).id;
    for (let i = 0; ; i++) {
      const nas = (await call(base, 'connect/state')).nas.find((n) => n.id === ctx[at]);
      if (nas?.online) break;
      ok(i < 30, `${base} doesn't see the NAS online after 30 s`);
      await sleep(1000);
    }
    const shares = (await call(base, 'list_dir', { path: `smb://${ctx.nas}` })).entries.map((e) => e.name);
    ok(shares.includes('games'), `${base}: shares ${shares.join(', ')}`);
    const games = (await call(base, 'list_dir', { path: `smb://${ctx.nas}/games` })).entries.map((e) => e.name);
    ok(games.includes('game-nas'), `${base}: the share holds ${games.join(', ')}`);
  }
});

test('NAS games and refusals: the console converts and copies one, A copies one; no edit, a share root not deleted', async () => {
  const src = nasGame();
  const out = bPath('nas.ffpkg');
  await run(B, request({ source: src, format: 'ffpkg', output: out }));
  const found = await call(B, 'inspect', { path: out });
  ok(found.title_id === 'PPSA99903', `inspect on B: ${JSON.stringify(found).slice(0, 200)}`);
  await run(B, transfer([src], bPath('in')));
  same(sizeTree(path.join(ctx.nasDir, 'game-nas')), await remoteSizes(B, bPath('in/game-nas')), 'NAS game on B');
  await run(A, transfer([src], path.join(ctx.aRun, 'pulled')));
  same(hashTree(path.join(ctx.nasDir, 'game-nas')), hashTree(path.join(ctx.aRun, 'pulled/game-nas')), 'NAS game on A');
  await refused('edit a NAS game', /./, () => start(A, request({ source: src, format: 'folder', edits: [{ op: 'delete', path: 'data/zero.bin' }] })));
  await refused('delete a share root', /./, () => call(A, 'delete_path', { path: src.replace(/\/game-nas$/, '') }));
});

test('NAS outputs: A writes an .exfat, an fpkg (streamed) and a transfer onto the share, hashed; forgotten', async () => {
  const out = `${nasGame().replace(/\/game-nas$/, '')}/out`;
  const here = path.join(ctx.nasDir, 'out');
  const [img, pkg] = [`${RUN}-smb.exfat`, `${RUN}-smb.pkg`];
  try {
    await run(A, request({ source: ctx.small, format: 'exfat', output: `${out}/${img}` }));
    await sameImage(path.join(here, img), ctx.small, 'exfat on the share');
    await run(A, request({ source: ctx.small, format: 'pkg', output: `${out}/${pkg}` }));
    await sameImage(path.join(here, pkg), ctx.small, 'fpkg on the share', true);
    await run(A, transfer([ctx.runExtra], out));
    same(hashTree(ctx.extra), hashTree(path.join(here, path.basename(ctx.runExtra))), 'transfer onto the share');
  } finally {
    for (const n of [img, pkg, path.basename(ctx.runExtra)]) fs.rmSync(path.join(here, n), { recursive: true, force: true });
  }
  await forgetNas('nasA', 'nasB');
});

/** Converts the image `img` (here) back into a folder here and compares it with `game`; an fpkg
 * (`pkg`) holds the builder's own param.json and a keystone, left out (as case 6). */
async function sameImage(img, game, what, pkg = false) {
  const back = path.join(ctx.aRun, 'out', `${path.basename(img)}-back`);
  await run(A, request({ source: img, format: 'folder', output: back }));
  const [want, got] = [hashTree(game), hashTree(back)];
  if (pkg) for (const k of ['sce_sys/param.json', 'sce_sys/keystone']) { delete want[k]; delete got[k]; }
  same(want, got, what);
  fs.rmSync(back, { recursive: true, force: true });
}

/** Forgets the saved entries `ctx[a]` (on A) and `ctx[b]` (on B). */
async function forgetNas(a, b) {
  for (const [base, at] of [[A, a], [B, b]]) {
    ok(await call(base, 'connect/nas_forget', { id: ctx[at] }), `${base}: nas_forget returned false`);
    ok(!(await call(base, 'connect/state')).nas.some((n) => n.id === ctx[at]), `${base} still lists the server`);
  }
}

/** The FTP server's root (`ftp://host:port`); skips the case without one. */
function ftpRoot() {
  if (!ctx.ftp) throw new Skip('set FORGE_FTP (tests/scripts/ftp.sh up)');
  return `ftp://${ctx.ftp}`;
}

test('FTP: saved on both (login), online; the console converts its game, A writes an fpkg onto it', async () => {
  const root = ftpRoot();
  for (const [base, at] of [[A, 'ftpA'], [B, 'ftpB']]) {
    ctx[at] = (await call(base, 'connect/nas_save', { name: 'Smoke FTP', host: ctx.ftp, protocol: 'ftp', user: 'forge', password: 'forge' })).id;
    for (let i = 0; ; i++) {
      const nas = (await call(base, 'connect/state')).nas.find((n) => n.id === ctx[at]);
      if (nas?.online) break;
      ok(i < 30, `${base} doesn't see the FTP server online after 30 s`);
      await sleep(1000);
    }
    const names = (await call(base, 'list_dir', { path: root })).entries.map((e) => e.name);
    ok(names.includes('game-ftp') && names.includes('out'), `${base}: the server holds ${names.join(', ')}`);
  }
  const out = bPath('ftp.ffpkg');
  await run(B, request({ source: `${root}/game-ftp`, format: 'ffpkg', output: out }));
  const found = await call(B, 'inspect', { path: out });
  ok(found.title_id === 'PPSA99904', `inspect on B: ${JSON.stringify(found).slice(0, 200)}`);
  // An fpkg goes back over what it wrote: the server's rewrite probe lets it.
  const pkg = `${RUN}-a.pkg`;
  await run(A, request({ source: ctx.game1k, format: 'pkg', output: `${root}/out/${pkg}` }));
  const back = path.join(ctx.aRun, 'out/ftp-pkg');
  await run(A, request({ source: path.join(ctx.ftpDir, 'out', pkg), format: 'folder', output: back }));
  // As case 6: the builder writes its own param.json and a keystone; every game file comes back.
  const [want, got] = [hashTree(ctx.game1k), hashTree(back)];
  for (const k of ['sce_sys/param.json', 'sce_sys/keystone']) { delete want[k]; delete got[k]; }
  same(want, got, 'fpkg on FTP, extracted');
});

test('FTP from the console: an image and a transfer written onto it; no edit, its root not deleted', async () => {
  const root = ftpRoot();
  const img = `${RUN}-b.exfat`;
  await run(B, request({ source: fA(ctx.game), format: 'exfat', output: `${root}/out/${img}` }));
  const found = await call(A, 'inspect', { path: path.join(ctx.ftpDir, 'out', img) });
  ok(found.title_id, `inspect of the image on FTP: ${JSON.stringify(found).slice(0, 200)}`);
  await run(B, transfer([fA(ctx.runExtra)], `${root}/out`));
  same(hashTree(ctx.extra), hashTree(path.join(ctx.ftpDir, 'out', path.basename(ctx.runExtra))), 'transfer onto FTP');
  await refused('edit an FTP game', /./, () => start(A, request({ source: `${root}/game-ftp`, format: 'folder', edits: [{ op: 'delete', path: 'eboot.bin' }] })));
  await refused('delete the FTP root', /./, () => call(A, 'delete_path', { path: `${root}/` }));
  fs.rmSync(path.join(ctx.ftpDir, 'out', img), { force: true });
  fs.rmSync(path.join(ctx.ftpDir, 'out', `${RUN}-a.pkg`), { force: true });
  fs.rmSync(path.join(ctx.ftpDir, 'out', path.basename(ctx.runExtra)), { recursive: true, force: true });
});

test('FTP after the server closed idle sessions: a job on each Forge logs in again; forgotten', async () => {
  const root = ftpRoot();
  if (ctx.ftpIdle === null || ctx.ftpIdle > 60) throw new Skip('restart the FTP server: tests/scripts/ftp.sh up (a short idle timeout)');
  // Both Forges hold a session from the cases above; vsftpd closes it (421) once idle.
  for (const base of [A, B]) await call(base, 'list_dir', { path: `${root}/game-ftp` });
  await sleep((ctx.ftpIdle + 5) * 1000);
  const game = path.join(ctx.ftpDir, 'game-ftp/sce_sys');
  await run(B, transfer([`${root}/game-ftp/sce_sys`], bPath('in')));
  same(sizeTree(game), await remoteSizes(B, bPath('in/sce_sys')), 'FTP after the idle drop, on B');
  await run(A, transfer([`${root}/game-ftp/sce_sys`], path.join(ctx.aRun, 'pulled')));
  same(hashTree(game), hashTree(path.join(ctx.aRun, 'pulled/sce_sys')), 'FTP after the idle drop, on A');
  await forgetNas('ftpA', 'ftpB');
});

/** One NFS server (`ver` 3 or 4): saved on both Forges (no login), online, its export and game
 * listed; the console converts its game; A writes an .ffpkg onto out/, compared with the source
 * (v3: the export is the folder here; v4: a Docker volume, copied out with `docker cp`); no edit
 * or delete; forgotten on both. */
async function nfsCase(ver) {
  const addr = ctx[`nfs${ver}`];
  if (!addr) throw new Skip(`set FORGE_NFS${ver} (tests/scripts/nfs.sh up)`);
  const root = `nfs://${addr}`;
  const [a, b] = [`nfs${ver}A`, `nfs${ver}B`];
  for (const [base, at] of [[A, a], [B, b]]) {
    ctx[at] = (await call(base, 'connect/nas_save', { name: `Smoke NFSv${ver}`, host: addr, protocol: 'nfs' })).id;
    for (let i = 0; ; i++) {
      const nas = (await call(base, 'connect/state')).nas.find((n) => n.id === ctx[at]);
      if (nas?.online) break;
      ok(i < 30, `${base} doesn't see the NFS server online after 30 s`);
      await sleep(1000);
    }
    const exports = (await call(base, 'list_dir', { path: root })).entries.map((e) => e.name);
    ok(exports.includes('share'), `${base}: ${root} lists ${exports.join(', ')}`);
    const names = (await call(base, 'list_dir', { path: `${root}/share` })).entries.map((e) => e.name);
    ok(names.includes('game-nfs') && names.includes('out'), `${base}: the export holds ${names.join(', ')}`);
  }
  const src = `${root}/share/game-nfs`;
  const out = bPath(`nfs${ver}.ffpkg`);
  await run(B, request({ source: src, format: 'ffpkg', output: out }));
  const found = await call(B, 'inspect', { path: out });
  ok(found.title_id === 'PPSA99905', `inspect on B: ${JSON.stringify(found).slice(0, 200)}`);
  const img = `${RUN}-nfs${ver}.ffpkg`;
  const container = `forge-smoke-nfs${ver}`;
  let local = path.join(ctx.nfsDir, 'out', img);
  try {
    await run(A, request({ source: ctx.small, format: 'ffpkg', output: `${root}/share/out/${img}` }));
    if (ver === 4) {
      local = path.join(ctx.aRun, 'out', img);
      execFileSync('docker', ['cp', `${container}:/share/out/${img}`, local]);
    }
    await sameImage(local, ctx.small, `ffpkg on NFSv${ver}`);
  } finally {
    if (ver === 4) execFileSync('docker', ['exec', container, 'rm', '-f', `/share/out/${img}`]);
    fs.rmSync(local, { force: true });
  }
  await refused('edit an NFS game', /./, () => start(A, request({ source: src, format: 'folder', edits: [{ op: 'delete', path: 'eboot.bin' }] })));
  await refused('delete an NFS export root', /./, () => call(A, 'delete_path', { path: src.slice(0, src.lastIndexOf('/')) }));
  await forgetNas(a, b);
}

test('NFSv3: saved on both, listed; the console converts its game, A writes an .ffpkg onto it; no edit, the export not deleted', () => nfsCase(3));
test('NFSv4.1: saved on both, listed; the console converts its game, A writes an .ffpkg onto it; no edit, the export not deleted', () => nfsCase(4));

test('forget and pair again: one saved entry on each side, online', async () => {
  ok(await call(A, 'connect/forget', { key_id: ctx.keyB }), 'forget returned false');
  ok(!(await devices(A)).some((d) => d.key_id === ctx.keyB), 'B is still saved on A');
  // B still has A saved: asking again is refused unasked until B forgets A too.
  const again = await pairVia(true);
  ok(again.state === 'already_paired', `asking again ended ${again.state}`);
  ok(await call(B, 'connect/forget', { key_id: ctx.keyA }), 'forget on B returned false');
  const paired = await pairVia(true);
  ok(paired.state === 'paired', `pairing again ended ${paired.state}: ${paired.error ?? ''}`);
  await online(A, ctx.keyB);
  await online(B, ctx.keyA);
  ok((await devices(A)).filter((d) => d.key_id === ctx.keyB).length === 1, 'B saved twice on A');
  ok((await devices(B)).filter((d) => d.key_id === ctx.keyA).length === 1, 'A saved twice on B');
});

// ---- main --------------------------------------------------------------------------------------

async function main() {
  if (args.includes('--list')) {
    for (const t of tests) console.log(`${String(t.n).padStart(2)}  ${t.name}`);
    return 0;
  }
  console.log('setup');
  await setup();
  const results = [];
  let skipped = 0;
  for (const t of tests) {
    if (only && !only.has(t.n)) continue;
    const t0 = Date.now();
    process.stdout.write(`${String(t.n).padStart(2)}  ${t.name}\n`);
    try {
      await t.fn();
      results.push([t, null]);
      console.log(`    PASS (${((Date.now() - t0) / 1000).toFixed(1)} s)`);
    } catch (e) {
      if (e instanceof Skip) {
        console.log(`    SKIP: ${e.message}`);
        skipped++;
        continue;
      }
      results.push([t, e]);
      console.log(`    FAIL (${((Date.now() - t0) / 1000).toFixed(1)} s): ${e.message}`);
    }
  }
  const failed = results.filter(([, e]) => e);
  console.log(`\n${results.length - failed.length} passed, ${failed.length} failed${failed.length ? `: ${failed.map(([t]) => t.n).join(', ')}` : ''}${skipped ? `, ${skipped} skipped` : ''}`);
  if (failed.length || keep) {
    console.log(`kept: ${ctx.aRun} on A, ${ctx.bRun} on B`);
  } else {
    fs.rmSync(ctx.aRun, { recursive: true, force: true });
    fs.rmSync(ctx.skel, { recursive: true, force: true });
    try { fs.rmdirSync(path.dirname(ctx.skel)); } catch {} // only when no other run's skeleton is left
    await call(B, 'delete_path', { path: ctx.bRun }).catch((e) => console.log(`cleanup on B: ${e.message}`));
  }
  return failed.length ? 1 : 0;
}

main().then((code) => process.exit(code), (e) => { console.error(`smoke: ${e.message}`); process.exit(2); });
