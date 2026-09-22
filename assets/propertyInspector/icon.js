// The icon picker shared by the property inspectors: opens a file dialog and
// hands back a data URI, normalized so nothing downstream has to resize or pad.
//
// A key image is 128×128 and a dial's touchstrip icon box is square, so a
// non-square picture would come out stretched. The chosen file is drawn centred
// on a transparent 128×128 canvas, aspect preserved, and re-encoded as PNG.
// That also bounds what the icon costs: it is stored base64 in the profile JSON
// and embedded in every key image the plugin redraws, so a full-resolution
// photo would otherwise cross the websocket at each volume tick.
//
// SVGs pass through untouched — they scale by themselves, and a canvas would
// flatten them to a bitmap.

const ICON_SIDE = 128;

// Fit `uri` on a transparent square canvas. Resolves to the original for an
// SVG, or for anything the webview can't decode.
const squareIcon = (uri) =>
	new Promise((resolve) => {
		if (uri.startsWith("data:image/svg+xml")) return resolve(uri);
		const img = new Image();
		img.onload = () => {
			const ratio = Math.min(ICON_SIDE / img.width, ICON_SIDE / img.height);
			const w = Math.max(1, Math.round(img.width * ratio));
			const h = Math.max(1, Math.round(img.height * ratio));
			const canvas = document.createElement("canvas");
			canvas.width = canvas.height = ICON_SIDE;
			const ctx = canvas.getContext("2d");
			ctx.imageSmoothingQuality = "high";
			ctx.drawImage(img, (ICON_SIDE - w) / 2, (ICON_SIDE - h) / 2, w, h);
			resolve(canvas.toDataURL("image/png"));
		};
		img.onerror = () => resolve(uri);
		img.src = uri;
	});

// Open a file dialog and hand the chosen image back as a square data URI.
const pickIcon = (cb) => {
	const inp = document.createElement("input");
	inp.type = "file";
	inp.accept = "image/*";
	// Kept in the document (invisible): a detached file input does not
	// reliably open a picker / fire change in every webview.
	inp.style.cssText = "position:fixed;opacity:0;pointer-events:none;";
	document.body.appendChild(inp);
	inp.addEventListener("change", () => {
		const f = inp.files && inp.files[0];
		inp.remove();
		if (!f) return;
		const r = new FileReader();
		r.onload = () => squareIcon(r.result).then(cb);
		r.readAsDataURL(f);
	});
	inp.click();
};

// A [thumbnail-or-placeholder] pick button, plus a clear button once set.
// `save` persists the new value, `redraw` rebuilds the controls around it.
const iconControls = (getUri, setUri, save, redraw) => {
	const uri = getUri();
	const btn = document.createElement("button");
	btn.className = "icon-btn";
	btn.title = uri ? "Change icon" : "Set icon";
	if (uri) btn.style.backgroundImage = `url("${uri}")`;
	else btn.textContent = "🖼";
	btn.addEventListener("click", () => pickIcon((u) => { setUri(u); save(); redraw(); }));
	const els = [btn];
	if (uri) {
		const clr = document.createElement("button");
		clr.textContent = "⌫";
		clr.title = "Remove icon";
		clr.addEventListener("click", () => { setUri(null); save(); redraw(); });
		els.push(clr);
	}
	return els;
};

// A single-icon row: keeps `container` in sync with the value behind
// `getUri` / `setUri`, persisting through `save`.
const iconRow = (container, getUri, setUri, save) => {
	const redraw = () => {
		container.innerHTML = "";
		for (const el of iconControls(getUri, setUri, save, redraw)) container.appendChild(el);
	};
	redraw();
};
