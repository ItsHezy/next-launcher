<script lang="ts">
  import type { Component } from 'svelte';
  import { boot } from './lib/boot';
  import Sidebar from './lib/components/Sidebar.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import { router, type Route } from './lib/state/router.svelte';
  import { applyAccent } from './lib/state/settings.svelte';
  import { ui } from './lib/state/ui.svelte';
  import Library from './routes/library/Library.svelte';

  applyAccent();

  type Lazy = Exclude<Route, 'library'>;
  const loaders: Record<Lazy, () => Promise<{ default: Component }>> = {
    store: () => import('./routes/store/Store.svelte'),
    unreal: () => import('./routes/web/Unreal.svelte'),
    fab: () => import('./routes/web/Fab.svelte'),
    settings: () => import('./routes/settings/Settings.svelte'),
  };

  let views = $state.raw<Partial<Record<Lazy, Component>>>({});

  function load(r: Lazy) {
    if (views[r]) return;
    loaders[r]().then((m) => {
      if (!views[r]) views = { ...views, [r]: m.default };
    });
  }

  const VIEW_LABELS: Record<string, string> = { store: 'Store', unreal: 'Unreal Engine', fab: 'Fab' };

  // Load a lazy route as soon as it is visited.
  $effect(() => {
    if (router.route !== 'library') load(router.route);
  });

  // Prefetch every lazy route once the app is idle (after first paint).
  $effect(() => {
    const idle = window.requestIdleCallback ?? ((cb: () => void) => setTimeout(cb, 1500));
    const handle = idle(() => (Object.keys(loaders) as Lazy[]).forEach(load), { timeout: 4000 });
    return () => window.cancelIdleCallback?.(handle as number);
  });

  const lazyMain: Lazy[] = ['store', 'unreal', 'fab'];
</script>

<div class="app">
  <div class="shell" hidden={router.route === 'settings'}>
    <Sidebar />
    <div class="right">
      <TopBar />
      <main class="views">
        <section class="view" hidden={router.route !== 'library'} aria-label="Library">
          <Library />
        </section>
        {#each lazyMain as r (r)}
          {#if router.visited[r]}
            {@const View = views[r]}
            <section class="view" hidden={router.route !== r} aria-label={VIEW_LABELS[r]}>
              {#if View}
                <View />
              {:else}
                <div class="skeleton" aria-busy="true" aria-label="Loading">
                  {#each Array(10) as _, i (i)}<div class="sk"></div>{/each}
                </div>
              {/if}
            </section>
          {/if}
        {/each}
      </main>
    </div>
  </div>

  {#if router.visited.settings}
    {@const SettingsView = views.settings}
    <div class="full" hidden={router.route !== 'settings'}>
      {#if SettingsView}<SettingsView />{/if}
    </div>
  {/if}

  {#if ui.dialog === 'import'}
    {#await import('./lib/components/ImportDialog.svelte') then { default: ImportDialog }}
      <ImportDialog />
    {/await}
  {:else if ui.dialog?.kind === 'install'}
    {#await import('./lib/components/InstallDialog.svelte') then { default: InstallDialog }}
      <InstallDialog appName={ui.dialog.appName} />
    {/await}
  {:else if ui.dialog?.kind === 'uninstall'}
    {#await import('./lib/components/UninstallDialog.svelte') then { default: UninstallDialog }}
      <UninstallDialog appName={ui.dialog.appName} />
    {/await}
  {:else if ui.dialog?.kind === 'launch'}
    {#await import('./lib/components/LaunchPrompt.svelte') then { default: LaunchPrompt }}
      <LaunchPrompt appName={ui.dialog.appName} code={ui.dialog.code} message={ui.dialog.message} />
    {/await}
  {:else if ui.dialog?.kind === 'account'}
    {#await import('./lib/components/AccountDialog.svelte') then { default: AccountDialog }}
      <AccountDialog what={ui.dialog.what} />
    {/await}
  {:else if ui.dialog?.kind === 'licenses'}
    {#await import('./lib/components/LicensesDialog.svelte') then { default: LicensesDialog }}
      <LicensesDialog />
    {/await}
  {:else if ui.dialog?.kind === 'appUpdate'}
    {#await import('./lib/components/AppUpdateDialog.svelte') then { default: AppUpdateDialog }}
      <AppUpdateDialog version={ui.dialog.version} notes={ui.dialog.notes} />
    {/await}
  {:else if ui.dialog?.kind === 'saves'}
    {#await import('./lib/components/SaveConflictDialog.svelte') then { default: SaveConflictDialog }}
      <SaveConflictDialog
        appName={ui.dialog.appName}
        title={ui.dialog.title}
        localTime={ui.dialog.localTime}
        cloudTime={ui.dialog.cloudTime}
        launch={ui.dialog.launch}
      />
    {/await}
  {:else if ui.dialog?.kind === 'move'}
    {#await import('./lib/components/MoveDialog.svelte') then { default: MoveDialog }}
      <MoveDialog appName={ui.dialog.appName} />
    {/await}
  {/if}

  {#if boot.bootError}
    <div class="boot-error" role="alert">{boot.bootError}</div>
  {/if}
  <Toasts />
</div>

<style>
  .app {
    height: 100%;
  }
  .shell {
    display: flex;
    gap: var(--gap);
    height: 100%;
    padding: var(--gap);
  }
  .right {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--gap);
  }
  .views {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .view {
    position: absolute;
    inset: 0;
  }
  .full {
    position: fixed;
    inset: 0;
  }
  [hidden] {
    display: none !important;
  }
  .skeleton {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 28px 20px;
    padding: 96px 32px;
  }
  .sk {
    aspect-ratio: 3 / 4;
    border-radius: var(--radius-md);
    background: var(--surface);
  }
  .boot-error {
    position: fixed;
    left: 50%;
    top: 16px;
    transform: translateX(-50%);
    max-width: 640px;
    padding: 12px 18px;
    border-radius: var(--radius-md);
    background: #4a2622;
    color: var(--text);
    border: 1px solid rgba(255, 138, 128, 0.4);
    z-index: 200;
  }
</style>
