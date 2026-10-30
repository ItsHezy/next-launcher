// Tiny in-memory router. Routes stay mounted once visited (tab switches only
// toggle visibility), and lazy routes are prefetched when the app is idle.

import type { EpicPage } from '../bindings';

export type Route = 'library' | 'store' | 'unreal' | 'fab' | 'settings';
export type SettingsSection =
  | 'account'
  | 'wallet'
  | 'general'
  | 'downloads'
  | 'notifications'
  | 'cloud'
  | 'network'
  | 'games'
  | 'support'
  | 'legal'
  | 'about';

export interface NavTiming {
  route: Route;
  ms: number;
  firstVisit: boolean;
}

class RouterState {
  // The app always opens on Library (brief section 6).
  route = $state<Route>('library');
  section = $state<SettingsSection>('general');
  /** The route to return to from Settings. */
  back = $state<Route>('library');
  /** Epic's own page shown inside the Settings section (ADR 0016); any navigation leaves it. */
  epicPage = $state<{ page: EpicPage; title: string } | null>(null);
  visited = $state<Record<Route, boolean>>({
    library: true,
    store: false,
    unreal: false,
    fab: false,
    settings: false,
  });
  timings = $state.raw<NavTiming[]>([]);

  go(route: Route, section?: SettingsSection) {
    this.epicPage = null;
    if (section) this.section = section;
    if (route === this.route) return;
    const firstVisit = !this.visited[route];
    const t0 = performance.now();
    if (route === 'settings') this.back = this.route;
    this.visited[route] = true;
    this.route = route;
    // Two frames later the new view has painted.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        const ms = performance.now() - t0;
        this.timings = [...this.timings.slice(-19), { route, ms, firstVisit }];
        if (import.meta.env.DEV) console.debug(`[nav] ${route} ${firstVisit ? '(first)' : ''} ${ms.toFixed(1)} ms`);
      }),
    );
  }
}

export const router = new RouterState();
