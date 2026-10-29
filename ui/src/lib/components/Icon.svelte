<script module lang="ts">
	/** 24×24 outline icons, drawn with the current text color. */
	const PATHS = {
		overview: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
		fleet: 'M3 5h8v6H3zM13 5h8v6h-8zM3 15h8v4H3zM13 15h8v4h-8zM7 8h.01M17 8h.01',
		pod: 'M12 3l8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9',
		deployment: 'M12 3l9 5-9 5-9-5zM3 13l9 5 9-5M3 17.5l9 5 9-5',
		statefulset: 'M4 5h16v5H4zM4 14h16v5H4zM8 7.5h.01M8 16.5h.01',
		daemonset: 'M8 8h12v12H8zM4 16V4h12',
		job: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2',
		cronjob: 'M4 6h16v14H4zM4 10h16M8 3v4M16 3v4M12 13v3h2',
		node: 'M3 4h18v7H3zM3 13h18v7H3zM7 7.5h.01M7 16.5h.01',
		namespace: 'M3 6h7l2 2h9v11H3z',
		service: 'M12 4v5M5 20v-4h14v4M12 12v4M9 9h6v3H9z',
		ingress:
			'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM3 12h18M12 3c2.5 2.5 3.5 5.5 3.5 9s-1 6.5-3.5 9M12 3C9.5 5.5 8.5 8.5 8.5 12s1 6.5 3.5 9',
		config: 'M6 3h9l4 4v14H6zM14 3v5h5M9 12h7M9 16h7',
		secret: 'M8 15a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM11 11h10M18 11v3M21 11v3',
		volume:
			'M4 7c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 7v10c0 1.7 3.6 3 8 3s8-1.3 8-3V7M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3',
		event: 'M3 12h4l3-8 4 16 3-8h4',
		helm: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 3v5M12 16v5M3 12h5M16 12h5',
		topology: 'M6 3a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM18 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM6 9v3a3 3 0 0 0 3 3h6',
		access: 'M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4',
		sanitizer: 'M12 3l8 3v6c0 4.5-3.4 8-8 9-4.6-1-8-4.5-8-9V6zM8.5 12l2.5 2.5 4.5-5',
		forward: 'M4 8h14M14 4l4 4-4 4M20 16H6M10 12l-4 4 4 4',
		plug: 'M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4',
		sizing: 'M3 20h18M6 20v-5M10 20v-9M14 20v-6M18 20V6M16 8l2-2 2 2',
		benchmark: 'M12 20a8 8 0 1 1 8-8M12 12l4-4M4 20h16',
		settings:
			'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1',
		resource: 'M4 4h16v16H4zM4 9h16M9 9v11',
		bookmark: 'M6 3h12v18l-6-4-6 4z',
		search: 'M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4',
		chevron: 'M6 9l6 6 6-6',
		up: 'M6 15l6-6 6 6',
		back: 'M15 18l-6-6 6-6',
		next: 'M9 6l6 6-6 6',
		moon: 'M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z',
		sun: 'M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4',
		lock: 'M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4',
		unlock: 'M5 11h14v10H5zM8 11V7a4 4 0 0 1 7.5-2',
		critical: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7.5v5.5M12 16.5h.01',
		warning: 'M12 4l9 16H3zM12 10v4M12 17h.01',
		ok: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM8 12.5l2.5 2.5L16 9.5',
		pending: 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2',
		terminal: 'M4 17l6-5-6-5M12 19h8',
		book: 'M4 5a2 2 0 0 1 2-2h14v16H6a2 2 0 0 0-2 2zM4 21V5',
		arrow: 'M5 12h14M13 6l6 6-6 6',
		close: 'M6 6l12 12M18 6L6 18',
		logs: 'M4 6h16M4 10h11M4 14h16M4 18h8',
		split: 'M4 4h16v16H4zM12 4v16',
		dockBottom: 'M4 4h16v16H4zM4 14h16',
		dockRight: 'M4 4h16v16H4zM14 4v16',
		plus: 'M12 5v14M5 12h14',
		popout: 'M14 4h6v6M20 4l-8 8M18 14v6H4V6h6',
		download: 'M12 4v11M7 10l5 5 5-5M4 20h16',
		keyboard: 'M3 6h18v12H3zM7 10h.01M11 10h.01M15 10h.01M7 14h10'
	} as const;

	export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
	let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>

<svg class="icon" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
	<path d={PATHS[name]} />
</svg>

<style>
	.icon {
		flex: none;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.7;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
