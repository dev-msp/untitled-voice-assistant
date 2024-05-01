# `llm`

Utilities for text completion.

## Configuration

Currently `llm` sources its config from `$XDG_CONFIG_HOME/voice/config.toml`.

### File structure

```toml
[transcription]
model_dir = "/path/to/whisper.cpp/models"

[llm.providers.openai]
enabled = true

# works well with CLI utilities like `pass` or 1Password's `op`
api_key_command = [
  "command",
  "that",
  "writes",
  "api_key",
  "to",
  "stdout",
]

[llm.providers.groq]
enabled = true
api_key_command = [ ... ]

[llm.providers.ollama]
enabled = true
host = "localhost"
port = 11434
```

## Usage

```
Usage: voice-llm <COMMAND>

Commands:
  completion
  list-models
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### `completion`

```
Usage: voice-llm completion [OPTIONS] --provider <PROVIDER> <USER_MESSAGE>

Arguments:
  <USER_MESSAGE>

Options:
  -p, --provider <PROVIDER>              [possible values: openai, groq, ollama]
  -s, --system-message <SYSTEM_MESSAGE>
  -h, --help                             Print help
```

### `list-models`

```
Usage: voice-llm list-models

Options:
  -h, --help  Print help
```
