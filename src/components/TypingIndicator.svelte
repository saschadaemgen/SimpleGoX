<script>
    import { displayName } from '../lib/utils.js';

    export let users = [];

    $: text = users.length === 1
        ? `${displayName(users[0])} is typing`
        : users.length > 1
            ? `${users.length} typing`
            : '';
</script>

{#if users.length > 0}
    <div class="typ">
        <div class="dots"><i></i><i></i><i></i></div>
        <span>{text}</span>
    </div>
{/if}

<style>
    .typ {
        display: flex; align-items: center; gap: 6px; padding: 3px 12px;
        color: var(--text-3); font-size: 0.78em;
        animation: typIn 180ms var(--ease);
    }
    @keyframes typIn { from { opacity: 0; transform: translateY(2px); } to { opacity: 1; } }

    .dots { display: flex; gap: 3px; }
    i {
        width: 4px; height: 4px; border-radius: 50%; background: var(--text-3);
        font-style: normal; animation: dj 1.4s var(--ease-b) infinite;
    }
    i:nth-child(2) { animation-delay: 0.15s; }
    i:nth-child(3) { animation-delay: 0.3s; }
    @keyframes dj { 0%,60%,100% { transform: translateY(0); opacity: 0.3; } 30% { transform: translateY(-4px); opacity: 0.7; } }
</style>
