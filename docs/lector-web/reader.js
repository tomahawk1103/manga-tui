const page = document.querySelector("#page");
const position = document.querySelector("#position");
const previous = document.querySelector("#previous");
const next = document.querySelector("#next");
const storageKey = "mangora-web-demo-page";
const pageCount = 3;
let current = 1;

try {
  const saved = Number(localStorage.getItem(storageKey));
  if (Number.isInteger(saved) && saved >= 1 && saved <= pageCount) current = saved;
} catch {
  // Reading still works if storage is unavailable.
}

function render() {
  page.src = `../images/pagina-${current}.jpg`;
  page.alt = `Página ${current} de la muestra`;
  position.textContent = `${current} de ${pageCount}`;
  previous.disabled = current === 1;
  next.disabled = current === pageCount;
}

function turn(direction) {
  const target = current + direction;
  if (target < 1 || target > pageCount) return;
  current = target;
  try {
    localStorage.setItem(storageKey, String(current));
  } catch {
    // Keep the current page in memory.
  }
  render();
  window.scrollTo(0, 0);
}

previous.addEventListener("click", () => turn(-1));
next.addEventListener("click", () => turn(1));
render();

const offline = document.querySelector("#offline");
if (window.isSecureContext && "serviceWorker" in navigator) {
  navigator.serviceWorker.register("./sw.js")
    .then(() => navigator.serviceWorker.ready)
    .then(() => { offline.textContent = "Capítulo disponible sin conexión en este dispositivo."; })
    .catch(() => { offline.textContent = "La muestra requiere conexión en este dispositivo."; });
}
