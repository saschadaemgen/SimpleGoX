<script>
    import { formatDate } from '../lib/utils.js';
    export let timestamp;
</script>

<div class="sep">{formatDate(timestamp)}</div>

<style>
    .sep {
        display: flex; align-items: center; gap: 14px; padding: 8px 0 3px;
        color: var(--text-3); font-size: 0.68em; font-weight: 600;
        letter-spacing: 1px; text-transform: uppercase;
    }
    .sep::before, .sep::after { content: ''; flex: 1; height: 1px; background: var(--border-2); }
</style>
