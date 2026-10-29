<script lang="ts">
  import { parse, type Inline } from '../markdown';

  let { source }: { source: string } = $props();
  const blocks = $derived(parse(source));
</script>

{#snippet runs(inl: Inline[])}
  {#each inl as r, i (i)}{#if r.bold}<strong>{r.text}</strong>{:else if r.italic}<em>{r.text}</em>{:else}{r.text}{/if}{/each}
{/snippet}

<div class="md">
  {#each blocks as b, i (i)}
    {#if b.kind === 'h'}
      {#if b.level === 2}<h2>{@render runs(b.inl)}</h2>{:else if b.level === 3}<h3>{@render runs(b.inl)}</h3>{:else}<h4>
          {@render runs(b.inl)}
        </h4>{/if}
    {:else if b.kind === 'p'}
      <p>{@render runs(b.inl)}</p>
    {:else if b.kind === 'ul'}
      <ul>
        {#each b.items as item, j (j)}<li>{@render runs(item)}</li>{/each}
      </ul>
    {:else if b.kind === 'img'}
      <img src={b.src} alt={b.alt} loading="lazy" decoding="async" />
    {/if}
  {/each}
</div>

<style>
  .md {
    color: var(--text-2);
    line-height: 1.55;
    max-width: 760px;
  }
  .md :global(strong) {
    color: var(--text);
  }
  h2,
  h3,
  h4 {
    color: var(--text);
    margin: 22px 0 8px;
    line-height: 1.25;
  }
  h2 {
    font-size: 19px;
  }
  h3 {
    font-size: 17px;
  }
  h4 {
    font-size: 15px;
  }
  p {
    margin: 0 0 12px;
  }
  ul {
    margin: 0 0 12px;
    padding-left: 20px;
  }
  li {
    margin: 4px 0;
  }
  img {
    display: block;
    max-width: 100%;
    height: auto;
    margin: 12px 0;
    border-radius: var(--radius-md);
  }
</style>
