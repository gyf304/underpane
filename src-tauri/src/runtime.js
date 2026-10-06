const webviewWindow = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();
const invoke = window.__TAURI__.core.invoke;

// Pointer input, visibility and focus are all driven natively by the window
// daemon, so the page only needs console forwarding and config plumbing here.

{
	const originalConsole = console;
	const forwardedLevels = new Set(["log", "info", "debug", "warn", "error", "trace"]);
	const wrapperCache = new Map();

	const formatArg = (a) => {
		if (typeof a === "string") return a;
		if (a instanceof Error) return a.stack || `${a.name}: ${a.message}`;
		try { return JSON.stringify(a); } catch { return String(a); }
	};

	const proxied = new Proxy(originalConsole, {
		get(target, prop, receiver) {
			const value = Reflect.get(target, prop, receiver);
			if (typeof prop !== "string" || !forwardedLevels.has(prop) || typeof value !== "function") {
				return typeof value === "function" ? value.bind(target) : value;
			}
			let wrapped = wrapperCache.get(prop);
			if (!wrapped) {
				const original = value.bind(target);
				wrapped = (...args) => {
					original(...args);
					try {
						const message = args.map(formatArg).join(" ");
						invoke("runtime_log", { level: prop, message }).catch(() => {});
					} catch {
						// Never let logging break the caller.
					}
				};
				wrapperCache.set(prop, wrapped);
			}
			return wrapped;
		},
	});

	try {
		window.console = proxied;
	} catch {
		Object.defineProperty(globalThis, "console", {
			value: proxied, configurable: true, writable: true,
		});
	}
}

function encodeConfigHash(cfg) {
	const params = new URLSearchParams();
	for (const [k, v] of Object.entries(cfg)) {
		params.set(k, String(v));
	}
	return params.toString();
}

function setHashTransient(hash) {
	const url = new URL(window.location.href);
	const oldURL = window.location.href;
	url.hash = hash;
	const newURL = url.toString();
	if (oldURL === newURL) return;
	history.replaceState(history.state, "", newURL);
	window.dispatchEvent(new HashChangeEvent("hashchange", { oldURL, newURL }));
}

try {
	setHashTransient(encodeConfigHash(await invoke("get_config")));
} catch (e) {
	console.error("underpane: failed to load initial config", e);
}

webviewWindow.listen("config-change", (event) => {
	setHashTransient(encodeConfigHash(event.payload.config));
});
