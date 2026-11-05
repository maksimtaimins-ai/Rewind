// Shared motion. Svelte transitions run through the Web Animations API, which
// the reduced-motion rule in app.css can't reach, so durations go through `ms`.
import { cubicIn, cubicOut } from 'svelte/easing';

export const reducedMotion = () => typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** `n` milliseconds, or 0 when the user asked for less motion. */
export const ms = (n: number) => (reducedMotion() ? 0 : n);

/** cubic-bezier(0.2, 0.8, 0.2, 1): the standard curve for things that move in place. */
export function standard(t: number): number {
	// Solve x(s) = t for the curve parameter, then return y(s).
	const x1 = 0.2, y1 = 0.8, x2 = 0.2, y2 = 1;
	const bx = (s: number) => 3 * x1 * s * (1 - s) ** 2 + 3 * x2 * s * s * (1 - s) + s ** 3;
	const by = (s: number) => 3 * y1 * s * (1 - s) ** 2 + 3 * y2 * s * s * (1 - s) + s ** 3;
	let lo = 0;
	let hi = 1;
	for (let i = 0; i < 20; i++) {
		const mid = (lo + hi) / 2;
		if (bx(mid) < t) lo = mid;
		else hi = mid;
	}
	return by((lo + hi) / 2);
}

/** Things entering ease out; things leaving ease in. */
export const enter = cubicOut;
export const exit = cubicIn;

/** A deleted tile shrinks toward its centre as it fades; the grid then glides into the gap (`animate:flip`). */
export function collapse(_node: Element, { duration = 260 }: { duration?: number } = {}) {
	return {
		duration: ms(duration),
		easing: cubicIn,
		css: (t: number) => `transform: scale(${0.7 + 0.3 * t}); opacity: ${t * t}; filter: blur(${(1 - t) * 2}px)`
	};
}
