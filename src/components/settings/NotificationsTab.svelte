<script>
    import { desktopNotifications, notificationSound } from '../../lib/stores.js';
</script>

<h3 class="tab-title">Notifications</h3>

<label class="row">
    <div>
        <div class="lbl">Desktop Notifications</div>
        <div class="desc">Show notifications for new messages</div>
    </div>
    <label class="toggle"><input type="checkbox" bind:checked={$desktopNotifications}><span class="knob"></span></label>
</label>

<label class="row">
    <div>
        <div class="lbl">Notification Sound</div>
        <div class="desc">Play a sound for new messages</div>
    </div>
    <label class="toggle"><input type="checkbox" bind:checked={$notificationSound}><span class="knob"></span></label>
</label>

<style>
    .tab-title { font-size: 1.1em; font-weight: 600; margin: 0 0 20px; }

    .row {
        display: flex; align-items: center; justify-content: space-between;
        padding: 12px 0; border-bottom: 1px solid rgba(255,255,255,0.04);
    }
    .lbl { font-size: 0.88em; font-weight: 500; }
    .desc { font-size: 0.74em; color: #8b949e; margin-top: 2px; }

    .toggle {
        width: 40px; height: 22px; border-radius: 11px; background: var(--bg, #0e1117);
        border: 1px solid rgba(255,255,255,0.1); position: relative; cursor: pointer;
        transition: all 280ms ease; flex-shrink: 0; display: block;
    }
    .toggle input { position: absolute; width: 0; height: 0; opacity: 0; pointer-events: none; }
    .knob {
        position: absolute; width: 16px; height: 16px; border-radius: 50%;
        top: 2px; left: 2px; background: #8b949e; pointer-events: none;
        transition: all 280ms cubic-bezier(0.34, 1.56, 0.64, 1);
    }
    .toggle input:checked ~ .knob { transform: translateX(18px); background: var(--ac, #3fb9a8); }
    .toggle:has(input:checked) { border-color: var(--ac-border); }
</style>
