// Account details, wallet and token capabilities (Phase 7). Rust caches them; this just
// holds the latest copy and refreshes on events.

import {
  commands,
  events,
  type AccountDetails_Serialize as Details,
  type Capabilities,
  type WalletInfo_Serialize as Wallet,
} from '../bindings';

class AccountState {
  details = $state.raw<Details | null>(null);
  wallet = $state.raw<Wallet | null>(null);
  caps = $state.raw<Capabilities | null>(null);
  hiddenCount = $state(0);
  error = $state<string | null>(null);

  async loadDetails(force = false) {
    const res = await commands.accountDetails(force);
    if (res.status === 'ok') {
      this.details = res.data;
      this.caps = res.data.capabilities;
      this.error = null;
    } else this.error = res.error.message;
  }

  async loadWallet(force = false) {
    const res = await commands.walletSummary(force);
    if (res.status === 'ok') this.wallet = res.data;
  }

  async loadHidden() {
    const res = await commands.hiddenGames();
    if (res.status === 'ok') this.hiddenCount = res.data.length;
  }

  async init() {
    await events.accountChanged.listen((e) => {
      if (e.payload.what === 'wallet') void this.loadWallet();
      else void this.loadDetails(true);
    });
    this.caps = await commands.accountCapabilities();
    void this.loadWallet();
  }

  reset() {
    this.details = null;
    this.wallet = null;
    this.caps = null;
  }
}

export const account = new AccountState();

/** Epic's `externalAuths` type → the name people know. */
export const CONNECTIONS: { kind: string; label: string }[] = [
  { kind: 'psn', label: 'PlayStation' },
  { kind: 'xbl', label: 'Xbox' },
  { kind: 'nintendo', label: 'Nintendo' },
  { kind: 'steam', label: 'Steam' },
  { kind: 'discord', label: 'Discord' },
];

const OTHER_NAMES: Record<string, string> = {
  lego: 'LEGO',
  ubisoft: 'Ubisoft',
  github: 'GitHub',
  twitch: 'Twitch',
  google: 'Google',
  apple: 'Apple',
  facebook: 'Facebook',
  vk: 'VK',
};

/** A connected account's display name, for kinds beyond the five main platforms. */
export function connectionName(kind: string): string {
  return CONNECTIONS.find((c) => c.kind === kind)?.label ?? OTHER_NAMES[kind] ?? kind;
}

/** Communication languages Epic accepts for `preferredLanguage` (account.md). */
export const LANGUAGES: { value: string; label: string }[] = [
  { value: 'en', label: 'English' },
  { value: 'ar', label: 'العربية' },
  { value: 'de', label: 'Deutsch' },
  { value: 'es-ES', label: 'Español (España)' },
  { value: 'es-MX', label: 'Español (Latinoamérica)' },
  { value: 'fr', label: 'Français' },
  { value: 'it', label: 'Italiano' },
  { value: 'ja', label: '日本語' },
  { value: 'ko', label: '한국어' },
  { value: 'pl', label: 'Polski' },
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'ru', label: 'Русский' },
  { value: 'th', label: 'ไทย' },
  { value: 'tr', label: 'Türkçe' },
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-Hant', label: '繁體中文' },
];

export function languageName(code: string | null | undefined): string {
  if (!code) return '—';
  return LANGUAGES.find((l) => l.value.toLowerCase() === code.toLowerCase())?.label ?? code;
}
