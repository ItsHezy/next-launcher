// App version for Settings → About (read once, from Rust).

import { commands } from '../../lib/bindings';

export const appVersion = $state({ value: '…' });

void commands.appInfo().then((i) => (appVersion.value = i.version));
