# shipsim-agent

`shipsim-agent` is a Python 3.11+ standard-library-only external participant.
It joins a session-v1 seat and submits ordinary protocol-v4 orders. Provider
credentials and conversation history stay in this process; the server sees no
provider configuration or API key.

Copy `agents.example.toml` to `~/.config/shipsim/agents.toml`, or set
`SHIPSIM_AGENT_CONFIG` to a private config path. Profiles contain an
`api_key_env` name, never a key value. The Fireworks profile reads
`FIREWORKS_API_KEY` only when that profile is used.

```bash
cp frontend/agent/agents.example.toml ~/.config/shipsim/agents.toml
frontend/agent/shipsim-agent profiles --json
printf '%s\n' "$JOIN_TOKEN" | frontend/agent/shipsim-agent play \
  --connect 127.0.0.1:4100 --profile local --join-token-stdin
```

The token is read from stdin (or an interactive masked prompt), never from
argv. Do not put it in shell history or ordinary logs. Provider failures and
invalid model decisions retry at most three times, then submit legal hold
orders for the current phase.

The example `fake` profile is deterministic and offline. It is useful for
testing the complete server/TUI/agent process chain without model credentials.

Tests are offline:

```bash
(cd frontend/agent && python3 -m unittest discover -s tests)
python3 -m compileall -q frontend/agent
```
