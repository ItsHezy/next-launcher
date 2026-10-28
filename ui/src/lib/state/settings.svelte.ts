import { boot } from '../boot';
import { commands, type SettingValue } from '../bindings';

/** Defaults for every setting the UI knows. Stored values override these. */
export const DEFAULTS = {
  'general.language': 'en-US',
  'general.runAtStartup': false,
  'general.startMinimized': false,
  'general.minimizeToTray': true,
  'general.openOn': 'library',
  'general.showUninstalled': true,
  'general.defaultSort': 'title',
  'general.accent': '#0A6FD6',
  'downloads.installDir': 'C:\\Games',
  'downloads.limitMbps': 0,
  'downloads.pauseWhilePlaying': true,
  'downloads.simultaneous': 1,
  'downloads.autoUpdate': true,
  'downloads.updateOnlyIdle': false,
  'downloads.verifyAfterInstall': false,
  'downloads.cacheSizeMb': 256,
  'notifications.freeGames': true,
  'notifications.newsOffers': false,
  'notifications.chat': false,
  'notifications.invites': false,
  'notifications.downloadFinished': true,
  'notifications.sound': true,
  'notifications.dndWhilePlaying': true,
  'cloud.enabled': true,
  'cloud.syncOnStart': true,
  'cloud.syncOnExit': true,
  'cloud.conflict': 'ask',
  'network.proxyEnabled': false,
  'network.proxyAddress': '',
  'support.debugLogging': false,
} as const satisfies Record<string, SettingValue>;

export type SettingKey = keyof typeof DEFAULTS;
type Widen<T> = T extends boolean ? boolean : T extends number ? number : string;
export type SettingOf<K extends SettingKey> = Widen<(typeof DEFAULTS)[K]>;

export const ACCENTS = [
  { value: '#0A6FD6', label: 'Blue' },
  { value: '#248A3D', label: 'Green' },
  { value: '#C93400', label: 'Orange' },
  { value: '#8944AB', label: 'Purple' },
] as const;

class SettingsState {
  values = $state<Record<string, SettingValue>>({ ...DEFAULTS, ...boot.settings });

  get<K extends SettingKey>(key: K): SettingOf<K> {
    return this.values[key] as SettingOf<K>;
  }

  async set<K extends SettingKey>(key: K, value: SettingOf<K>) {
    await this.setRaw(key, value);
  }

  /** Dynamic keys (e.g. `cloud.game.<appName>`). */
  getRaw(key: string, fallback: SettingValue): SettingValue {
    return this.values[key] ?? fallback;
  }

  async setRaw(key: string, value: SettingValue) {
    const prev = this.values[key];
    this.values[key] = value;
    const res = await commands.setSetting(key, value);
    if (res.status === 'error') {
      if (prev === undefined) delete this.values[key];
      else this.values[key] = prev;
    }
  }
}

export const settings = new SettingsState();

/** Apply the accent token whenever it changes. */
export function applyAccent() {
  $effect(() => {
    document.documentElement.style.setProperty('--accent', settings.get('general.accent'));
  });
}
