<script lang="ts">
  // iOS-style switch: 52×32 track with a 44px hit area; green when on (brief section 6).
  let {
    checked,
    onchange,
    label,
    labelledby,
    disabled = false,
  }: {
    checked: boolean;
    onchange: (next: boolean) => void;
    label?: string;
    labelledby?: string;
    disabled?: boolean;
  } = $props();
</script>

<button
  class="switch"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  aria-labelledby={labelledby}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    flex: none;
    width: 52px;
    height: 32px;
    min-height: 32px;
    margin: 6px 0;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.16);
    transition: background-color var(--dur) var(--ease-out);
  }
  .switch::before {
    content: '';
    position: absolute;
    inset: -6px -4px;
  }
  .switch[aria-checked='true'] {
    background: var(--success-fill);
  }
  .switch:disabled {
    opacity: 0.45;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
    transition: transform var(--dur) var(--ease-spring);
  }
  .switch[aria-checked='true'] .knob {
    transform: translateX(20px);
  }
</style>
