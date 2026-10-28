import { boot, num } from '../boot';
import { commands, type GameRow_Serialize, type DownloadRow, type AccountRow_Serialize } from '../bindings';

export type Game = GameRow_Serialize;
export type Filter = 'all' | 'installed' | 'updates';
export type Sort = 'title' | 'recent' | 'playtime' | 'size';

const isInstalled = (g: Game) => g.state === 'installed' || g.state === 'updateAvailable';

class LibraryState {
  /** Immutable rows; updates replace the array so only changed tiles re-render. */
  games = $state.raw<Game[]>(boot.snapshot.games);
  quickLaunch = $state.raw<string[]>(boot.snapshot.quickLaunch);
  recent = $state.raw<string[]>(boot.snapshot.recent);
  download = $state.raw<DownloadRow | null>(boot.snapshot.activeDownload ?? null);
  account = $state.raw<AccountRow_Serialize | null>(boot.snapshot.account ?? null);
  filter = $state<Filter>('all');
  query = $state('');

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
