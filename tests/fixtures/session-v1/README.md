# Session protocol v1 golden traces

Each NDJSON file records canonical session-framing messages for one lobby
controller pairing. They stop when the lobby reaches `running`; ordinary game
traffic after that point remains unwrapped protocol v4.

- `human-vs-bot.ndjson` — one external Human seat and one server-owned Bot.
- `human-vs-human.ndjson` — host assignment, invitation, and second Human join.
- `human-vs-llm-agent.ndjson` — host assignment, invitation, external agent
  join, and advisory readiness.

Tokens and identifiers are inert fixture values. Production tokens must never
be written to ordinary logs.
