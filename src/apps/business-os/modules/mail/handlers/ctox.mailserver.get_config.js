// Origin: CTOX
// License: Apache-2.0
// ctox.mailserver.get_config – ref: store.rs:16415-16518 (handle_mailserver_command) @ 4794af03d
// Einstufung FAEHIGKEIT: Domains/DKIM/Nutzer liegen im eingebetteten Mailserver-Store (stalwart_*), Laufzeit-
// status und Erreichbarkeit (TCP-Probe SMTP/IMAP) sind native Wirkung. Keine Fachlogik im Rust-Arm außer der
// Rechteprüfung SecretsManage (Kern, Policy). Handler = Delegation an ctx.cap.mail.server.config.get.
import { callCap } from './_base.js';

export default async function handle(_command, ctx) {
  // Rust-Rückgabe: { domains[], users[], runtime_config, runtime_status, health{…} } – unverändert durchreichen.
  return callCap(ctx, 'mail.server.config.get', {});
}
