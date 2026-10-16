// Übergang (Kern-Bitte 15.10., §16.40): alte Clients senden noch business_os.chat.task. Die ctox-App nimmt den Typ an
// und behandelt ihn exakt wie ctox.chat.task. Löschen, sobald der Harness-Abbau-Strang null Sender misst
// (v4/debt/LOESCHPLAN.md L-35).
export { default } from './ctox.chat.task.js';
