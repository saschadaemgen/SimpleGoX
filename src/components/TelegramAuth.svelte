<script>
    import { telegramAuthOpen, telegramChats, telegramConnected } from '../lib/stores.js';
    import { tgGetAuthState, tgSubmitPhone, tgSubmitCode, tgSubmitPassword, tgListChats, tgSubscribeUpdates, tgLogout } from '../lib/tauri.js';
    import { invoke } from '@tauri-apps/api/core';

    let phone = '';
    let code = '';
    let password = '';
    let error = null;
    let loading = false;
    let step = 'phone'; // phone, code, password, unregistered, done
    let codeType = '';

    // When dialog opens: reset local fields, then check sidecar state
    $: if ($telegramAuthOpen) { onOpen(); }

    async function onOpen() {
        // Always start fresh locally
        phone = ''; code = ''; password = '';
        error = null; loading = false; codeType = '';

        try {
            const auth = await tgGetAuthState();
            console.log('=== AUTH onOpen state:', JSON.stringify(auth));

            if (auth.state === 'ready') {
                // Already logged in - just load chats and close
                await onSuccess();
                return;
            }

            if (auth.state === 'wait_code' || auth.state === 'wait_password') {
                // Sidecar stuck in mid-auth - reset it to get a clean phone screen
                console.log('=== AUTH: sidecar stuck in', auth.state, '- resetting...');
                await restartSidecar();
            }

            step = 'phone';
        } catch (_) {
            // Sidecar not connected
            step = 'phone';
        }
    }

    async function submitPhone() {
        if (!phone.trim()) return;
        loading = true; error = null;
        try {
            const result = await tgSubmitPhone(phone.trim());
            console.log('=== AUTH submitPhone result:', JSON.stringify(result));
            codeType = result.code_type || '';

            if (['sms', 'sms_word', 'sms_phrase'].includes(codeType)) {
                step = 'unregistered';
            } else {
                step = 'code';
            }
        } catch (e) { error = String(e); }
        finally { loading = false; }
    }

    async function submitCode() {
        if (!code.trim()) return;
        loading = true; error = null;
        try {
            await tgSubmitCode(code.trim());
            for (let i = 0; i < 15; i++) {
                await new Promise(r => setTimeout(r, 400));
                const auth = await tgGetAuthState();
                if (auth.state === 'ready') { await onSuccess(); return; }
                if (auth.state === 'wait_password') { step = 'password'; return; }
            }
        } catch (e) { error = String(e); }
        finally { loading = false; }
    }

    async function submitPassword() {
        if (!password.trim()) return;
        loading = true; error = null;
        try {
            await tgSubmitPassword(password.trim());
            for (let i = 0; i < 15; i++) {
                await new Promise(r => setTimeout(r, 400));
                const auth = await tgGetAuthState();
                if (auth.state === 'ready') { await onSuccess(); return; }
            }
        } catch (e) { error = String(e); }
        finally { loading = false; }
    }

    async function onSuccess() {
        console.log('=== AUTH onSuccess: login complete');
        step = 'done';
        try {
            // Ensure gRPC client is connected to the current sidecar
            console.log('=== AUTH onSuccess: reconnecting gRPC client...');
            await invoke('tg_connect', { port: 50051 });
            console.log('=== AUTH onSuccess: loading chats...');
            const chats = await tgListChats(50);
            console.log('=== AUTH onSuccess: loaded', chats.length, 'chats');
            telegramChats.set(chats);
            telegramConnected.set(true);
            console.log('=== AUTH onSuccess: subscribing to updates...');
            await tgSubscribeUpdates();
            console.log('=== AUTH onSuccess: all done');
        } catch (e) { console.error('=== AUTH onSuccess FAILED:', e); }
        setTimeout(() => telegramAuthOpen.set(false), 800);
    }

    async function goBack() {
        // TDLib has no "go back" in auth flow - must restart sidecar
        error = null;
        loading = true;
        await restartSidecar();
        loading = false;
        phone = ''; code = ''; password = ''; codeType = '';
        step = 'phone';
    }

    async function restartSidecar() {
        try { await tgLogout(); } catch (_) {}
        try {
            await invoke('tg_start_sidecar', { port: 50051 });
            await new Promise(r => setTimeout(r, 2000));
            await invoke('tg_connect', { port: 50051 });
        } catch (_) {}
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
            <div class="actions">
                <button class="btn secondary" on:click={close}>Cancel</button>
                <button class="btn primary" on:click={submitPhone} disabled={loading || !phone.trim()}>
                    {loading ? 'Sending...' : 'Send Code'}
                </button>
            </div>

        {:else if step === 'code'}
            <p class="hint">
                {#if codeType === 'telegram_message'}
                    Check your <strong>Telegram app</strong> for the login code.
                {:else}
                    Enter the verification code.
                {/if}
            </p>
            <div class="field">
                <label>Verification Code</label>
                <input type="text" bind:value={code} placeholder="12345" maxlength="6"
                    on:keydown={e => e.key === 'Enter' && submitCode()} autofocus />
            </div>
            <div class="actions">
                <button class="btn secondary" on:click={goBack} disabled={loading}>
                    {loading ? 'Resetting...' : 'Back'}
                </button>
                <button class="btn primary" on:click={submitCode} disabled={loading || !code.trim()}>
                    {loading ? 'Verifying...' : 'Verify Code'}
                </button>
            </div>

        {:else if step === 'password'}
            <p class="hint">Enter your two-factor authentication password.</p>
            <div class="field">
                <label>2FA Password</label>
                <input type="password" bind:value={password} placeholder="Password"
                    on:keydown={e => e.key === 'Enter' && submitPassword()} autofocus />
            </div>
            <div class="actions">
                <button class="btn secondary" on:click={goBack} disabled={loading}>
                    {loading ? 'Resetting...' : 'Back'}
                </button>
                <button class="btn primary" on:click={submitPassword} disabled={loading || !password.trim()}>
                    {loading ? 'Authenticating...' : 'Login'}
                </button>
            </div>

        {:else if step === 'unregistered'}
            <div class="warn-icon">!</div>
            <p class="hint center"><strong>Phone number not registered</strong></p>
            <p class="hint center">
                This number does not have a Telegram account.
                SimpleGoX can only connect to existing accounts.
                Please create your account first using the official Telegram app.
            </p>
            <div class="actions">
                <button class="btn secondary" on:click={goBack} disabled={loading}>
                    {loading ? 'Resetting...' : 'Try Different Number'}
                </button>
                <button class="btn primary" on:click={close}>Close</button>
            </div>

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
        padding: 24px; width: 380px; max-width: 90vw;
        border: 1px solid rgba(255,255,255,0.06);
        box-shadow: 0 20px 40px rgba(0,0,0,0.4);
        animation: slideUp 200ms ease;
    }
    @keyframes slideUp { from { opacity: 0; transform: translateY(12px); } }

    .hdr { display: flex; align-items: center; gap: 10px; margin-bottom: 20px; }
    .hdr h3 { flex: 1; margin: 0; font-size: 1em; font-weight: 600; }
    .badge { padding: 3px 8px; border-radius: 6px; font-size: 0.65em; font-weight: 700; letter-spacing: 0.5px; background: rgba(97,175,239,0.15); color: #61afef; }
    .x { width: 28px; height: 28px; border-radius: 8px; border: none; background: transparent; color: #8b949e; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 0.15s; }
    .x:hover { background: rgba(255,255,255,0.06); color: #e6edf3; }

    .hint { font-size: 0.82em; color: #8b949e; margin: 0 0 16px; line-height: 1.5; }
    .hint.center { text-align: center; }
    .hint strong { color: #c9d1d9; }

    .field { margin-bottom: 16px; }
    .field label { display: block; font-size: 0.72em; font-weight: 600; color: #8b949e; margin-bottom: 6px; text-transform: uppercase; letter-spacing: 0.5px; }
    .field input { width: 100%; padding: 10px 14px; border-radius: 10px; border: 1px solid rgba(255,255,255,0.08); background: #0e1117; color: #c9d1d9; font-size: 0.9em; font-family: inherit; outline: none; transition: border-color 150ms; }
    .field input:focus { border-color: #61afef; }

    .actions { display: flex; gap: 8px; }
    .btn { flex: 1; padding: 11px; border-radius: 10px; border: none; font-size: 0.85em; font-weight: 600; cursor: pointer; font-family: inherit; transition: all 120ms ease; }
    .btn.primary { background: #61afef; color: #0e1117; }
    .btn.primary:hover:not(:disabled) { background: #7bc0f5; }
    .btn.secondary { background: rgba(255,255,255,0.06); color: #c9d1d9; }
    .btn.secondary:hover:not(:disabled) { background: rgba(255,255,255,0.1); }
    .btn:disabled { opacity: 0.4; cursor: default; }

    .err { background: rgba(248,81,73,0.08); border: 1px solid rgba(248,81,73,0.15); border-radius: 10px; padding: 10px 14px; margin-bottom: 16px; font-size: 0.8em; color: #f85149; }
    .warn-icon { width: 48px; height: 48px; border-radius: 50%; background: rgba(224,108,117,0.15); color: #e06c75; display: flex; align-items: center; justify-content: center; font-size: 24px; font-weight: 700; margin: 0 auto 16px; }
    .success { display: flex; align-items: center; justify-content: center; gap: 10px; padding: 20px; border-radius: 12px; background: rgba(97,175,239,0.08); color: #61afef; font-size: 0.95em; font-weight: 600; }
</style>
