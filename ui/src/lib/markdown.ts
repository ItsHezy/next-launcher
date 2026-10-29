// A small, safe Markdown reader for store descriptions. It never produces HTML: the
// result is plain data rendered by Markdown.svelte, so text from Epic's catalog can't
// inject markup or scripts. Supported: headings, paragraphs, bullet lists, **bold**,
// *italic*, images from Epic's CDN. Links keep their text only (no navigation).

export interface Inline {
  text: string;
  bold?: boolean;
  italic?: boolean;
}

export type Block =
  | { kind: 'h'; level: 2 | 3 | 4; inl: Inline[] }
  | { kind: 'p'; inl: Inline[] }
  | { kind: 'ul'; items: Inline[][] }
  | { kind: 'img'; src: string; alt: string };

const IMG_HOSTS = ['https://cdn1.epicgames.com/', 'https://cdn2.unrealengine.com/'];

/** Text with **bold** / *italic* / [links](…) / `code`, as plain runs. */
export function inline(s: string): Inline[] {
  const clean = s
    .replace(/<[^>]*>/g, '') // stray HTML tags: dropped, never rendered
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1') // links: text only
    .replace(/`([^`]*)`/g, '$1')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
  const out: Inline[] = [];
  const re = /(\*\*|__)(.+?)\1|(\*|_)(?=\S)(.+?)(?<=\S)\3/g;
  let last = 0;
  for (const m of clean.matchAll(re)) {
    const at = m.index ?? 0;
    if (at > last) out.push({ text: clean.slice(last, at) });
    if (m[2] !== undefined) out.push({ text: m[2], bold: true });
    else out.push({ text: m[4] ?? '', italic: true });
    last = at + m[0].length;
  }
  if (last < clean.length) out.push({ text: clean.slice(last) });
  return out.filter((r) => r.text.length > 0);
}

export function parse(md: string): Block[] {
  const lines = md
    .replace(/<!--[\s\S]*?-->/g, '\n') // Epic's layout comments
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/\r\n?/g, '\n')
    .split('\n');
  const blocks: Block[] = [];
  let para: string[] = [];
  let list: Inline[][] | null = null;

  const flushPara = () => {
    const text = para.join(' ').trim();
    if (text) blocks.push({ kind: 'p', inl: inline(text) });
    para = [];
  };
  const flushList = () => {
    if (list?.length) blocks.push({ kind: 'ul', items: list });
    list = null;
  };

  for (const raw of lines) {
    const line = raw.trim();
    if (!line) {
      flushPara();
      flushList();
      continue;
    }
    const img = /^!\[([^\]]*)\]\((\S+?)(?:\s+"[^"]*")?\)$/.exec(line);
    if (img) {
      flushPara();
      flushList();
      const src = img[2] ?? '';
      if (IMG_HOSTS.some((h) => src.startsWith(h))) blocks.push({ kind: 'img', src, alt: img[1] ?? '' });
      continue;
    }
    const h = /^(#{1,6})\s+(.*)$/.exec(line);
    if (h) {
      flushPara();
      flushList();
      const level = Math.min(4, Math.max(2, (h[1]?.length ?? 1) + 1)) as 2 | 3 | 4;
      blocks.push({ kind: 'h', level, inl: inline(h[2] ?? '') });
      continue;
    }
    const li = /^[-*+•]\s+(.*)$/.exec(line);
    if (li) {
      flushPara();
      list ??= [];
      list.push(inline(li[1] ?? ''));
      continue;
    }
    flushList();
    para.push(line);
  }
  flushPara();
  flushList();
  return blocks;
}
