// Account/session state. Rust owns the session and pushes `authChanged`; the
// initial value comes from the boot payload so the first paint is correct.

import { boot } from '../boot';
import { commands, events, type AuthStatus_Serialize, type EpicPage } from '../bindings';
import { library } from './library.svelte';
import { ui } from './ui.svelte';

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

  private apply(s: AuthStatus) {
    this.status = s;
    if (s.state === 'signedIn') {
      library.account = {
        accountId: s.accountId,
        displayName: s.displayName,
        email: s.email ?? library.account?.email ?? null,
        country: s.country ?? library.account?.country ?? null,
        signedIn: true,
      };
    } else if (s.state === 'signedOut' && library.account) {
      library.account = { ...library.account, signedIn: false };
    }
  }

  /** Subscribe once at startup, then sync in case we missed an event. */
  async init() {
    await events.authChanged.listen((e) => this.apply(e.payload));
    this.apply(await commands.authStatus());
  }

  async signIn() {
    const res = await commands.signIn();
    if (res.status === 'error') ui.toast(res.error.message);
  }

  async signOut() {
    const res = await commands.signOut();
    if (res.status === 'error') ui.toast(res.error.message);
  }

  async openEpicPage(page: EpicPage) {
    if (!this.signedIn) {
      ui.toast('Sign in first to open your Epic account pages.');
      return;
    }
    const res = await commands.openEpicPage(page);
    if (res.status === 'error') ui.toast(res.error.message);
  }

  async openCorrectiveAction() {
    const res = await commands.openCorrectiveAction();
    if (res.status === 'error') ui.toast(res.error.message);
  }
}

export const auth = new AuthState();
