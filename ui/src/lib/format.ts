const units = ['B', 'KB', 'MB', 'GB', 'TB'];

export function bytes(n: number): string {
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

export function speed(bps: number): string {
  return `${bytes(bps)}/s`;
}

export function duration(secs: number): string {
  if (!Number.isFinite(secs) || secs <= 0) return '—';
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min`;
  return `${Math.ceil(secs)} s`;
}

export function playtime(secs: number): string {
  if (secs < 60) return 'Not played yet';
  const h = secs / 3600;
  return h < 1 ? `${Math.round(secs / 60)} min played` : `${h < 10 ? h.toFixed(1) : Math.round(h)} h played`;
}

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

export function ago(unixSecs: number): string {
  const diff = unixSecs - Date.now() / 1000;
  const abs = Math.abs(diff);
  if (abs < 3600) return rtf.format(Math.round(diff / 60), 'minute');
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), 'hour');
  if (abs < 86400 * 30) return rtf.format(Math.round(diff / 86400), 'day');
  return rtf.format(Math.round(diff / (86400 * 30)), 'month');
}

export function initials(name: string): string {
  const parts = name.split(/\s+/).filter((p) => /[A-Za-z0-9]/.test(p[0] ?? ''));
  return ((parts[0]?.[0] ?? '') + (parts[1]?.[0] ?? '')).toUpperCase() || '?';
}

/** Display name of the store that manages a third-party title (catalog provider id). */
export function storeName(provider: string): string {
  const p = provider.toLowerCase();
  if (p.includes('ubisoft') || p.includes('uplay')) return 'Ubisoft Connect';
  if (p.includes('origin') || p.includes('ea')) return 'EA app';
  return provider;
}
