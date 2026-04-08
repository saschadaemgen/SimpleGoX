export function formatTime(ts) {
    if (!ts) return '';
    const d = new Date(ts);
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

export function formatDate(ts) {
    const d = new Date(ts), now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const msgDay = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    const diff = (today - msgDay) / 86400000;
    if (diff === 0) return 'Today';
    if (diff === 1) return 'Yesterday';
    return d.toLocaleDateString('en-US', { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' });
}

const SENDER_COLORS = [
    '#e06c75', '#61afef', '#c678dd', '#e5c07b',
    '#56b6c2', '#be5046', '#98c379', '#d19a66',
    '#e88c6a', '#7ec8e3', '#c3a6d8', '#a8d5a2',
];

export function senderColor(userId) {
    let hash = 0;
    for (let i = 0; i < userId.length; i++) hash = userId.charCodeAt(i) + ((hash << 5) - hash);
    return SENDER_COLORS[Math.abs(hash) % SENDER_COLORS.length];
}

export function displayName(userId) {
    if (!userId) return 'Unknown';
    const m = userId.match(/^@([^:]+)/);
    return m ? m[1] : userId;
}

export function groupMessages(msgs) {
    const groups = [];
    let cur = null;
    for (const m of msgs) {
        const canGroup = cur
            && cur.sender === m.sender
            && !m.reply_to_event_id
            && !m.is_redacted
            && m.timestamp - cur.messages[cur.messages.length - 1].timestamp < 300000;
        if (canGroup) {
            cur.messages.push(m);
        } else {
            cur = { sender: m.sender, messages: [m] };
            groups.push(cur);
        }
    }
    return groups;
}

export function needsDateSep(msg, prev) {
    if (!prev) return true;
    return new Date(msg.timestamp).toDateString() !== new Date(prev.timestamp).toDateString();
}
