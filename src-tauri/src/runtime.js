class MouseSimulator {
	#prevEl = null;

	move(x, y) {
		const el = document.elementFromPoint(x, y);
		if (!el) {
			this.#prevEl = null;
			return null;
		}

		const base = { bubbles: true, cancelable: true, clientX: x, clientY: y, view: window };

		if (this.#prevEl !== el) {
			const prev = this.#prevEl;
			if (prev) {
				const leaving = this.#getLeavingElements(prev, el);
				prev.dispatchEvent(new PointerEvent("pointerout", { ...base, pointerType: "mouse", relatedTarget: el }));
				prev.dispatchEvent(new MouseEvent("mouseout", { ...base, relatedTarget: el }));
				for (const a of leaving) {
					a.dispatchEvent(new PointerEvent("pointerleave", { ...base, bubbles: false, pointerType: "mouse", relatedTarget: el }));
					a.dispatchEvent(new MouseEvent("mouseleave", { ...base, bubbles: false, relatedTarget: el }));
				}
			}
			el.dispatchEvent(new PointerEvent("pointerover", { ...base, pointerType: "mouse", relatedTarget: prev }));
			el.dispatchEvent(new MouseEvent("mouseover", { ...base, relatedTarget: prev }));
			const entering = this.#getEnteringElements(prev, el);
			for (const a of entering) {
				a.dispatchEvent(new PointerEvent("pointerenter", { ...base, bubbles: false, pointerType: "mouse", relatedTarget: prev }));
				a.dispatchEvent(new MouseEvent("mouseenter", { ...base, bubbles: false, relatedTarget: prev }));
			}
		}

		el.dispatchEvent(new PointerEvent("pointermove", { ...base, pointerType: "mouse" }));
		el.dispatchEvent(new MouseEvent("mousemove", base));

		this.#prevEl = el;
		return el;
	}

	#isAncestor(candidate, el) {
		let current = el.parentElement;
		while (current) {
			if (current === candidate) return true;
			current = current.parentElement;
		}
		return false;
	}

	#getLeavingElements(prevEl, nextEl) {
		const out = [];
		let current = prevEl;
		while (current) {
			if (!this.#isAncestor(current, nextEl) && current !== nextEl) out.push(current);
			current = current.parentElement;
		}
		return out;
	}

	#getEnteringElements(prevEl, nextEl) {
		const out = [];
		let current = nextEl;
		while (current) {
			if (prevEl && (this.#isAncestor(current, prevEl) || current === prevEl)) break;
			out.push(current);
			current = current.parentElement;
		}
		return out.reverse();
	}
}

const mouse = new MouseSimulator();

const webviewWindow = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();
const invoke = window.__TAURI__.core.invoke;

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

webviewWindow.listen("cursor-position", (event) => {
	mouse.move(event.payload.x, event.payload.y);
});

let focused = false;
let coverage = 0.0;
let coverageThreshold = 0.8;
let config = {};

try {
	const [visibility, initialConfig] = await Promise.all([
		invoke("get_visibility"),
		invoke("get_config"),
	]);
	coverage = visibility.coverage;
	focused = visibility.focused;
	config = initialConfig;
	setHashTransient(encodeConfigHash(config));
} catch (e) {
	console.error("underpane: failed to load initial state", e);
}

function covered(coverage) {
	return coverage >= coverageThreshold;
}

class Underpane extends EventTarget {
	constructor() {
		super();
	}

	get visibilityState() {
		return covered(coverage) ? "hidden" : "visible";
	}

	get focused() {
		return focused;
	}

	get coverage() {
		return coverage;
	}

	get coverageThreshold() {
		return coverageThreshold;
	}

	set coverageThreshold(threshold) {
		coverageThreshold = threshold;
	}

	get config() {
		return config;
	}
}

const underpane = new Underpane();

webviewWindow.listen("desktop-focus", (event) => {
	focused = event.payload.focused;
	underpane.dispatchEvent(new FocusEvent(focused ? "focus" : "blur"));
});

webviewWindow.listen("desktop-coverage", (event) => {
	const prevCoverage = coverage;
	coverage = event.payload.coverage;
	underpane.dispatchEvent(
		new CustomEvent("coveragechange", { detail: { coverage } })
	);
	if (covered(prevCoverage) !== covered(coverage)) {
		underpane.dispatchEvent(new Event("visibilitychange", {}))
	}
});

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

webviewWindow.listen("config-change", (event) => {
	config = event.payload.config;
	setHashTransient(encodeConfigHash(config));
	underpane.dispatchEvent(
		new CustomEvent("configchange", { detail: { config } })
	);
});

const windowAddEventListener = window.addEventListener.bind(window);
const windowRemoveEventListener = window.removeEventListener.bind(window);
const documentAddEventListener = document.addEventListener.bind(document);
const documentRemoveEventListener = document.removeEventListener.bind(document);

// Intercept document.visibilityState
Object.defineProperty(document, "visibilityState", {
	get() { return underpane.visibilityState; },
	configurable: true,
});

// Intercept document.hasFocus
Object.defineProperty(document, "hasFocus", {
	get() { return () => underpane.focused; },
	configurable: true,
});

// Intercept window.addEventListener/removeEventListener for blur and focus
window.addEventListener = function (type, listener, options) {
	if (type === "blur" || type === "focus") {
		underpane.addEventListener(type, listener, options);
	} else {
		windowAddEventListener(type, listener, options);
	}
};

window.removeEventListener = function (type, listener, options) {
	if (type === "blur" || type === "focus") {
		underpane.removeEventListener(type, listener, options);
	} else {
		windowRemoveEventListener(type, listener, options);
	}
};

// Intercept document.addEventListener/removeEventListener for visibilitychange
document.addEventListener = function (type, listener, options) {
	if (type === "visibilitychange" || type === "blur" || type === "focus") {
		underpane.addEventListener(type, listener, options);
	} else {
		documentAddEventListener(type, listener, options);
	}
};
document.removeEventListener = function (type, listener, options) {
	if (type === "visibilitychange" || type === "blur" || type === "focus") {
		underpane.removeEventListener(type, listener, options);
	} else {
		documentRemoveEventListener(type, listener, options);
	}
};

// Intercept window.onblur and window.onfocus
let windowOnblur = null;
let windowOnfocus = null;
Object.defineProperty(window, "onblur", {
	get() { return windowOnblur; },
	set(handler) {
		if (windowOnblur) underpane.removeEventListener("blur", windowOnblur);
		windowOnblur = handler;
		if (handler) underpane.addEventListener("blur", handler);
	},
	configurable: true,
});

Object.defineProperty(window, "onfocus", {
	get() { return windowOnfocus; },
	set(handler) {
		if (windowOnfocus) underpane.removeEventListener("focus", windowOnfocus);
		windowOnfocus = handler;
		if (handler) underpane.addEventListener("focus", handler);
	},
	configurable: true,
});

// Intercept document.onvisibilitychange
let documentOnvisibilitychange = null;
let documentOnblur = null;
let documentOnfocus = null;
Object.defineProperty(document, "onvisibilitychange", {
	get() { return documentOnvisibilitychange; },
	set(handler) {
		if (documentOnvisibilitychange) underpane.removeEventListener("visibilitychange", documentOnvisibilitychange);
		documentOnvisibilitychange = handler;
		if (handler) underpane.addEventListener("visibilitychange", handler);
	},
	configurable: true,
});

Object.defineProperty(document, "onblur", {
	get() { return documentOnblur; },
	set(handler) {
		if (documentOnblur) underpane.removeEventListener("blur", documentOnblur);
		documentOnblur = handler;
		if (handler) underpane.addEventListener("blur", handler);
	},
	configurable: true,
});

Object.defineProperty(document, "onfocus", {
	get() { return documentOnfocus; },
	set(handler) {
		if (documentOnfocus) underpane.removeEventListener("focus", documentOnfocus);
		documentOnfocus = handler;
		if (handler) underpane.addEventListener("focus", handler);
	},
	configurable: true,
});
