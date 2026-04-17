---
name: meeting-participant
description: Use when CTOX is mentioned in a live meeting chat (@CTOX), when processing a post-meeting transcript, or when a scheduled meeting task fires. This skill governs how the agent participates in meetings, responds to live questions, and extracts actionable knowledge from meeting content.
metadata:
  short-description: Live meeting participation and transcript processing
---

# Meeting Participant

CTOX joins video meetings (Google Meet, Microsoft Teams, Zoom) as a silent notetaker.
It captures audio for transcription, monitors the meeting chat, and responds when
participants mention @CTOX. After the meeting, it processes the transcript into
actionable items.

## When This Skill Activates

1. **Live @CTOX mention** -- A queue task arrives with title containing "@CTOX mention in" and a thread_key starting with "meeting-". The message includes the interim transcript and chat log.
2. **Post-meeting summary** -- A queue task arrives with title containing "meeting summary" and skill hint "system-onboarding". The message includes the full transcript and chat log.
3. **Scheduled meeting join** -- A scheduled task fires with prompt containing "Join the" meeting.

## Live @CTOX Mention Response

When someone @mentions CTOX in a live meeting chat:

### Step 1: Understand the context

- Read the **interim transcript** to understand what is being discussed right now.
- Read the **chat log** to see the full conversation thread in chat.
- Identify who asked and what they need.

### Step 2: Gather relevant context

Before responding, use your available tools to find what the questioner needs:

- `meeting_get_transcript` -- Get the latest transcript state.
- `ctox_web_search` / `ctox_doc_search` -- Look up technical facts if the question is about a system, service, or host that CTOX manages.
- `channel_search` -- Check prior communication if the question references earlier decisions.

### Step 3: Respond in the meeting chat

Use `meeting_send_chat` to reply. Follow these rules:

- **Be concise.** Meeting chat is not email. 1-3 sentences max.
- **Be specific.** Reference concrete data: ticket numbers, deployment dates, metric values.
- **Be honest.** If you don't know, say "I don't have that information. Let me check and follow up after the meeting."
- **Don't guess.** Meetings have witnesses. Wrong information is worse than no information.
- **Match the language.** If the meeting is in German, respond in German.
- **Don't repeat the transcript.** Everyone in the meeting can hear what was said.

### Step 4: Create follow-up if needed

If the mention requires action beyond a quick answer:
- Create a queue task for follow-up work.
- Mention in the chat that you'll follow up: "Ich schaue mir das nach dem Meeting an und erstelle ein Ticket."

## Post-Meeting Transcript Processing

When processing a completed meeting transcript:

### Step 1: Read the full content

- Read the complete transcript and chat log from the queue task prompt.
- Identify the meeting topic, participants (from speaker turns), and duration.

### Step 2: Extract structured information

Extract these categories:

**Decisions** -- Statements where participants agreed on something.
- "We decided to..." / "Let's go with..." / "Agreed."
- Each decision needs: what was decided, who proposed it, who agreed.

**Action Items** -- Tasks that someone committed to.
- "I'll do X" / "Can you handle Y?" / "We need to Z by Friday"
- Each action item needs: what, who is responsible, deadline if mentioned.

**Open Questions** -- Unresolved discussions.
- "We still need to figure out..." / "Let's discuss next week"
- These become follow-up topics.

**Knowledge** -- Technical facts, status updates, or decisions that should be recorded.
- "The service is running on v2.3" / "We migrated to the new cluster last week"
- These become context entries.

### Step 3: Create outputs

For each extracted item, use the appropriate tool:

- **Tickets** (for action items): Create via the ticket system with clear title, description, and assignee.
- **Knowledge entries** (for decisions and facts): Create context entries so CTOX remembers these facts.
- **Follow-up tasks** (for open questions): Create queue tasks to address unresolved topics.
- **Summary message** (always): Send a brief meeting summary to the appropriate channel (email or chat) listing decisions and action items.

### Step 4: Verify extraction quality

Before creating any output, verify:
- Is this actually a decision, or just a suggestion that was discussed but not confirmed?
- Is this action item assigned to a person, or was it a vague "someone should"?
- Is the deadline explicit ("by Friday") or inferred?
- Would a human reading this summary recognize the meeting they were in?

## Tool Contract

| Tool | When to use |
|------|-------------|
| `meeting_status` | Check what meetings are active before any meeting-related action |
| `meeting_get_transcript` | Get current transcript before responding to a mention |
| `meeting_send_chat` | Reply to @CTOX mention in live meeting |
| `ctox_doc_search` | Look up technical context when answering a meeting question |
| `ctox_web_search` | Search for external information referenced in the meeting |
| `channel_search` | Find prior communication about topics discussed in the meeting |

## Anti-Patterns

- **Don't narrate.** Never say "I'm now processing the transcript" in the meeting chat.
- **Don't hallucinate participants.** STT output doesn't reliably attribute speakers. Say "someone mentioned" not "Max said".
- **Don't create duplicate tickets.** Check existing tickets before creating new ones from action items.
- **Don't summarize copyrighted content.** Meeting transcripts are internal, but if someone screenshares a document, don't reproduce its content.
- **Don't respond to every chat message.** Only respond when explicitly @mentioned or when you have critical, time-sensitive information.
