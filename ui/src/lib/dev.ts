// Dev tooling: `--dev-route <route>[:<section>|:profile|:wallet]` opens a view after
// startup so tools/screenshot.ps1 can capture every screen. Never used by users.

import { router, type Route, type SettingsSection } from './state/router.svelte';
import { ui } from './state/ui.svelte';

const ROUTES: Route[] = ['library', 'store', 'unreal', 'fab', 'settings'];

export function applyDevRoute(spec: string) {
  const [route, extra] = spec.split(':') as [string, string | undefined];
  if (!ROUTES.includes(route as Route)) return;
  router.go(route as Route, route === 'settings' ? (extra as SettingsSection | undefined) : undefined);
  if (extra === 'profile' || extra === 'wallet') ui.popover = extra;
}
