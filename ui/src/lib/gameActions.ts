// Game actions shared by the library, its context menu and Settings → Manage games.

import { commands } from './bindings';
import { num } from './boot';
import { ui } from './state/ui.svelte';

export function install(appName: string) {
  ui.dialog = { kind: 'install', appName };
}

export function uninstall(appName: string) {
  ui.dialog = { kind: 'uninstall', appName };
}

export function move(appName: string) {
  ui.dialog = { kind: 'move', appName };
}

export async function update(appName: string, title: string) {
  const res = await commands.startUpdate(appName);
  ui.toast(res.status === 'ok' ? `Updating ${title}.` : res.error.message);
}

/** Verify every file; queue a repair only if something is wrong. */
export async function verifyAndRepair(appName: string, title: string) {
  ui.toast(`Checking ${title}'s files…`);
  const res = await commands.verifyGame(appName);
  if (res.status === 'error') {
    ui.toast(res.error.message);
    return;
  }
  const problems = res.data.problems.length;
  if (problems === 0) {
    ui.toast(`${title}: all ${num(res.data.filesChecked)} files are OK.`);
    return;
  }
  const repair = await commands.repairGame(appName);
  ui.toast(
    repair.status === 'ok'
      ? `${title}: ${problems} file${problems === 1 ? '' : 's'} damaged or missing. Repairing them now.`
      : repair.error.message,
  );
}
