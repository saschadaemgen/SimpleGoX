<script>
    import { isLoggedIn } from './lib/stores.js';
    import { tryRestore } from './lib/tauri.js';
    import LoginScreen from './components/LoginScreen.svelte';
    import ChatLayout from './components/ChatLayout.svelte';
    import { onMount } from 'svelte';

    onMount(() => { tryRestore(); });
</script>

{#if $isLoggedIn}
    <ChatLayout />
{:else}
    <LoginScreen />
{/if}
