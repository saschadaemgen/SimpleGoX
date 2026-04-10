<script>
    import { telegramAuthOpen, telegramAuthState, telegramChats, telegramConnected } from '../lib/stores.js';
    import { tgGetAuthState, tgSubmitPhone, tgSubmitCode, tgSubmitPassword, tgListChats, tgSubscribeUpdates } from '../lib/tauri.js';

    let phone = '';
    let code = '';
    let password = '';
    let error = null;
    let loading = false;
    let step = 'phone'; // phone, code, password, done

    // Detect current auth state on open
    $: if ($telegramAuthOpen) { detectState(); }

    async function detectState() {
        try {
            const state = await tgGetAuthState();
            if (state === 'wait_code') step = 'code';
            else if (state === 'wait_password') step = 'password';
            else if (state === 'ready') { await onSuccess(); return; }
            else step = 'phone';
        } catch (_) {
            step = 'phone';
        }
    }

    async function submitPhone() {
        if (!phone.trim()) return;
        loading = true;
        error = null;
        try {
            await tgSubmitPhone(phone.trim());
            await pollForState('wait_code');
            step = 'code';
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function submitCode() {
        if (!code.trim()) return;
        loading = true;
        error = null;
        try {
            await tgSubmitCode(code.trim());
            const state = await pollForNextState();
            if (state === 'ready') {
                await onSuccess();
            } else if (state === 'wait_password') {
                step = 'password';
            }
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function submitPassword() {
        if (!password.trim()) return;
        loading = true;
        error = null;
        try {
            await tgSubmitPassword(password.trim());
            await pollForState('ready');
            await onSuccess();
        } catch (e) {
            error = String(e);
        } finally {
            loading = false;
        }
    }

    async function pollForState(target) {
        for (let i = 0; i < 15; i++) {
            await new Promise(r => setTimeout(r, 400));
            const s = await tgGetAuthState();
            if (s === target) return;
        }
    }

    async function pollForNextState() {
        for (let i = 0; i < 15; i++) {
            await new Promise(r => setTimeout(r, 400));
            const s = await tgGetAuthState();
            if (s !== 'wait_code') return s;
        }
        return 'wait_code';
    }

    async function onSuccess() {
        step = 'done';
        try {
            const chats = await tgListChats(50);
            telegramChats.set(chats);
            telegramConnected.set(true);
            await tgSubscribeUpdates();
        } catch (e) {
            console.warn('Post-login load failed:', e);
        }
        // Auto-close after brief success message
        setTimeout(() => telegramAuthOpen.set(false), 800);
    }

    function close() { telegramAuthOpen.set(false); }
    function handleKey(e) { if (e.key === 'Escape') close(); }
</script>

<svelte:window on:keydown={handleKey} />

{#if $telegramAuthOpen}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" on:click={close}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="dialog" on:click|stopPropagation>
        <div class="hdr">
            <span class="badge">TG</span>
            <h3>Add Telegram Account</h3>
            <button class="x" on:click={close}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
            </button>
        </div>

        {#if error}
            <div class="err">{error}</div>
        {/if}

        {#if step === 'phone'}
            <p class="hint">Enter your phone number with country code.</p>
            <div class="field">
                <label>Phone Number</label>
                <input type="tel" bind:value={phone} placeholder="+49 123 456 789"
                    on:keydown={e => e.key === 'Enter' && submitPhone()} autofocus />
            </div>
            <button class="btn" on:click={submitPhone} disabled={loading || !phone.trim()}>
                {loading ? 'Sending...' : 'Send Code'}
            </button>

        {:else if step === 'code'}
            <p class="hint">Enter the code sent to your Telegram app.</p>
            <div class="field">
                <label>Verification Code</label>
                <input type="text" bind:value={code} placeholder="12345" maxlength="6"
                    on:keydown={e => e.key === 'Enter' && submitCode()} autofocus />
            </div>
            <button class="btn" on:click={submitCode} disabled={loading || !code.trim()}>
                {loading ? 'Verifying...' : 'Verify Code'}
            </button>

        {:else if step === 'password'}
            <p class="hint">Enter your two-factor authentication password.</p>
            <div class="field">
                <label>2FA Password</label>
                <input type="password" bind:value={password} placeholder="Password"
                    on:keydown={e => e.key === 'Enter' && submitPassword()} autofocus />
            </div>
            <button class="btn" on:click={submitPassword} disabled={loading || !password.trim()}>
                {loading ? 'Authenticating...' : 'Login'}
            </button>

        {:else if step === 'done'}
            <div class="success">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
                <span>Telegram connected!</span>
            </div>
        {/if}
    </div>
</div>
{/if}

<style>
    .overlay {
        position: fixed; inset: 0; background: rgba(0,0,0,0.6);
        backdrop-filter: blur(6px);
        display: flex; align-items: center; justify-content: center;
        z-index: 400; animation: fadeIn 200ms ease;
    }
    @keyframes fadeIn { from { opacity: 0; } }

    .dialog {
        background: #161b22; border-radius: 16px;
        padding: 24px; width: 360px; max-width: 90vw;
        border: 1px solid rgba(255,255,255,0.06);
        box-shadow: 0 20px 40px rgba(0,0,0,0.4);
        animation: slideUp 200ms ease;
    }
    @keyframes slideUp { from { opacity: 0; transform: translateY(12px); } }

    .hdr { display: flex; align-items: center; gap: 10px; margin-bottom: 20px; }
    .hdr h3 { flex: 1; margin: 0; font-size: 1em; font-weight: 600; }

    .badge {
        padding: 3px 8px; border-radius: 6px;
        font-size: 0.65em; font-weight: 700; letter-spacing: 0.5px;
        background: rgba(97,175,239,0.15); color: #61afef;
    }

    .x {
        width: 28px; height: 28px; border-radius: 8px; border: none;
        background: transparent; color: #8b949e; cursor: pointer;
        display: flex; align-items: center; justify-content: center;
        transition: all 0.15s;
    }
    .x:hover { background: rgba(255,255,255,0.06); color: #e6edf3; }

    .hint { font-size: 0.82em; color: #8b949e; margin: 0 0 16px; line-height: 1.4; }

    .field { margin-bottom: 16px; }
    .field label {
        display: block; font-size: 0.72em; font-weight: 600;
        color: #8b949e; margin-bottom: 6px; text-transform: uppercase; letter-spacing: 0.5px;
    }
    .field input {
        width: 100%; padding: 10px 14px; border-radius: 10px;
        border: 1px solid rgba(255,255,255,0.08);
        background: #0e1117; color: #c9d1d9;
        font-size: 0.9em; font-family: inherit;
        outline: none; transition: border-color 150ms;
    }
    .field input:focus { border-color: #61afef; }

    .btn {
        width: 100%; padding: 11px; border-radius: 10px;
        border: none; font-size: 0.88em; font-weight: 600;
        cursor: pointer; font-family: inherit;
        background: #61afef; color: #0e1117;
        transition: all 120ms ease;
    }
    .btn:hover:not(:disabled) { background: #7bc0f5; }
    .btn:disabled { opacity: 0.4; cursor: default; }

    .err {
        background: rgba(248,81,73,0.08); border: 1px solid rgba(248,81,73,0.15);
        border-radius: 10px; padding: 10px 14px; margin-bottom: 16px;
        font-size: 0.8em; color: #f85149;
    }

    .success {
        display: flex; align-items: center; justify-content: center; gap: 10px;
        padding: 20px; border-radius: 12px;
        background: rgba(97,175,239,0.08); color: #61afef;
        font-size: 0.95em; font-weight: 600;
    }
</style>
