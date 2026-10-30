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
  | { kind: 'web'; label: string; desc?: string; action: Action }
  /** A value with nothing to change here. */
  | { kind: 'info'; label: string; desc?: string; value: string };

/** What a row does when used. Phase numbers say where the real behaviour lands. */
export type Action =
  | { type: 'soon'; what: string; phase: number }
  /** Epic's own page, signed in, shown inside Settings (ADR 0016). */
  | { type: 'web'; page: EpicPage }
  | { type: 'goto'; section: SettingsSection }
  | { type: 'import' }
  /** An in-app dialog; `fallback` opens instead when Epic doesn't let us do it in-app. */
  | { type: 'dialog'; dialog: 'displayName' | 'language' | 'redeem' | 'hidden' | 'licenses' | 'license'; fallback?: EpicPage }
  | { type: 'cmd'; cmd: 'checkConnection' | 'checkFolder' | 'checkDisk' | 'openLogs' | 'checkAppUpdate' };

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

const web = (page: EpicPage): Action => ({ type: 'web', page });
const soon = (what: string, phase: number): Action => ({ type: 'soon', what, phase });
const cmd = (c: Extract<Action, { type: 'cmd' }>['cmd']): Action => ({ type: 'cmd', cmd: c });

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
            { kind: 'value', label: 'Display name', value: '{displayName}', action: { type: 'dialog', dialog: 'displayName', fallback: 'accountPersonal' } },
            { kind: 'value', label: 'Email', desc: "Changed on Epic's page", value: '{email}', action: web('accountPersonal') },
            { kind: 'value', label: 'Country', desc: "Changed on Epic's page", value: '{country}', action: web('accountPersonal') },
            { kind: 'value', label: 'Communication language', value: '{language}', action: { type: 'dialog', dialog: 'language', fallback: 'accountPersonal' } },
          ],
        },
        {
          title: 'Sign-in & security',
          rows: [
            { kind: 'web', label: 'Password', desc: 'Change your Epic account password', action: web('password') },
            { kind: 'value', label: 'Two-factor authentication', value: '{tfa}', action: web('password') },
          ],
        },
        {
          title: 'Connected accounts',
          rows: [
            { kind: 'value', label: 'PlayStation', value: '{conn.psn}', action: web('connections') },
            { kind: 'value', label: 'Xbox', value: '{conn.xbl}', action: web('connections') },
            { kind: 'value', label: 'Nintendo', value: '{conn.nintendo}', action: web('connections') },
            { kind: 'value', label: 'Steam', value: '{conn.steam}', action: web('connections') },
            { kind: 'value', label: 'Discord', value: '{conn.discord}', action: web('connections') },
            { kind: 'value', label: 'Other connected accounts', value: '{conn.other}', action: web('connections') },
          ],
        },
        {
          title: 'Privacy',
          rows: [
            { kind: 'web', label: 'Privacy & your data', desc: "Epic's account settings page: privacy, data download", action: web('accountPersonal') },
            { kind: 'web', label: 'Communication preferences', desc: 'Emails and messages from Epic', action: web('emailPreferences') },
          ],
        },
        {
          rows: [
            { kind: 'web', label: 'Sign out of all devices', desc: "On Epic's password & security page (Epic doesn't let launchers do it)", action: web('password') },
            { kind: 'danger', label: 'Delete account', desc: "Opens Epic's account page, where deletion is Epic's own flow", button: 'Delete account…', action: web('accountPersonal') },
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
            { kind: 'value', label: 'Wallet balance', value: '{balance}', action: web('wallet') },
            { kind: 'value', label: 'Epic Rewards', value: '{rewards}', action: web('rewards') },
            { kind: 'button', label: 'Add funds', desc: "On Epic's page; you confirm any payment there", button: 'Add funds', action: web('wallet') },
            { kind: 'button', label: 'Redeem Epic gift card', button: 'Redeem', action: web('redeem') },
            { kind: 'button', label: 'Redeem Fortnite V-Bucks card', button: 'Redeem', action: web('vbucksCard') },
            { kind: 'button', label: 'Redeem code', desc: 'Game keys and other codes', button: 'Enter code', action: { type: 'dialog', dialog: 'redeem', fallback: 'redeem' } },
          ],
        },
        {
          title: 'History & payments',
          rows: [
            { kind: 'web', label: 'Transaction history', action: web('transactions') },
            { kind: 'web', label: 'Payment methods', action: web('paymentMethods') },
            { kind: 'web', label: 'Refund requests', desc: 'From a purchase in your transaction history', action: web('transactions') },
            { kind: 'web', label: 'Gifts', action: web('gifts') },
            { kind: 'info', label: 'Coupons', desc: 'Applied at checkout on Epic\'s store', value: '{coupons}' },
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
            { kind: 'toggle', label: 'Run at startup', desc: 'Start Next Launcher when you sign in to Windows', key: 'general.runAtStartup' },
            { kind: 'toggle', label: 'Start minimized', desc: 'When Windows starts it, stay in the tray', key: 'general.startMinimized' },
            {
              kind: 'toggle',
              label: 'Minimize to tray on close',
              desc: 'Closing the window keeps Next Launcher in the tray; quit from the tray menu',
              key: 'general.minimizeToTray',
            },
            {
              kind: 'toggle',
              label: 'Check for app updates',
              desc: 'Once a day; you choose when to install',
              key: 'general.checkAppUpdates',
            },
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
            { kind: 'value', label: 'Hidden games', desc: 'Hide a game from its menu in the library', value: '{hidden}', action: { type: 'dialog', dialog: 'hidden' } },
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
            {
              kind: 'toggle',
              label: 'Add games from the Epic Games Launcher',
              desc: 'Games it installed show up here as installed, without downloading them again. Their folders are not changed.',
              key: 'downloads.autoImport',
            },
            { kind: 'button', label: 'Import existing installation', desc: 'See what the Epic Games Launcher installed and add games by hand', button: 'Import…', action: { type: 'import' } },
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
            { kind: 'info', label: 'Simultaneous downloads', desc: 'One game at a time; the queue continues on its own', value: '1' },
          ],
        },
        {
          title: 'Updates',
          rows: [
            {
              kind: 'toggle',
              label: 'Update games automatically',
              desc: 'Games shared with the Epic Games Launcher update only when you click Update',
              key: 'downloads.autoUpdate',
            },
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
              desc: 'Memory used to reassemble files while downloading (nothing is cached on disk)',
              key: 'downloads.cacheSizeMb',
              options: [128, 256, 512, 1024].map((n) => ({ value: n, label: `${n} MB` })),
            },
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
            { kind: 'toggle', label: 'Free games', desc: "When this week's free games change", key: 'notifications.freeGames' },
            { kind: 'toggle', label: 'News & offers', key: 'notifications.newsOffers', disabled: 'No news source from Epic yet' },
            { kind: 'toggle', label: 'Chat messages', key: 'notifications.chat', disabled: 'Available when friends & chat are supported' },
            { kind: 'toggle', label: 'Invites', key: 'notifications.invites', disabled: 'Available when friends & chat are supported' },
            { kind: 'toggle', label: 'Download finished', desc: 'Installs and updates', key: 'notifications.downloadFinished' },
          ],
        },
        {
          title: 'Behavior',
          rows: [
            { kind: 'toggle', label: 'Play a sound', key: 'notifications.sound' },
            { kind: 'toggle', label: 'Do not disturb while playing', desc: 'No notifications while a game runs', key: 'notifications.dndWhilePlaying' },
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
            {
              kind: 'toggle',
              label: 'Use a proxy',
              desc: "Applies after restarting Next Launcher. Epic's web pages use Windows' proxy settings.",
              key: 'network.proxyEnabled',
            },
            { kind: 'text', label: 'Proxy address', key: 'network.proxyAddress', placeholder: 'http://host:port', enabledBy: 'network.proxyEnabled' },
            { kind: 'button', label: 'Test connection', desc: "Checks that Epic's services can be reached", button: 'Test', action: cmd('checkConnection') },
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
            { kind: 'button', label: 'Connection', desc: "Can Next Launcher reach Epic's servers?", button: 'Check', action: cmd('checkConnection') },
            { kind: 'button', label: 'Folder permissions', desc: 'Can games be written to your install folder?', button: 'Check', action: cmd('checkFolder') },
            { kind: 'button', label: 'Disk space', desc: 'Free space where games install', button: 'Check', action: cmd('checkDisk') },
          ],
        },
        {
          title: 'Logs',
          rows: [
            { kind: 'button', label: 'Logs folder', desc: 'Logs never contain passwords, tokens or your email', button: 'Open folder', action: cmd('openLogs') },
            { kind: 'toggle', label: 'Debug logging', desc: 'More detail in the logs; applies after restarting', key: 'support.debugLogging' },
          ],
        },
        {
          title: 'Help',
          rows: [
            { kind: 'web', label: 'Epic Games help & support', action: web('support') },
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
            { kind: 'web', label: 'Terms of Service', action: web('terms') },
            { kind: 'web', label: 'Privacy Policy', action: web('privacyPolicy') },
            { kind: 'web', label: 'Store Refund Policy', action: web('refundPolicy') },
            { kind: 'web', label: 'Publisher Index', action: web('publisherIndex') },
            { kind: 'web', label: 'End User License Agreement', action: web('eula') },
          ],
          footer: 'Next Launcher is an independent, free (GPL-3.0-or-later) client and is not made by Epic Games.',
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
            { kind: 'info', label: 'Version', value: '{version}' },
            {
              kind: 'value',
              label: 'License',
              desc: 'Free software: you may share and change it under the GPL',
              value: 'GNU GPL v3 or later',
              action: { type: 'dialog', dialog: 'license' },
            },
            { kind: 'button', label: 'Check for updates', desc: '{updateStatus}', button: 'Check', action: cmd('checkAppUpdate') },
            { kind: 'value', label: 'Open-source licenses', value: '', action: { type: 'dialog', dialog: 'licenses' } },
          ],
        },
      ],
    },
  ],
];

export const ALL_SECTIONS: Section[] = SECTIONS.flat();
