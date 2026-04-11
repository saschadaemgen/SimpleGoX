<script>
    import { onMount } from 'svelte';

    let canvas;

    onMount(() => {
        const ctx = canvas.getContext('2d');
        const w = canvas.width = canvas.offsetWidth * 2;
        const h = canvas.height = canvas.offsetHeight * 2;

        // Aurora blobs
        const blobs = [
            {x:0.3, y:0.4, r:0.35, color:[88,166,255], speed:0.003, phase:0},
            {x:0.7, y:0.6, r:0.3, color:[104,158,213], speed:0.002, phase:2},
            {x:0.5, y:0.3, r:0.25, color:[63,185,168], speed:0.004, phase:4},
        ];

        // Floating particles
        const pts = [];
        for (let i = 0; i < 60; i++) {
            pts.push({
                x: Math.random() * w,
                y: Math.random() * h,
                vx: (Math.random() - 0.5) * 0.8,
                vy: (Math.random() - 0.5) * 0.8,
                r: Math.random() * 2 + 1
            });
        }

        let t = 0;
        let animId;

        function draw() {
            t += 0.01;
            ctx.clearRect(0, 0, w, h);

            // Layer 1: Aurora gradient blobs
            for (let b of blobs) {
                let cx = (b.x + Math.sin(t * b.speed * 100 + b.phase) * 0.15) * w;
                let cy = (b.y + Math.cos(t * b.speed * 80 + b.phase) * 0.1) * h;
                let r = b.r * w;
                let grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, r);
                grad.addColorStop(0, 'rgba(' + b.color.join(',') + ',0.12)');
                grad.addColorStop(0.5, 'rgba(' + b.color.join(',') + ',0.04)');
                grad.addColorStop(1, 'rgba(' + b.color.join(',') + ',0)');
                ctx.fillStyle = grad;
                ctx.fillRect(0, 0, w, h);
            }

            // Layer 2: Floating connected particles
            for (let i = 0; i < pts.length; i++) {
                let p = pts[i];
                p.x += p.vx; p.y += p.vy;
                if (p.x < 0) p.x = w;
                if (p.x > w) p.x = 0;
                if (p.y < 0) p.y = h;
                if (p.y > h) p.y = 0;

                ctx.beginPath();
                ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
                ctx.fillStyle = 'rgba(88,166,255,0.4)';
                ctx.fill();

                for (let j = i + 1; j < pts.length; j++) {
                    let q = pts[j];
                    let dx = p.x - q.x, dy = p.y - q.y;
                    let d = Math.sqrt(dx * dx + dy * dy);
                    if (d < 200) {
                        ctx.beginPath();
                        ctx.moveTo(p.x, p.y);
                        ctx.lineTo(q.x, q.y);
                        ctx.strokeStyle = 'rgba(88,166,255,' + (0.15 * (1 - d / 200)) + ')';
                        ctx.lineWidth = 0.5;
                        ctx.stroke();
                    }
                }
            }

            animId = requestAnimationFrame(draw);
        }

        draw();
        return () => cancelAnimationFrame(animId);
    });
</script>

<canvas class="abg" bind:this={canvas}></canvas>

<style>
    .abg {
        position: absolute; inset: 0; width: 100%; height: 100%;
        z-index: 0; pointer-events: none;
    }
</style>
