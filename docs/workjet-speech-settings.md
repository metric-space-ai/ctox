# Native speech settings for Workjet

Workjet controls the selected CTOX instance through the authenticated native
DataChannel method `ctox.workjet.speech.settings.v1`, capability
`ctox-workjet-speech-settings-v1`. Full settings, credential replacement,
voice discovery and checks require the current native Owner/Admin peer.
Settings are not Business OS collection data and have no HTTP fallback.

Actions are read, configure, key, voices, check, check.transcription and playback, prefixed
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

The transcription check synthesizes a short German reference clip with the saved
Mistral voice and replays its actual mono signed-16-bit WAV PCM at capture cadence
through the configured Mistral realtime gateway. It requires both speech paths
to select Mistral; it never silently substitutes a backend or records the microphone.
A nonempty branded gateway final defines success. The persisted latency measures
gateway audio-end to provider final, excluding synthesis, connection setup,
client transport and VAD. Partial-before-audio-end and audio duration are retained.
The probe has a 25-second total deadline and closes its stream on peer retirement.
TTS and STT results persist separately under the same configuration/credential
binding. Diagnostics confer no authority to append a meeting transcript.

Reference: https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription

Installed microphone/narration acceptance and local GPU readiness remain open.
Computer-pool enrollment uses the existing authorized speech computer contract.
