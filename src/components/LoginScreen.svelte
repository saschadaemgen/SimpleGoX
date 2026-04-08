<script>
    import { doLogin } from '../lib/tauri.js';
    import { loginError, loginLoading } from '../lib/stores.js';
    import { onMount } from 'svelte';

    let hs = 'https://matrix.simplego.dev';
    let username = '';
    let password = '';
    let visible = false;

    onMount(() => { setTimeout(() => visible = true, 100); });

    function submit() {
        if (username && password) doLogin(hs, username, password);
    }

    function onKey(e) { if (e.key === 'Enter') submit(); }
</script>

<div class="login" class:visible>
    <div class="card">
        <div class="logo" style="--d:0">
            <h1>Simple<span>GoX</span></h1>
            <p>Matrix Communication Terminal</p>
        </div>
        <div class="field" style="--d:1">
            <label for="hs">Homeserver</label>
            <input id="hs" bind:value={hs} on:keydown={onKey} placeholder="https://matrix.simplego.dev" />
        </div>
        <div class="field" style="--d:2">
            <label for="un">Username</label>
            <input id="un" bind:value={username} on:keydown={onKey} placeholder="username" autocomplete="username" />
        </div>
        <div class="field" style="--d:3">
            <label for="pw">Password</label>
            <input id="pw" type="password" bind:value={password} on:keydown={onKey} placeholder="password" autocomplete="current-password" />
        </div>
        {#if $loginError}
            <div class="err">{$loginError}</div>
        {/if}
        <button class="btn" style="--d:4" on:click={submit} disabled={$loginLoading || !username || !password}>
            {#if $loginLoading}Connecting...{:else}Login{/if}
        </button>
    </div>
</div>

<style>
    .login {
        display: flex; align-items: center; justify-content: center;
        height: 100vh; background: var(--bg); opacity: 0;
        transition: opacity 0.5s ease;
    }
    .login.visible { opacity: 1; }

    .card { width: 380px; padding: 48px 36px; }

    .logo { text-align: center; margin-bottom: 36px; }
    .logo h1 { font-size: 2.2em; font-weight: 700; letter-spacing: -0.5px; color: var(--text); }
    .logo h1 span { color: var(--ac); }
    .logo p { color: var(--text-3); font-size: 0.88em; margin-top: 6px; }

    .field, .btn {
        opacity: 0; transform: translateY(10px);
        animation: fadeIn 0.4s var(--ease) forwards;
        animation-delay: calc(var(--d) * 80ms + 200ms);
    }
    @keyframes fadeIn { to { opacity: 1; transform: translateY(0); } }

    .field { margin-bottom: 18px; }
    .field label {
        display: block; font-size: 0.72em; font-weight: 600; color: var(--text-3);
        text-transform: uppercase; letter-spacing: 1px; margin-bottom: 5px;
    }
    .field input {
        width: 100%; padding: 10px 14px; border-radius: 10px;
        border: 1px solid var(--border-2); background: var(--bg-input);
        color: var(--text); font-family: 'DM Sans', sans-serif; font-size: 0.88em;
        outline: none; transition: border-color 180ms, box-shadow 180ms;
    }
    .field input:focus {
        border-color: var(--ac-border);
        box-shadow: 0 0 0 2px var(--ac-glow);
    }
    .field input::placeholder { color: var(--text-3); }

    .btn {
        width: 100%; padding: 12px; border: none; border-radius: 10px;
        background: var(--ac); color: white; font-size: 0.95em; font-weight: 600;
        font-family: 'DM Sans', sans-serif; cursor: pointer;
        transition: transform 150ms var(--ease-b), box-shadow 250ms;
    }
    .btn:hover:not(:disabled) { transform: translateY(-1px); box-shadow: 0 0 20px var(--ac-glow); }
    .btn:active:not(:disabled) { transform: scale(0.98); }
    .btn:disabled { opacity: 0.5; cursor: not-allowed; }

    .err {
        color: var(--red); font-size: 0.82em; text-align: center;
        padding: 8px; background: rgba(248, 81, 73, 0.08);
        border-radius: 8px; border: 1px solid rgba(248, 81, 73, 0.15); margin-bottom: 12px;
    }
</style>
