// Account/session state. Rust owns the session and pushes `authChanged`; the
// initial value comes from the boot payload so the first paint is correct.

import { boot } from '../boot';
import { commands, events, type AuthStatus_Serialize, type EpicPage } from '../bindings';
import { library } from './library.svelte';
import { router, type Route, type SettingsSection } from './router.svelte';
import { ui } from './ui.svelte';
import { account } from './account.svelte';

export type AuthStatus = AuthStatus_Serialize;

class AuthState {
  status = $state.raw<AuthStatus>(boot.auth);

  /** Shown name while signed in, restoring, offline or needing action. */
  displayName = $derived.by(() => {
    const s = this.status;
    return 'displayName' in s && s.displayName ? s.displayName : (library.account?.displayName ?? '');
  });

  /** Signed in, or holding a saved session we expect to restore. */
  hasAccount = $derived(this.status.state !== 'signedOut' && this.status.state !== 'signingIn');

  signedIn = $derived(this.status.state === 'signedIn');

  /** Where Sign in was pressed: signing in returns there. */
  #from: Route = 'library';

  private apply(s: AuthStatus) {
    this.status = s;
    this.#leaveSignIn(s);
    if (s.state === 'signedIn') {
      library.account = {
        accountId: s.accountId,
        displayName: s.displayName,
        email: s.email ?? library.account?.email ?? null,
        country: s.country ?? library.account?.country ?? null,
        signedIn: true,
      };
    } else if (s.state === 'signedOut') {
      if (library.account) library.account = { ...library.account, signedIn: false };
      account.reset();
    }
  }

  /** Subscribe once at startup, then sync in case we missed an event. */
  async init() {
    await events.authChanged.listen((e) => this.apply(e.payload));
    this.apply(await commands.authStatus());
  }

  /** Epic's sign-in page, inside Settings › Account (no second window). */
  signIn() {
    this.#from = router.route;
    router.show('settings', { kind: 'signIn' }, 'Sign in', 'account');
  }

  /** Epic's sign-in page answered: leave it, or go on to the step Epic asked for. */
  #leaveSignIn(s: AuthStatus) {
    if (router.web?.target.kind !== 'signIn' || s.state === 'signingIn') return;
    if (s.state === 'needsAction') {
      this.openCorrectiveAction();
      return;
    }
    router.web = null;
    if (s.state === 'signedOut' && s.reason) ui.toast(s.reason);
    if (s.state === 'signedIn' && this.#from !== 'settings') router.go(this.#from);
  }

  async signOut() {
    const res = await commands.signOut();
    if (res.status === 'error') ui.toast(res.error.message);
  }

  /** Show Epic's page inside Settings, in the section it belongs to (no second window). */
  openEpicPage(page: EpicPage) {
    if (!this.signedIn && !PUBLIC_PAGES.has(page)) {
      ui.toast('Sign in first to open your Epic account pages.');
      return;
    }
    router.show('settings', { kind: 'page', page }, PAGE_TITLES[page], PAGE_SECTION[page]);
  }

  /** The step Epic asked for before the account can be used, inside Settings › Account. */
  openCorrectiveAction() {
    router.show('settings', { kind: 'corrective' }, 'Confirm on Epic', 'account');
  }
}

/** Epic pages that need no sign-in (help and legal). */
const PUBLIC_PAGES = new Set<EpicPage>(['support', 'terms', 'privacyPolicy', 'refundPolicy', 'publisherIndex', 'eula']);

const PAGE_SECTION: Record<EpicPage, SettingsSection> = {
  accountPersonal: 'account',
  password: 'account',
  connections: 'account',
  emailPreferences: 'account',
  transactions: 'wallet',
  paymentMethods: 'wallet',
  wallet: 'wallet',
  redeem: 'wallet',
  gifts: 'wallet',
  rewards: 'wallet',
  vbucksCard: 'wallet',
  support: 'support',
  terms: 'legal',
  privacyPolicy: 'legal',
  refundPolicy: 'legal',
  publisherIndex: 'legal',
  eula: 'legal',
};

const PAGE_TITLES: Record<EpicPage, string> = {
  accountPersonal: 'Account settings',
  password: 'Password & security',
  connections: 'Connections',
  emailPreferences: 'Communication preferences',
  transactions: 'Transaction history',
  paymentMethods: 'Payment methods',
  wallet: 'Wallet',
  redeem: 'Redeem a code',
  gifts: 'Gifts',
  rewards: 'Epic Rewards',
  vbucksCard: 'Redeem a V-Bucks card',
  support: 'Help & support',
  terms: 'Terms of Service',
  privacyPolicy: 'Privacy Policy',
  refundPolicy: 'Store Refund Policy',
  publisherIndex: 'Publisher Index',
  eula: 'End User License Agreement',
};

export const auth = new AuthState();
