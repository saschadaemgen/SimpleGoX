<script>
    import { telegramAuthOpen, telegramAuthState, connectedBackends, telegramChats, telegramConnected } from '../lib/stores.js';
    import { tgStartSidecar, tgConnect, tgGetAuthState, tgSubmitPhone, tgSubmitCode, tgSubmitPassword, tgListChats, getBackends } from '../lib/tauri.js';

    let phone = '';
    let code = '';
    let password = '';
    let error = null;
    let loading = false;
    let port = 50051;
    let mode = 'choose'; // choose, start, connect

    async function onConnected(state) {
        telegramAuthState.set(state);
        if (state === 'ready') {
            // Already authenticated - load chats and close dialog
            try {
                const chats = await tgListChats(50);
                telegramChats.set(chats);
                telegramConnected.set(true);
            } catch (e) { console.warn('Failed to load TG chats:', e); }
            telegramAuthOpen.set(false);
        }
    }

    async function startSidecar() {
        loading = true;
        error = null;
        try {
            await tgStartSidecar(port);
            const state = await tgGetAuthState();
            await onConnected(state);
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function connect() {
        loading = true;
        error = null;
        try {
            await tgConnect(port);
            const state = await tgGetAuthState();
            await onConnected(state);
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function submitPhone() {
        loading = true;
        error = null;
        try {
            await tgSubmitPhone(phone);
            // Poll for new state
            await pollState();
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function submitCode() {
        loading = true;
        error = null;
        try {
            await tgSubmitCode(code);
            await pollState();
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function submitPassword() {
        loading = true;
        error = null;
        try {
            await tgSubmitPassword(password);
            await pollState();
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function pollState() {
        for (let i = 0; i < 10; i++) {
            await new Promise(r => setTimeout(r, 500));
            const state = await tgGetAuthState();
            telegramAuthState.set(state);
            if (state !== $telegramAuthState) break;
        }
        if ($telegramAuthState === 'ready') {
            // Load Telegram chats into sidebar
            try {
                const chats = await tgListChats(50);
                telegramChats.set(chats);
                telegramConnected.set(true);
                console.log(`Loaded ${chats.length} Telegram chats after auth`);
            } catch (e) {
                console.warn('Failed to load TG chats:', e);
            }
            const backends = await getBackends();
            connectedBackends.set(backends);
            telegramAuthOpen.set(false);
        }
    }

    function close() {
        telegramAuthOpen.set(false);
    }

    function handleKey(e) {
        if (e.key === 'Escape') close();
    }
</script>

<svelte:window on:keydown={handleKey} />

{#if $telegramAuthOpen}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" on:click={close}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="dialog" on:click|stopPropagation>
        <div class="hdr">
            <span class="badge" style="background:rgba(97,175,239,0.15);color:#61afef">TG</span>
            <h3>Connect Telegram</h3>
            <button class="close" on:click={close}>&times;</button>
        </div>

        {#if error}
            <div class="err">{error}</div>
        {/if}

        {#if $telegramAuthState === 'disconnected'}
            <p class="hint">Start the Telegram sidecar or connect to a running one.</p>
            <div class="field">
                <label>Sidecar Port</label>
                <input type="number" bind:value={port} placeholder="50051" />
            </div>
            <div class="btn-row">
                <button class="btn primary" on:click={startSidecar} disabled={loading}>
                    {loading ? 'Starting...' : 'Start Sidecar'}
                </button>
                <button class="btn secondary" on:click={connect} disabled={loading}>
                    Connect
                </button>
            </div>

        {:else if $telegramAuthState === 'wait_phone'}
            <p class="hint">Enter your phone number with country code.</p>
            <div class="field">
                <label>Phone Number</label>
                <input type="tel" bind:value={phone} placeholder="+49123456789" on:keydown={e => e.key === 'Enter' && submitPhone()} />
            </div>
            <button class="btn primary" on:click={submitPhone} disabled={loading || !phone}>
                {loading ? 'Sending...' : 'Send Code'}
            </button>

        {:else if $telegramAuthState === 'wait_code'}
            <p class="hint">Enter the code sent to your Telegram app.</p>
            <div class="field">
                <label>Verification Code</label>
                <input type="text" bind:value={code} placeholder="12345" maxlength="6"
                    on:keydown={e => e.key === 'Enter' && submitCode()} />
            </div>
            <button class="btn primary" on:click={submitCode} disabled={loading || !code}>
                {loading ? 'Verifying...' : 'Verify'}
            </button>

        {:else if $telegramAuthState === 'wait_password'}
            <p class="hint">Enter your two-factor authentication password.</p>
            <div class="field">
                <label>2FA Password</label>
                <input type="password" bind:value={password} placeholder="Password"
                    on:keydown={e => e.key === 'Enter' && submitPassword()} />
            </div>
            <button class="btn primary" on:click={submitPassword} disabled={loading || !password}>
                {loading ? 'Authenticating...' : 'Login'}
            </button>

        {:else if $telegramAuthState === 'ready'}
            <div class="success">
                <span class="check">&#10003;</span>
                <span>Telegram connected successfully!</span>
            </div>
            <button class="btn primary" on:click={close}>Done</button>
        {/if}
    </div>
</div>
{/if}

<style>
    .overlay {
        position: fixed; inset: 0; background: rgba(0,0,0,0.6);
        display: flex; align-items: center; justify-content: center;
        z-index: 200; animation: fadeIn 150ms ease;
    }
    @keyframes fadeIn { from { opacity: 0; } }

    .dialog {
        background: var(--bg-card, #161b22); border-radius: 14px;
        padding: 24px; width: 380px; max-width: 90vw;
        border: 1px solid var(--border, rgba(240,246,252,0.06));
        animation: slideUp 200ms var(--ease, ease);
    }
    @keyframes slideUp { from { opacity: 0; transform: translateY(10px); } }

    .hdr {
        display: flex; align-items: center; gap: 10px; margin-bottom: 18px;
    }
    .hdr h3 { flex: 1; margin: 0; font-size: 1.05em; font-weight: 600; }

    .badge {
        padding: 2px 7px; border-radius: 5px;
        font-size: 0.65em; font-weight: 700; letter-spacing: 0.5px;
    }

    .close {
        background: none; border: none; color: var(--text-3, #8b949e);
        font-size: 1.4em; cursor: pointer; padding: 0 4px; line-height: 1;
    }
    .close:hover { color: var(--text, #c9d1d9); }

    .hint {
        font-size: 0.82em; color: var(--text-3, #8b949e); margin: 0 0 14px;
    }

    .field { margin-bottom: 14px; }
    .field label {
        display: block; font-size: 0.75em; font-weight: 500;
        color: var(--text-2, #b1bac4); margin-bottom: 5px;
    }
    .field input {
        width: 100%; padding: 9px 12px; border-radius: 8px;
        border: 1px solid var(--border, rgba(240,246,252,0.06));
        background: var(--bg, #0e1117); color: var(--text, #c9d1d9);
        font-size: 0.88em; font-family: inherit;
        outline: none; transition: border-color 150ms;
    }
    .field input:focus {
        border-color: #61afef;
    }

    .btn-row { display: flex; gap: 8px; }
    .btn-row .btn { flex: 1; }

    .btn {
        width: 100%; padding: 10px; border-radius: 9px;
        border: none; font-size: 0.85em; font-weight: 600;
        cursor: pointer; font-family: inherit;
        transition: all 120ms ease;
    }
    .btn.primary {
        background: #61afef; color: #0e1117;
    }
    .btn.primary:hover:not(:disabled) {
        background: #7bc0f5;
    }
    .btn.secondary {
        background: rgba(97,175,239,0.12); color: #61afef;
    }
    .btn.secondary:hover:not(:disabled) {
        background: rgba(97,175,239,0.2);
    }
    .btn:disabled {
        opacity: 0.5; cursor: default;
    }

    .err {
        background: rgba(248,81,73,0.1); border: 1px solid rgba(248,81,73,0.2);
        border-radius: 8px; padding: 8px 12px; margin-bottom: 14px;
        font-size: 0.78em; color: #f85149;
    }

    .success {
        display: flex; align-items: center; gap: 10px;
        padding: 14px; margin-bottom: 14px; border-radius: 9px;
        background: rgba(97,175,239,0.1); color: #61afef;
        font-size: 0.88em; font-weight: 500;
    }
    .check {
        font-size: 1.2em; font-weight: 700;
    }
</style>
