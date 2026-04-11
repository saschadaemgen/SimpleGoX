<script>
    import { createEventDispatcher } from 'svelte';
    import { tgGetAuthState, tgSubmitPhone, tgSubmitCode, tgSubmitPassword, tgListChats, tgSubscribeUpdates, tgConnect } from '../../lib/tauri.js';
    import { telegramChats, telegramConnected } from '../../lib/stores.js';
    import { invoke } from '@tauri-apps/api/core';
    const dispatch = createEventDispatcher();

    export let telegramName = '';

    let sub = 'phone'; // phone, code, password, unregistered, done
    let phone = '';
    let code = '';
    let password = '';
    let error = null;
    let loading = false;
    let codeType = '';
    let sidecarReady = false;

    // Start sidecar on mount
    import { onMount } from 'svelte';
    onMount(async () => {
        console.log('=== TG Setup: starting sidecar...');
        try {
            await invoke('tg_start_sidecar', { port: 50051 });
            console.log('=== TG Setup: sidecar started, connecting...');
            await tgConnect(50051);
            sidecarReady = true;
            console.log('=== TG Setup: sidecar ready');
        } catch (e) {
            console.log('=== TG Setup: start failed, trying connect:', e);
            try {
                await tgConnect(50051);
                sidecarReady = true;
                console.log('=== TG Setup: connected to existing sidecar');
            } catch (e2) {
                console.warn('=== TG Setup: sidecar NOT ready:', e2);
            }
        }
    });

    async function submitPhone() {
        if (!phone.trim()) return;
        loading = true; error = null;
        try {
            // Ensure sidecar is connected before submitting
            if (!sidecarReady) {
                try {
                    await invoke('tg_start_sidecar', { port: 50051 });
                    await tgConnect(50051);
                    sidecarReady = true;
                } catch (_) {
                    try { await tgConnect(50051); sidecarReady = true; } catch (_2) {}
                }
            }
            const result = await tgSubmitPhone(phone.trim());
            codeType = result.code_type || '';
            if (['sms', 'sms_word', 'sms_phrase'].includes(codeType)) {
                sub = 'unregistered';
            } else {
                sub = 'code';
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
                if (auth.state === 'ready') { await onDone(); return; }
                if (auth.state === 'wait_password') { sub = 'password'; return; }
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
                if (auth.state === 'ready') { await onDone(); return; }
            }
        } catch (e) { error = String(e); }
        finally { loading = false; }
    }

    async function onDone() {
        sub = 'done';
        try {
            await tgConnect(50051);
            // Wait for TDLib to populate chat list after auth
            await new Promise(r => setTimeout(r, 1000));
            let chats = await tgListChats(50);
            // Retry once if empty (TDLib may still be loading)
            if (chats.length === 0) {
                await new Promise(r => setTimeout(r, 1500));
                chats = await tgListChats(50);
            }
            telegramChats.set(chats);
            telegramConnected.set(true);
            await tgSubscribeUpdates();
            telegramName = 'Connected';
            console.log(`TG wizard: loaded ${chats.length} chats`);
        } catch (e) { console.warn('TG post-login:', e); }
    }

    function skip() { dispatch('skip'); }
</script>

<div class="step">
    {#if sub === 'phone'}
        <h2 class="title">Connect Telegram</h2>
        <p class="sub">SimpleGoX connects to your existing Telegram account. Your session runs locally - credentials never leave your device.</p>
        <div class="notice">
            <span class="notice-icon">i</span>
            You need an existing Telegram account. New accounts must be created using the official Telegram app first.
        </div>

        <div class="field">
            <label>PHONE NUMBER</label>
            <input type="tel" bind:value={phone} placeholder="+49 123 456 789"
                on:keydown={e => e.key === 'Enter' && submitPhone()} autofocus />
        </div>

        {#if error}<div class="err">{error}</div>{/if}

        <div class="actions">
            <button class="btn sec" on:click={skip}>Skip for now</button>
            <button class="btn pri" on:click={submitPhone} disabled={loading || !phone.trim()}>
                {loading ? 'Sending...' : 'Send Code'}
            </button>
        </div>

    {:else if sub === 'code'}
        <h2 class="title">Verification Code</h2>
        <p class="sub">
            {#if codeType === 'telegram_message'}
                We sent a code to your <strong>Telegram app</strong>. Enter it below.
            {:else}
                Enter the verification code.
            {/if}
        </p>
        <p class="hint">The code is sent to your Telegram app, not via SMS.</p>

        <div class="field">
            <label>CODE</label>
            <input type="text" bind:value={code} placeholder="12345" maxlength="6"
                on:keydown={e => e.key === 'Enter' && submitCode()} autofocus />
        </div>

        {#if error}<div class="err">{error}</div>{/if}

        <div class="actions">
            <button class="btn sec" on:click={() => { sub = 'phone'; error = null; }}>Back</button>
            <button class="btn pri" on:click={submitCode} disabled={loading || !code.trim()}>
                {loading ? 'Verifying...' : 'Verify'}
            </button>
        </div>

    {:else if sub === 'password'}
        <h2 class="title">Two-Factor Authentication</h2>
        <p class="sub">Your account has 2FA enabled. Enter your cloud password.</p>

        <div class="field">
            <label>PASSWORD</label>
            <input type="password" bind:value={password} placeholder="Password"
                on:keydown={e => e.key === 'Enter' && submitPassword()} autofocus />
        </div>

        {#if error}<div class="err">{error}</div>{/if}

        <div class="actions">
            <button class="btn sec" on:click={() => { sub = 'code'; error = null; }}>Back</button>
            <button class="btn pri" on:click={submitPassword} disabled={loading || !password.trim()}>
                {loading ? 'Authenticating...' : 'Submit'}
            </button>
        </div>

    {:else if sub === 'unregistered'}
        <div class="warn-area">
            <div class="warn-icon">!</div>
            <h2 class="title">Number not registered</h2>
            <p class="sub center">This phone number is not registered on Telegram. Please create your account using the official Telegram app first.</p>
        </div>
        <div class="actions">
            <button class="btn sec" on:click={() => { sub = 'phone'; phone = ''; error = null; }}>Try different number</button>
            <button class="btn sec" on:click={skip}>Skip Telegram</button>
        </div>

    {:else if sub === 'done'}
        <div class="done">
            <div class="check-circle">
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
            </div>
            <h2 class="title">Telegram Connected!</h2>
            <p class="sub">Your Telegram chats will appear in the unified inbox alongside Matrix.</p>
            <div class="actions center">
                <button class="btn pri" on:click={() => dispatch('next')}>Continue</button>
            </div>
        </div>
    {/if}
</div>

<style>
    .step { max-width: 440px; margin: 0 auto; padding: 0 32px; }
    .title { font-size: 1.15em; font-weight: 600; margin: 0 0 8px; }
    .sub { font-size: 0.84em; color: #8b949e; margin: 0 0 20px; line-height: 1.5; }
    .sub strong { color: #c9d1d9; }
    .sub.center { text-align: center; }
    .hint { font-size: 0.78em; color: #8b949e; font-style: italic; margin: -12px 0 16px; }

    .notice {
        display: flex; align-items: flex-start; gap: 10px; padding: 12px 14px;
        border-radius: 10px; background: rgba(229,192,123,0.06);
        border: 1px solid rgba(229,192,123,0.12);
        font-size: 0.8em; color: #e5c07b; margin-bottom: 20px; line-height: 1.4;
    }
    .notice-icon {
        width: 18px; height: 18px; border-radius: 50%; background: rgba(229,192,123,0.15);
        display: flex; align-items: center; justify-content: center;
        font-weight: 700; font-size: 0.8em; flex-shrink: 0;
    }

    .field { margin-bottom: 16px; }
    .field label { display: block; font-size: 0.7em; font-weight: 600; color: #8b949e; text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 6px; }
    .field input {
        width: 100%; padding: 10px 14px; border-radius: 10px;
        border: 1px solid rgba(255,255,255,0.08); background: #0e1117;
        color: #c9d1d9; font-size: 0.9em; font-family: inherit; outline: none;
    }
    .field input:focus { border-color: #61afef; }

    .err {
        background: rgba(248,81,73,0.08); border: 1px solid rgba(248,81,73,0.15);
        border-radius: 10px; padding: 10px 14px; margin-bottom: 16px;
        font-size: 0.8em; color: #f85149;
    }

    .warn-area { text-align: center; margin-bottom: 20px; }
    .warn-icon {
        width: 48px; height: 48px; border-radius: 50%; background: rgba(224,108,117,0.15);
        color: #e06c75; display: flex; align-items: center; justify-content: center;
        font-size: 24px; font-weight: 700; margin: 0 auto 16px;
    }

    .done { text-align: center; }
    .check-circle {
        width: 56px; height: 56px; border-radius: 50%;
        background: rgba(152,195,121,0.15); color: #98c379;
        display: flex; align-items: center; justify-content: center;
        margin: 0 auto 16px;
        animation: pop 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
    }
    @keyframes pop { 0% { transform: scale(0); } 100% { transform: scale(1); } }

    .actions { display: flex; justify-content: space-between; }
    .actions.center { justify-content: center; }
    .btn {
        padding: 11px 28px; border-radius: 10px; border: none;
        font-size: 0.88em; font-weight: 600; font-family: inherit; cursor: pointer; transition: all 0.15s;
    }
    .btn.pri { background: #61afef; color: #0e1117; }
    .btn.pri:hover:not(:disabled) { background: #7bc0f5; }
    .btn.pri:disabled { opacity: 0.4; cursor: default; }
    .btn.sec { background: rgba(255,255,255,0.06); color: #c9d1d9; }
    .btn.sec:hover { background: rgba(255,255,255,0.1); }
</style>
