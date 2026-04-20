class MouseSimulator {
	#prevEl = null;

	move(x, y) {
		const el = document.elementFromPoint(x, y);
		if (!el) return;

		if (this.#prevEl && this.#prevEl !== el) {
			const leaving = this.#getLeavingElements(this.#prevEl, el);
			const entering = this.#getEnteringElements(this.#prevEl, el);

			leaving.forEach(a => a.dispatchEvent(new MouseEvent("mouseleave", {
				bubbles: false, clientX: x, clientY: y, view: window, relatedTarget: el
			})));

			this.#prevEl.dispatchEvent(new MouseEvent("mouseout", {
				bubbles: true, clientX: x, clientY: y, view: window, relatedTarget: el
			}));

			el.dispatchEvent(new MouseEvent("mouseover", {
				bubbles: true, clientX: x, clientY: y, view: window, relatedTarget: this.#prevEl
			}));

			entering.forEach(a => a.dispatchEvent(new MouseEvent("mouseenter", {
				bubbles: false, clientX: x, clientY: y, view: window, relatedTarget: this.#prevEl
			})));
		}

		el.dispatchEvent(new MouseEvent("mousemove", {
			bubbles: true, clientX: x, clientY: y, view: window
		}));

		this.#prevEl = el;
		return el;
	}

	reset() {
		this.#prevEl = null;
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
		const leaving = [];
		let current = prevEl;
		while (current) {
			if (!this.#isAncestor(current, nextEl) && current !== nextEl) {
				leaving.push(current);
			}
			current = current.parentElement;
		}
		return leaving;
	}

	#getEnteringElements(prevEl, nextEl) {
		const entering = [];
		let current = nextEl;
		while (current) {
			if (!this.#isAncestor(current, prevEl) && current !== prevEl) {
				entering.push(current);
			}
			current = current.parentElement;
		}
		return entering.reverse(); // top-down order
	}
}

const mouse = new MouseSimulator();

const webviewWindow = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();

webviewWindow.listen("cursor-position", function (event) {
	mouse.move(event.payload.x, event.payload.y);
});

let focused = false;
let coverage = 0.0;
let coverageThreshold = 0.8;
let config = {};

function covered(coverage) {
	return coverage >= coverageThreshold;
}

class ActiveDesk extends EventTarget {
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

const activeDesk = new ActiveDesk();
window.activeDesk = activeDesk;

webviewWindow.listen("desktop-focus", (event) => {
	focused = event.payload.focused;
	activeDesk.dispatchEvent(new FocusEvent(focused ? "focus" : "blur"));
});

webviewWindow.listen("desktop-coverage", (event) => {
	const prevCoverage = coverage;
	coverage = event.payload.coverage;
	activeDesk.dispatchEvent(
		new CustomEvent("coveragechange", { detail: { coverage } })
	);
	if (covered(prevCoverage) !== covered(coverage)) {
		activeDesk.dispatchEvent(new Event("visibilitychange", {}))
	}
});

webviewWindow.listen("config-change", (event) => {
	config = event.payload.config;
	activeDesk.dispatchEvent(
		new CustomEvent("configchange", { detail: { config } })
	);
});

const windowAddEventListener = window.addEventListener.bind(window);
const windowRemoveEventListener = window.removeEventListener.bind(window);
const documentAddEventListener = document.addEventListener.bind(document);
const documentRemoveEventListener = document.removeEventListener.bind(document);

// Intercept document.visibilityState
Object.defineProperty(document, "visibilityState", {
	get() { return activeDesk.visibilityState; },
	configurable: true,
});

// Intercept document.hasFocus
Object.defineProperty(document, "hasFocus", {
	get() { return () => activeDesk.focused; },
	configurable: true,
});

// Intercept window.addEventListener/removeEventListener for blur and focus
window.addEventListener = function (type, listener, options) {
	if (type === "blur" || type === "focus") {
		activeDesk.addEventListener(type, listener, options);
	} else {
		windowAddEventListener(type, listener, options);
	}
};

window.removeEventListener = function (type, listener, options) {
	if (type === "blur" || type === "focus") {
		activeDesk.removeEventListener(type, listener, options);
	} else {
		windowRemoveEventListener(type, listener, options);
	}
};

// Intercept document.addEventListener/removeEventListener for visibilitychange
document.addEventListener = function (type, listener, options) {
	if (type === "visibilitychange" || type === "blur" || type === "focus") {
		activeDesk.addEventListener(type, listener, options);
	} else {
		documentAddEventListener(type, listener, options);
	}
};
document.removeEventListener = function (type, listener, options) {
	if (type === "visibilitychange" || type === "blur" || type === "focus") {
		activeDesk.removeEventListener(type, listener, options);
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
		if (windowOnblur) activeDesk.removeEventListener("blur", windowOnblur);
		windowOnblur = handler;
		if (handler) activeDesk.addEventListener("blur", handler);
	},
	configurable: true,
});

Object.defineProperty(window, "onfocus", {
	get() { return windowOnfocus; },
	set(handler) {
		if (windowOnfocus) activeDesk.removeEventListener("focus", windowOnfocus);
		windowOnfocus = handler;
		if (handler) activeDesk.addEventListener("focus", handler);
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
		if (documentOnvisibilitychange) activeDesk.removeEventListener("visibilitychange", documentOnvisibilitychange);
		documentOnvisibilitychange = handler;
		if (handler) activeDesk.addEventListener("visibilitychange", handler);
	},
	configurable: true,
});

Object.defineProperty(document, "onblur", {
	get() { return documentOnblur; },
	set(handler) {
		if (documentOnblur) activeDesk.removeEventListener("blur", documentOnblur);
		documentOnblur = handler;
		if (handler) activeDesk.addEventListener("blur", handler);
	},
	configurable: true,
});

Object.defineProperty(document, "onfocus", {
	get() { return documentOnfocus; },
	set(handler) {
		if (documentOnfocus) activeDesk.removeEventListener("focus", documentOnfocus);
		documentOnfocus = handler;
		if (handler) activeDesk.addEventListener("focus", handler);
	},
	configurable: true,
});
