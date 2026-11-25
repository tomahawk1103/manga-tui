"use strict";

const byId = (id) => document.getElementById(id);
const video = byId("video");
const episode = byId("episode");
const variant = byId("variant");
const mirror = byId("mirror");
const previous = byId("previous");
const next = byId("next");
const intro = byId("intro");
const theater = byId("theater");
const status = byId("status");
const cover = byId("cover");
const coverFallback = byId("cover-fallback");
const metaBadges = byId("meta-badges");
const genresBox = byId("genres");
const synopsisBox = byId("synopsis");
const epBadge = byId("ep-badge");
const epCount = byId("ep-count");
const epFilter = byId("ep-filter");
const epList = byId("ep-list");
const streamInfo = byId("stream-info");
const providerPill = byId("provider-pill");
const token = location.pathname.split("/").pop();
const api = "/api/" + token + "/";
let state;
let hls;
let busy = false;
let ready = false;
let failed = false;
const tried = new Set();

const STATUS_LABELS = {
    Ongoing: "En emisión",
    Completed: "Finalizado",
    Upcoming: "Próximamente"
};

function options(select, entries, selected) {
    select.replaceChildren(...entries.map(([value, label]) => new Option(label, String(value))));
    select.value = String(selected);
}

function controls() {
    episode.disabled = !state || busy;
    previous.disabled = !state || busy || state.episode === 0;
    next.disabled = !state || busy || state.episode + 1 >= state.episodes.length;
    variant.disabled = !state || busy || variant.options.length < 2;
    mirror.disabled = !state || busy || mirror.options.length < 2;
    intro.disabled = !ready || busy;
}

function makeChip(text, accent = false) {
    const span = document.createElement("span");
    span.className = accent ? "chip accent" : "chip";
    span.textContent = text;
    return span;
}

function renderMetadata() {
    if (providerPill && state.provider) providerPill.textContent = state.provider;
    if (coverFallback) {
        const initials = (state.title || "AN").trim().slice(0, 2).toUpperCase();
        coverFallback.textContent = initials || "AN";
    }
    if (cover) {
        if (state.cover && cover.dataset.loaded !== state.cover) {
            cover.dataset.loaded = state.cover;
            cover.onload = () => {
                cover.hidden = false;
                if (coverFallback) coverFallback.hidden = true;
            };
            cover.onerror = () => {
                cover.hidden = true;
                if (coverFallback) coverFallback.hidden = false;
            };
            cover.src = state.cover;
        } else if (!state.cover) {
            cover.hidden = true;
            if (coverFallback) coverFallback.hidden = false;
        }
    }
    if (metaBadges) {
        const badges = [];
        if (state.anime_type && state.anime_type !== "Unknown") badges.push(makeChip(state.anime_type));
        if (STATUS_LABELS[state.status]) badges.push(makeChip(STATUS_LABELS[state.status], true));
        if (typeof state.score === "number") badges.push(makeChip("★ " + state.score.toFixed(1), true));
        metaBadges.replaceChildren(...badges);
    }
    if (genresBox) {
        const genres = Array.isArray(state.genres) ? state.genres.slice(0, 8) : [];
        genresBox.replaceChildren(...genres.map((g) => makeChip(g)));
    }
    if (synopsisBox) {
        const text = (state.synopsis || "").trim();
        synopsisBox.textContent = text || "Sin sinopsis disponible para esta serie.";
    }
}

function renderEpisodeSidebar() {
    if (!epList || !state) return;
    if (epCount) epCount.textContent = state.episodes.length + " eps";
    const query = (epFilter?.value || "").trim().toLowerCase();
    const progressMap = state.all_progress || {};
    const buttons = [];

    state.episodes.forEach((item, index) => {
        const label = "Ep. " + item.number + (item.title ? " · " + item.title : "");
        if (query && !label.toLowerCase().includes(query) && String(item.number).toLowerCase() !== query) return;

        const btn = document.createElement("button");
        btn.type = "button";
        const p = progressMap[String(index)];
        const ratio = p && p.duration_seconds > 0 ? Math.min(1, p.position_seconds / p.duration_seconds) : 0;
        const watched = ratio >= 0.85;
        const isActive = index === state.episode;
        btn.className = "ep-item" + (isActive ? " active" : "") + (watched ? " watched" : "");
        btn.disabled = busy;
        btn.dataset.index = String(index);

        const textSpan = document.createElement("span");
        textSpan.className = "ep-label";
        textSpan.textContent = label;

        const stateSpan = document.createElement("span");
        stateSpan.className = "ep-state";
        stateSpan.textContent = isActive ? "▶ Viendo" : watched ? "✓ Visto" : ratio > 0.02 ? Math.round(ratio * 100) + "%" : "";

        btn.append(textSpan, stateSpan);
        if (ratio > 0.02 && !isActive) {
            const bar = document.createElement("span");
            bar.className = "ep-progress";
            bar.style.width = Math.round(ratio * 100) + "%";
            btn.append(bar);
        }
        btn.addEventListener("click", () => chooseEpisode(index));
        buttons.push(btn);
    });

    epList.replaceChildren(...buttons);
    const activeBtn = epList.querySelector(".ep-item.active");
    if (activeBtn) activeBtn.scrollIntoView({ block: "nearest" });
}

function renderStreamInfo() {
    if (!streamInfo || !state) return;
    const currentMirror = state.mirrors[state.mirror] || { name: "-", variant: "-" };
    const entries = [
        ["Servidor", currentMirror.name],
        ["Audio / Sub", currentMirror.variant],
        ["Formato", state.kind ? String(state.kind).toUpperCase() : "-"],
        ["Alternativas", state.mirrors.length + " disponibles"]
    ];
    streamInfo.replaceChildren(...entries.map(([k, v]) => {
        const cell = document.createElement("div");
        const small = document.createElement("small");
        small.textContent = k;
        const strong = document.createElement("strong");
        strong.textContent = v;
        cell.append(small, strong);
        return cell;
    }));
}

function render() {
    const activeEp = state.episodes[state.episode];
    byId("title").textContent = state.title;
    if (epBadge && activeEp) {
        epBadge.textContent = "Ep. " + activeEp.number + " / " + state.episodes.length;
    }
    document.title = state.title + (activeEp ? " · Ep. " + activeEp.number : "") + " · manga-tui";
    options(episode, state.episodes.map((item, index) => [
        index,
        "Ep. " + item.number + (item.title ? " · " + item.title : "")
    ]), state.episode);
    const activeVariant = state.mirrors[state.mirror].variant;
    options(variant, [...new Set(state.mirrors.map((item) => item.variant))].map((name) => [name, name]), activeVariant);
    options(mirror, state.mirrors.flatMap((item, index) =>
        item.variant === activeVariant ? [[index, item.name]] : []), state.mirror);
    renderMetadata();
    renderEpisodeSidebar();
    renderStreamInfo();
    controls();
}

async function request(action, data) {
    const response = await fetch(api + action, data && {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(data)
    });
    if (!response.ok) throw new Error(String(response.status));
    return response.status === 204 ? undefined : response.json();
}

function saveProgress(keepalive = false) {
    if (!state || !ready || !Number.isFinite(video.duration) || video.duration <= 0) return Promise.resolve();
    const position_seconds = Math.min(video.currentTime, video.duration);
    const duration_seconds = video.duration;
    state.all_progress = state.all_progress || {};
    state.all_progress[String(state.episode)] = { position_seconds, duration_seconds };
    return fetch(api + "progress", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        keepalive,
        body: JSON.stringify({
            generation: state.generation,
            episode_number: state.episodes[state.episode].number,
            position_seconds,
            duration_seconds
        })
    }).catch(() => {});
}

function play() {
    video.play().catch(() => {});
}

function loadMedia() {
    ready = false;
    failed = false;
    tried.add(state.mirror);
    hls?.destroy();
    hls = undefined;
    video.removeAttribute("src");
    video.load();
    render();
    const generation = state.generation;
    video.onloadedmetadata = () => {
        if (generation !== state.generation) return;
        const position = state.progress.position_seconds;
        if (position > 0 && position < video.duration) video.currentTime = position;
        ready = true;
        status.textContent = "";
        controls();
    };

    if (state.kind === "Hls" && window.Hls?.isSupported()) {
        hls = new Hls({ enableWorker: false });
        hls.once(Hls.Events.MANIFEST_PARSED, play);
        hls.on(Hls.Events.ERROR, (_event, error) => {
            if (error.fatal && state.generation === generation) handleFailure();
        });
        hls.loadSource(state.stream);
        hls.attachMedia(video);
    } else {
        video.src = state.stream;
        play();
    }
}

async function change(action, data) {
    if (!state || busy) return;
    busy = true;
    controls();
    renderEpisodeSidebar();
    try {
        await saveProgress();
        state = await request(action, data);
        tried.clear();
        status.textContent = "";
        loadMedia();
    } catch {
        render();
        status.textContent = "No se pudo cargar la selección.";
    } finally {
        busy = false;
        controls();
        renderEpisodeSidebar();
        if (failed) handleFailure();
    }
}

async function handleFailure() {
    if (!state) return;
    failed = true;
    if (busy) return;
    busy = true;
    controls();
    status.textContent = "Probando otro servidor…";
    try {
        const activeVariant = state.mirrors[state.mirror].variant;
        const alternatives = state.mirrors.flatMap((item, index) =>
            item.variant === activeVariant && !tried.has(index) ? [index] : []);
        for (const index of alternatives) {
            tried.add(index);
            let replacement;
            try {
                await saveProgress();
                replacement = await request("mirror", { generation: state.generation, mirror: index });
            } catch {
                continue;
            }
            state = replacement;
            loadMedia();
            return;
        }
        status.textContent = "No quedan servidores disponibles para esta variante.";
    } finally {
        busy = false;
        controls();
    }
}

function chooseEpisode(index) {
    if (state && index >= 0 && index < state.episodes.length && index !== state.episode) {
        change("episode", { episode: index });
    }
}

function chooseMirror(index) {
    if (state && index >= 0 && index !== state.mirror) {
        change("mirror", { generation: state.generation, mirror: index });
    }
}

function seek(seconds) {
    if (ready && Number.isFinite(video.duration)) {
        video.currentTime = Math.max(0, Math.min(video.duration, video.currentTime + seconds));
    }
}

function toggleTheater() {
    document.body.classList.toggle("theater");
}

video.addEventListener("error", () => {
    if (video.error) handleFailure();
});
video.addEventListener("pause", () => saveProgress());
video.addEventListener("ended", () => saveProgress());
window.addEventListener("pagehide", () => saveProgress(true));
setInterval(saveProgress, 10_000);
episode.addEventListener("change", () => chooseEpisode(Number(episode.value)));
mirror.addEventListener("change", () => chooseMirror(Number(mirror.value)));
variant.addEventListener("change", () => chooseMirror(state.mirrors.findIndex((item) => item.variant === variant.value)));
previous.addEventListener("click", () => chooseEpisode(state.episode - 1));
next.addEventListener("click", () => chooseEpisode(state.episode + 1));
intro.addEventListener("click", () => seek(85));
theater?.addEventListener("click", toggleTheater);
epFilter?.addEventListener("input", renderEpisodeSidebar);
document.addEventListener("keydown", (event) => {
    if (event.repeat || event.ctrlKey || event.metaKey || event.altKey || event.target.closest("input, button, select, video")) return;
    const actions = {
        " ": () => video.paused ? play() : video.pause(),
        k: () => video.paused ? play() : video.pause(),
        f: () => (document.fullscreenElement ? document.exitFullscreen() : video.requestFullscreen()).catch(() => {}),
        m: () => { video.muted = !video.muted; },
        t: toggleTheater,
        T: toggleTheater,
        ArrowLeft: () => seek(-5),
        ArrowRight: () => seek(5),
        n: () => chooseEpisode(state.episode + 1),
        p: () => chooseEpisode(state.episode - 1),
        s: () => seek(85)
    };
    if (!state || !Object.hasOwn(actions, event.key)) return;
    event.preventDefault();
    actions[event.key]();
});

request("state").then((initial) => {
    state = initial;
    loadMedia();
}).catch(() => { status.textContent = "La sesión de reproducción no está disponible."; });
