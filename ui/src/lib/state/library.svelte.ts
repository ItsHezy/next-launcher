import { boot, num } from '../boot';
import {
  commands,
  events,
  type GameRow_Serialize,
  type DownloadRow,
  type AccountRow_Serialize,
  type SyncStatus,
  type DownloadProgress,
  type LibrarySnapshot_Serialize,
} from '../bindings';

export type Game = GameRow_Serialize;
export type Filter = 'all' | 'installed' | 'updates';
export type Sort = 'title' | 'recent' | 'playtime' | 'size';

const isInstalled = (g: Game) => g.state === 'installed' || g.state === 'updateAvailable';

/** Same data? Then keep the old object so its tile doesn't re-render. */
function sameGame(a: Game, b: Game): boolean {
  return (
    a.title === b.title &&
    a.developer === b.developer &&
    a.state === b.state &&
    a.installSize === b.installSize &&
    a.lastPlayed === b.lastPlayed &&
    a.playtimeSecs === b.playtimeSecs &&
    a.pinned === b.pinned &&
    a.hasCover === b.hasCover &&
    a.hasWide === b.hasWide &&
    a.artRev === b.artRev &&
    a.installable === b.installable &&
    a.thirdParty === b.thirdParty
  );
}

function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i]);
}

class LibraryState {
  /** Immutable rows; updates replace the array so only changed tiles re-render. */
  games = $state.raw<Game[]>(boot.snapshot.games);
  quickLaunch = $state.raw<string[]>(boot.snapshot.quickLaunch);
  recent = $state.raw<string[]>(boot.snapshot.recent);
  download = $state.raw<DownloadRow | null>(boot.snapshot.activeDownload ?? null);
  account = $state.raw<AccountRow_Serialize | null>(boot.snapshot.account ?? null);
  filter = $state<Filter>('all');
  query = $state('');
  sync = $state.raw<SyncStatus>({ syncing: false, error: null, lastSync: null });
  /** Live progress of the running download (events, ~4/s). */
  progress = $state.raw<DownloadProgress | null>(null);

  byId = $derived(new Map(this.games.map((g) => [g.appName, g])));

  counts = $derived.by(() => {
    let installed = 0;
    let updates = 0;
    for (const g of this.games) {
      if (isInstalled(g)) installed++;
      if (g.state === 'updateAvailable') updates++;
    }
    return { all: this.games.length, installed, updates };
  });

  pinned = $derived(this.quickLaunch.map((id) => this.byId.get(id)).filter((g): g is Game => !!g));
  recentGames = $derived(this.recent.map((id) => this.byId.get(id)).filter((g): g is Game => !!g));

  list(sort: Sort, showUninstalled: boolean): Game[] {
    let rows = this.games;
    if (this.filter === 'installed') rows = rows.filter(isInstalled);
    else if (this.filter === 'updates') rows = rows.filter((g) => g.state === 'updateAvailable');
    else if (!showUninstalled) rows = rows.filter((g) => g.state !== 'notInstalled');
    const q = this.query.trim().toLowerCase();
    if (q) rows = rows.filter((g) => g.title.toLowerCase().includes(q) || g.developer.toLowerCase().includes(q));

    if (sort === 'title') return rows; // snapshot is already sorted by title
    const key: (g: Game) => number =
      sort === 'recent'
        ? (g) => num(g.lastPlayed)
        : sort === 'playtime'
          ? (g) => num(g.playtimeSecs)
          : (g) => num(g.installSize);
    return [...rows].sort((a, b) => key(b) - key(a));
  }

  /** Apply a fresh snapshot, reusing unchanged row objects (only changed tiles re-render). */
  merge(snap: LibrarySnapshot_Serialize) {
    const old = this.byId;
    let changed = snap.games.length !== this.games.length;
    const next = snap.games.map((g, i) => {
      const prev = old.get(g.appName);
      if (prev && sameGame(prev, g)) {
        if (this.games[i] !== prev) changed = true;
        return prev;
      }
      changed = true;
      return g;
    });
    if (changed) this.games = next;
    if (!sameList(this.quickLaunch, snap.quickLaunch)) this.quickLaunch = snap.quickLaunch;
    if (!sameList(this.recent, snap.recent)) this.recent = snap.recent;
    const dl = snap.activeDownload ?? null;
    if (JSON.stringify(dl) !== JSON.stringify(this.download)) this.download = dl;
    const acct = snap.account ?? null;
    if (JSON.stringify(acct) !== JSON.stringify(this.account)) this.account = acct;
  }

  async reload() {
    const res = await commands.getSnapshot();
    if (res.status === 'ok') this.merge(res.data);
  }

  /** Follow library changes from Rust (debounced: cover art arrives in bursts). */
  async init() {
    let timer: ReturnType<typeof setTimeout> | undefined;
    await events.libraryChanged.listen(() => {
      clearTimeout(timer);
      timer = setTimeout(() => void this.reload(), 150);
    });
    await events.syncStatusChanged.listen((e) => (this.sync = e.payload));
    await events.downloadProgress.listen((e) => (this.progress = e.payload));
    this.sync = await commands.syncStatus();
  }

  async refresh() {
    const res = await commands.refreshLibrary();
    if (res.status === 'error') return res.error.message;
    return null;
  }

  async setPinned(appName: string, pinned: boolean) {
    const before = this.quickLaunch;
    this.quickLaunch = pinned ? [...before, appName] : before.filter((id) => id !== appName);
    this.games = this.games.map((g) => (g.appName === appName ? { ...g, pinned } : g));
    const res = await commands.setPinned(appName, pinned);
    if (res.status === 'error') this.quickLaunch = before;
  }
}

export const library = new LibraryState();
export { isInstalled };
