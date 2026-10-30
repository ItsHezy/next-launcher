// App version and update status for Settings → About (read once, from Rust).

import { commands, events } from '../../lib/bindings';

export const appVersion = $state({ value: '…' });
/** Set when the daily background check found a newer version. */
export const appUpdate = $state<{ version: string | null }>({ version: null });

void commands.appInfo().then((i) => (appVersion.value = i.version));
void events.appUpdateAvailable.listen((e) => (appUpdate.version = e.payload.version));
