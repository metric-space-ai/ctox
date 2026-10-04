"""Expiring model availability, separate from model quality assessments."""
import datetime as dt


def parse_time(value):
    parsed = dt.datetime.fromisoformat(value.replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        raise ValueError('Reset time must include a timezone')
    return parsed.astimezone(dt.timezone.utc)


def defer(model, until, note, now=None):
    now = now or dt.datetime.now(dt.timezone.utc)
    retry = parse_time(until) if until else now + dt.timedelta(days=1)
    if retry <= now:
        raise ValueError('Retry time must be in the future')
    if not note.strip() or len(note) > 500:
        raise ValueError('Use a concise, sanitized quota/capacity note')
    return {'model': model, 'retry_at': retry.isoformat(), 'observed_at': now.isoformat(),
            'reason': note.strip(), 'kind': 'temporary_availability',
            'reset_source': 'provider_or_operator' if until else 'unknown_reset_retry_next_day'}


def active(entry, now=None):
    now = now or dt.datetime.now(dt.timezone.utc)
    return parse_time(entry['retry_at']) > now
