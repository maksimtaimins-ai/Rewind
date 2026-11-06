// Fires once, the first time the element comes within `margin` px of the viewport (or its scroll container).
// Used to load thumbnails on demand instead of decoding every one when the library opens.
export function inview(node: HTMLElement, opts: { onenter: () => void; margin?: number }) {
	if (typeof IntersectionObserver === 'undefined') {
		opts.onenter();
		return {};
	}
	const io = new IntersectionObserver(
		(entries) => {
			if (entries.some((e) => e.isIntersecting)) {
				io.disconnect();
				opts.onenter();
			}
		},
		{ rootMargin: `${opts.margin ?? 300}px` }
	);
	io.observe(node);
	return { destroy: () => io.disconnect() };
}
