// Game actions shared by the library, its context menu and Settings → Manage games.

import { commands, type LaunchOptions_Deserialize } from './bindings';
import { num } from './boot';
import { storeName } from './format';
import { library, type Game } from './state/library.svelte';
import { ui } from './state/ui.svelte';

/** Launch answers the UI can give: these errors open a question instead of a toast. */
const PROMPTS = new Set(['prereq', 'offline', 'needsOnline']);

/** Play (or install, or open in the store that manages it). */
export async function play(g: Game, options: LaunchOptions_Deserialize = { offline: false }) {
  const installed = g.state === 'installed' || g.state === 'updateAvailable';
  if (!installed && !g.thirdParty) {
    if (g.state === 'notInstalled' && g.installable) install(g.appName);
    return;
  }
  if (library.running.has(g.appName) || library.starting.has(g.appName)) return;
  library.setStarting(g.appName, true);
  const res = await commands.launchGame(g.appName, options);
  library.setStarting(g.appName, false);
  if (res.status === 'ok') {
    if (!installed && g.thirdParty) ui.toast(`Opening ${g.title} in ${storeName(g.thirdParty)}.`);
    return;
  }
  if (res.error.code === 'saveConflict') {
    // "title|local ISO|cloud ISO" from the launch command.
    const [title = g.title, localTime = '', cloudTime = ''] = res.error.message.split('|');
    ui.dialog = { kind: 'saves', appName: g.appName, title, localTime, cloudTime, launch: true };
    return;
  }
  if (PROMPTS.has(res.error.code)) {
    ui.dialog = { kind: 'launch', appName: g.appName, code: res.error.code, message: res.error.message };
    return;
  }
  ui.toast(res.error.message);
}

export function install(appName: string) {
  ui.dialog = { kind: 'install', appName };
}

export function uninstall(appName: string) {
  ui.dialog = { kind: 'uninstall', appName };
}

export function move(appName: string) {
  ui.dialog = { kind: 'move', appName };
}

/** Hide from the library (shown again from Settings → General → Hidden games). */
export async function hide(appName: string, title: string) {
  const res = await commands.setHidden(appName, true);
  ui.toast(res.status === 'ok' ? `${title} is hidden. Settings → General shows hidden games.` : res.error.message);
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
