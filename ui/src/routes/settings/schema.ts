// Declarative Settings content (brief section 6). The renderer is generic; a few
// sections add a custom block (profile card, Manage games, per-game cloud saves, About).

import type { EpicPage } from '../../lib/bindings';
import type { IconName } from '../../lib/icons';
import type { SettingsSection } from '../../lib/state/router.svelte';
import type { SettingKey } from '../../lib/state/settings.svelte';

export type Row =
  | { kind: 'toggle'; label: string; desc?: string; key: SettingKey; disabled?: string }
  | { kind: 'select'; label: string; desc?: string; key: SettingKey; options: { value: string | number; label: string }[] }
  | { kind: 'accent'; label: string; desc?: string }
  | { kind: 'folder'; label: string; desc?: string; key: SettingKey }
  | { kind: 'text'; label: string; desc?: string; key: SettingKey; placeholder?: string; enabledBy?: SettingKey }
  | { kind: 'value'; label: string; desc?: string; value: string; action: Action }
  | { kind: 'button'; label: string; desc?: string; button: string; action: Action }
  | { kind: 'danger'; label: string; desc?: string; button: string; action: Action }
  | { kind: 'web'; label: string; desc?: string; action: Action };

/** What a row does when used. Phase numbers say where the real behaviour lands. */
export type Action =
  | { type: 'soon'; what: string; phase: number }
  | { type: 'web'; what: string; phase: number; page?: EpicPage }
  | { type: 'goto'; section: SettingsSection };

export interface Group {
  title?: string;
  rows: Row[];
  footer?: string;
}

export interface Section {
  id: SettingsSection;
  label: string;
  icon: IconName;
  tile: string; // icon tile colour (white icon on it, ≥ 3:1)
  custom?: 'account' | 'games' | 'cloudGames' | 'about';
  groups: Group[];
}

/** An Epic page; with `page` it opens signed in now, otherwise it lands in `phase`. */
const web = (what: string, phase = 7, page?: EpicPage): Action => ({ type: 'web', what, phase, page });
const soon = (what: string, phase: number): Action => ({ type: 'soon', what, phase });

export const SECTIONS: Section[][] = [
  [
    {
      id: 'account',
      label: 'Account',
      icon: 'user',
      tile: '#0A6FD6',
      custom: 'account',
      groups: [
        {
          title: 'Profile',
          rows: [
            { kind: 'value', label: 'Display name', value: '{displayName}', action: soon('Changing your display name', 7) },
            { kind: 'value', label: 'Email', value: '{email}', action: web('Change email', 7, 'accountPersonal') },
            { kind: 'value', label: 'Country', value: '{country}', action: web('Change country', 7, 'accountPersonal') },
            { kind: 'value', label: 'Communication language', value: 'English', action: soon('Communication language', 7) },
          ],
        },
        {
          title: 'Sign-in & security',
          rows: [
            { kind: 'web', label: 'Password', desc: 'Change your Epic account password', action: web('Password', 7, 'password') },
            { kind: 'value', label: 'Two-factor authentication', value: 'On', action: web('Two-factor authentication', 7, 'password') },
            { kind: 'web', label: 'Active sessions', action: web('Active sessions') },
            { kind: 'web', label: 'Sign-in history', action: web('Sign-in history') },
          ],
        },
        {
          title: 'Connected accounts',
          rows: [
            { kind: 'value', label: 'PlayStation', value: 'Not connected', action: web('Connections', 7, 'connections') },
            { kind: 'value', label: 'Xbox', value: 'Connected', action: web('Connections', 7, 'connections') },
            { kind: 'value', label: 'Nintendo', value: 'Not connected', action: web('Connections', 7, 'connections') },
            { kind: 'value', label: 'Steam', value: 'Not connected', action: web('Connections', 7, 'connections') },
            { kind: 'value', label: 'Discord', value: 'Not connected', action: web('Connections', 7, 'connections') },
          ],
        },
        {
          title: 'Privacy',
          rows: [
            { kind: 'web', label: 'Privacy settings', desc: 'Who can find you, see your activity and send requests', action: web('Privacy settings', 7, 'privacy') },
            { kind: 'web', label: 'Download my data', action: web('Download my data', 7, 'privacy') },
          ],
        },
        {
          rows: [
            { kind: 'button', label: 'Sign out of all devices', desc: 'Ends every session except this one', button: 'Sign out everywhere', action: soon('Signing out of other devices', 7) },
            { kind: 'danger', label: 'Delete account', desc: "Opens Epic's account deletion page", button: 'Delete account…', action: web('Account deletion', 7, 'accountPersonal') },
          ],
        },
      ],
    },
    {
      id: 'wallet',
      label: 'Wallet & rewards',
      icon: 'wallet',
      tile: '#248A3D',
      groups: [
        {
          title: 'Balance',
          rows: [
            { kind: 'value', label: 'Wallet balance', value: '{balance}', action: web('Wallet', 7, 'wallet') },
            { kind: 'value', label: 'Epic Rewards', value: '{rewards}', action: web('Epic Rewards') },
            { kind: 'button', label: 'Add funds', button: 'Add funds', action: web('Add funds', 7, 'wallet') },
            { kind: 'button', label: 'Redeem gift card', desc: 'Epic Games and Fortnite gift cards', button: 'Redeem', action: web('Redeem gift card', 7, 'redeem') },
            { kind: 'button', label: 'Redeem code', button: 'Enter code', action: soon('Redeeming codes', 7) },
          ],
        },
        {
          title: 'History & payments',
          rows: [
            { kind: 'web', label: 'Transaction history', action: web('Transaction history', 7, 'transactions') },
            { kind: 'web', label: 'Payment methods', action: web('Payment methods', 7, 'paymentMethods') },
            { kind: 'web', label: 'Refund requests', action: web('Refund requests', 7, 'transactions') },
            { kind: 'web', label: 'Gifts', action: web('Gifts') },
            { kind: 'value', label: 'Coupons', value: '{coupons}', action: web('Coupons') },
          ],
        },
      ],
    },
  ],
  [
    {
      id: 'general',
      label: 'General',
      icon: 'sliders',
      tile: '#5E5E6B',
      groups: [
        {
          title: 'App',
          rows: [
            { kind: 'select', label: 'Language', key: 'general.language', options: [{ value: 'en-US', label: 'English (US)' }] },
            { kind: 'toggle', label: 'Run at startup', key: 'general.runAtStartup' },
            { kind: 'toggle', label: 'Start minimized', key: 'general.startMinimized' },
            { kind: 'toggle', label: 'Minimize to tray on close', key: 'general.minimizeToTray' },
            {
              kind: 'select',
              label: 'Open on',
              key: 'general.openOn',
              options: [
                { value: 'library', label: 'Library' },
                { value: 'unreal', label: 'Unreal Engine' },
                { value: 'fab', label: 'Fab' },
              ],
            },
          ],
        },
        {
          title: 'Library',
          rows: [
            { kind: 'value', label: 'Hidden games', desc: 'Games you hid from your library', value: 'None', action: soon('Hiding games', 3) },
            { kind: 'toggle', label: 'Show uninstalled games', key: 'general.showUninstalled' },
            {
              kind: 'select',
              label: 'Default sort',
              key: 'general.defaultSort',
              options: [
                { value: 'title', label: 'Title' },
                { value: 'recent', label: 'Recently played' },
                { value: 'playtime', label: 'Most played' },
                { value: 'size', label: 'Install size' },
              ],
            },
          ],
        },
        { title: 'Appearance', rows: [{ kind: 'accent', label: 'Accent color' }] },
      ],
    },
    {
      id: 'downloads',
      label: 'Downloads & installs',
      icon: 'download',
      tile: '#0A6FD6',
      groups: [
        {
          title: 'Location',
          rows: [
            { kind: 'folder', label: 'Default install folder', key: 'downloads.installDir' },
            { kind: 'button', label: 'Import existing installation', desc: 'Use games already installed by the Epic Games Launcher', button: 'Import…', action: soon('Importing installs', 3) },
          ],
        },
        {
          title: 'Speed',
          rows: [
            {
              kind: 'select',
              label: 'Download limit',
              key: 'downloads.limitMbps',
              options: [
                { value: 0, label: 'Unlimited' },
                { value: 5, label: '5 MB/s' },
                { value: 10, label: '10 MB/s' },
                { value: 25, label: '25 MB/s' },
                { value: 50, label: '50 MB/s' },
                { value: 100, label: '100 MB/s' },
              ],
            },
            { kind: 'toggle', label: 'Pause downloads while playing', key: 'downloads.pauseWhilePlaying' },
            {
              kind: 'select',
              label: 'Simultaneous downloads',
              key: 'downloads.simultaneous',
              options: [1, 2, 3].map((n) => ({ value: n, label: String(n) })),
            },
          ],
        },
        {
          title: 'Updates',
          rows: [
            { kind: 'toggle', label: 'Update games automatically', key: 'downloads.autoUpdate' },
            { kind: 'toggle', label: 'Update only when idle', desc: 'Wait until you are not using the computer', key: 'downloads.updateOnlyIdle' },
            { kind: 'toggle', label: 'Verify files after install', key: 'downloads.verifyAfterInstall' },
          ],
        },
        {
          title: 'Download cache',
          rows: [
            {
              kind: 'select',
              label: 'Cache size',
              desc: 'Memory used to reassemble files while downloading',
              key: 'downloads.cacheSizeMb',
              options: [128, 256, 512, 1024].map((n) => ({ value: n, label: `${n} MB` })),
            },
            { kind: 'button', label: 'Clear download cache', button: 'Clear', action: soon('Download cache', 4) },
          ],
        },
      ],
    },
    {
      id: 'notifications',
      label: 'Notifications',
      icon: 'bell',
      tile: '#C93400',
      groups: [
        {
          title: 'Notify me about',
          rows: [
            { kind: 'toggle', label: 'Free games', key: 'notifications.freeGames' },
            { kind: 'toggle', label: 'News & offers', key: 'notifications.newsOffers' },
            { kind: 'toggle', label: 'Chat messages', key: 'notifications.chat', disabled: 'Available when friends & chat are supported' },
            { kind: 'toggle', label: 'Invites', key: 'notifications.invites', disabled: 'Available when friends & chat are supported' },
            { kind: 'toggle', label: 'Download finished', key: 'notifications.downloadFinished' },
          ],
        },
        {
          title: 'Behavior',
          rows: [
            { kind: 'toggle', label: 'Play a sound', key: 'notifications.sound' },
            { kind: 'toggle', label: 'Do not disturb while playing', key: 'notifications.dndWhilePlaying' },
          ],
        },
      ],
    },
    {
      id: 'cloud',
      label: 'Cloud saves',
      icon: 'cloud',
      tile: '#2F7FC8',
      custom: 'cloudGames',
      groups: [
        {
          rows: [
            { kind: 'toggle', label: 'Enable cloud saves', key: 'cloud.enabled' },
            { kind: 'toggle', label: 'Sync when a game starts', key: 'cloud.syncOnStart' },
            { kind: 'toggle', label: 'Sync when a game exits', key: 'cloud.syncOnExit' },
            {
              kind: 'select',
              label: 'When saves conflict',
              key: 'cloud.conflict',
              options: [
                { value: 'ask', label: 'Ask me' },
                { value: 'local', label: 'Prefer this PC' },
                { value: 'cloud', label: 'Prefer cloud' },
              ],
            },
          ],
        },
      ],
    },
    {
      id: 'network',
      label: 'Network',
      icon: 'globe',
      tile: '#248A3D',
      groups: [
        {
          title: 'Proxy',
          rows: [
            { kind: 'toggle', label: 'Use a proxy', key: 'network.proxyEnabled' },
            { kind: 'text', label: 'Proxy address', key: 'network.proxyAddress', placeholder: 'http://host:port', enabledBy: 'network.proxyEnabled' },
            { kind: 'button', label: 'Test connection', desc: "Checks that Epic's services can be reached", button: 'Test', action: soon('Connection test', 2) },
          ],
        },
      ],
    },
  ],
  [{ id: 'games', label: 'Manage games', icon: 'gamepad', tile: '#8944AB', custom: 'games', groups: [] }],
  [
    {
      id: 'support',
      label: 'Support',
      icon: 'lifebuoy',
      tile: '#B3471B',
      groups: [
        {
          title: 'Troubleshoot',
          rows: [
            { kind: 'button', label: 'Connection', desc: "Can Next Launcher reach Epic's servers?", button: 'Check', action: soon('Connection check', 2) },
            { kind: 'button', label: 'Folder permissions', desc: 'Can games be written to your install folder?', button: 'Check', action: soon('Permission check', 4) },
            { kind: 'button', label: 'Disk space', button: 'Check', action: soon('Disk check', 4) },
          ],
        },
        {
          title: 'Logs',
          rows: [
            { kind: 'button', label: 'Logs folder', desc: 'Logs never contain passwords, tokens or your email', button: 'Open folder', action: soon('Opening the logs folder', 9) },
            { kind: 'toggle', label: 'Debug logging', key: 'support.debugLogging' },
          ],
        },
        {
          title: 'Help',
          rows: [
            { kind: 'web', label: 'Contact Epic support', action: web('Epic support', 7) },
            { kind: 'web', label: 'Report a Next Launcher bug', action: soon('Bug reports', 9) },
          ],
        },
      ],
    },
    {
      id: 'legal',
      label: 'Legal & policies',
      icon: 'document',
      tile: '#5E5E6B',
      groups: [
        {
          rows: [
            { kind: 'web', label: 'Terms of Service', action: web('Terms of Service') },
            { kind: 'web', label: 'Privacy Policy', action: web('Privacy Policy') },
            { kind: 'web', label: 'Store Refund Policy', action: web('Store Refund Policy') },
            { kind: 'web', label: 'Publisher Index', action: web('Publisher Index') },
            { kind: 'web', label: 'End User License Agreement', action: web('EULA') },
          ],
          footer: 'Next Launcher is an independent client and is not made by Epic Games.',
        },
      ],
    },
    {
      id: 'about',
      label: 'About',
      icon: 'info',
      tile: '#0A6FD6',
      custom: 'about',
      groups: [
        {
          rows: [
            { kind: 'value', label: 'Version', value: '0.1.0 (Phase 1 preview)', action: soon('Release notes', 9) },
            { kind: 'button', label: 'Check for updates', button: 'Check', action: soon('App updates', 9) },
            { kind: 'web', label: 'Open-source licenses', action: soon('License list', 9) },
          ],
        },
      ],
    },
  ],
];

export const ALL_SECTIONS: Section[] = SECTIONS.flat();
