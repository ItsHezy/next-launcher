<script lang="ts">
  import { commands } from '../bindings';
  import Icon from './Icon.svelte';
</script>

<div class="wc">
  <button class="icon-btn" aria-label="Minimize" onclick={() => commands.windowAction('minimize')}>
    <Icon name="minimize" size={18} />
  </button>
  <button class="icon-btn" aria-label="Maximize or restore" onclick={() => commands.windowAction('toggleMaximize')}>
    <Icon name="maximize" size={16} />
  </button>
  <button class="icon-btn close" aria-label="Close" onclick={() => commands.windowAction('close')}>
    <Icon name="close" size={18} />
  </button>
</div>

<style>
  .wc {
    display: flex;
    gap: 2px;
  }
  .close:hover {
    background: #c42b1c;
    color: #fff;
  }
</style>
