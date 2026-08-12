const sweeperOrigin = "https://sweeper.ctox.dev";
const frame = document.querySelector("#sweeper-frame");

function sendSweeperHandshake() {
  frame?.contentWindow?.postMessage(
    { type: "ctox-sweeper:parent-ready", version: 1 },
    sweeperOrigin,
  );
}

frame?.addEventListener("load", sendSweeperHandshake);
window.addEventListener("message", (event) => {
  if (event.origin !== sweeperOrigin) return;
  if (event.data?.type !== "ctox-sweeper:child-ready") return;
  sendSweeperHandshake();
});
