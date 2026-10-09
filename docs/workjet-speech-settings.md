# Native speech settings for Workjet

Workjet controls the selected CTOX instance through the authenticated native
DataChannel method `ctox.workjet.speech.settings.v1`, capability
`ctox-workjet-speech-settings-v1`. Full settings, credential replacement,
voice discovery and checks require the current native Owner/Admin peer.
Settings are not Business OS collection data and have no HTTP fallback.

Actions are read, configure, key, voices, check and playback, prefixed
`speech.settings.`, with a UUID commandId echoed in each receipt. Key inputs
are transient: the native handler writes only the encrypted credential
`CTOX_MISTRAL_API_KEY`; it never projects, logs or returns the key. Discovery
returns only IDs and names from the account's actual saved Mistral voices.
Configuration is saved in the existing speech-config SQLite payload.

The rate is a number from 0.8 to 1.5, default 1.15 for existing configurations
as well as new ones. The documented Mistral speech API has no rate parameter.
Workjet applies the persisted rate after synthesis using HTMLAudioElement
playbackRate with preservesPitch enabled, including retained Jour fixe audio.
No DSP dependency or environment toggle is introduced. Authenticated members
can read only the playback rate; they cannot read credential presence or edit
settings.

The API voice check synthesizes a short German sentence with a 20-second IO
budget. Green means the actual selected API returned a bounded, parseable WAV.
The result, latency and error class persist privately and are invalidated by
configuration or credential replacement. Configuration alone remains
unchecked. HTTP rejection classes come from actual upstream status codes;
missing key or voice remain local prerequisites. No provider body is returned.

This slice exposes API configuration and a TTS check. It does not claim a live
STT check, installed narration acceptance or local GPU readiness. Computer-pool
enrollment and STT acceptance remain separate work on the existing native
speech computer and authorized meeting transport contracts.
