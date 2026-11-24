"use strict";

const byId = (id) => document.getElementById(id);
const video = byId("video");
const episode = byId("episode");
const variant = byId("variant");
const mirror = byId("mirror");
const previous = byId("previous");
const next = byId("next");
const intro = byId("intro");
const status = byId("status");
const token = location.pathname.split("/").pop();
const api = "/api/" + token + "/";
let state;
let hls;
let busy = false;
let ready = false;
let failed = false;
const tried = new Set();

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

function render() {
    byId("title").textContent = state.title;
    document.title = state.title + " · manga-tui";
    options(episode, state.episodes.map((item, index) => [
        index,
        "Ep. " + item.number + (item.title ? " · " + item.title : "")
    ]), state.episode);
    const activeVariant = state.mirrors[state.mirror].variant;
    options(variant, [...new Set(state.mirrors.map((item) => item.variant))].map((name) => [name, name]), activeVariant);
    options(mirror, state.mirrors.flatMap((item, index) =>
        item.variant === activeVariant ? [[index, item.name]] : []), state.mirror);
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
    return fetch(api + "progress", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        keepalive,
        body: JSON.stringify({
            generation: state.generation,
            episode_number: state.episodes[state.episode].number,
            position_seconds: Math.min(video.currentTime, video.duration),
            duration_seconds: video.duration
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
document.addEventListener("keydown", (event) => {
    if (event.repeat || event.ctrlKey || event.metaKey || event.altKey || event.target.closest("button, select, video")) return;
    const actions = {
        " ": () => video.paused ? play() : video.pause(),
        k: () => video.paused ? play() : video.pause(),
        f: () => (document.fullscreenElement ? document.exitFullscreen() : video.requestFullscreen()).catch(() => {}),
        m: () => { video.muted = !video.muted; },
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
