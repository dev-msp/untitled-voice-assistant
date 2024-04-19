# Untitled Voice UI

Uses [`whisper.cpp`](https://github.com/ggerganov/whisper.cpp) by way of [`whisper-rs`](https://github.com/tazz4843/whisper-rs).

Bind requests to hotkeys as you prefer. I'm running the server as a daemon with `launchd`.

If you don't already have a `whisper.cpp`-compatible model, follow that project's [quick-start instructions](https://github.com/ggerganov/whisper.cpp#quick-start) to get one.

Start the server:
`./run.sh macbook ggml-base.en.bin /tmp/whisper.sock`

Start recording: `curl -X POST -H "Content-Type: application/json" -d "$body" "http://127.0.0.1:8088/voice/$1"`

Example request body:
```json
{
  // partial name matches OK
  "input_device": "MacBook Pro Microphone",

  // optional
  "sample_rate": 44100,
}
```
Response: `{ "type": "ack" }`

Stop recording: `curl -X POST http://localhost:8088/voice/stop`
Example response:
```json
{
  "data": {
    "content": "And we're recording and we're doing stuff and then we're going to send a stop message.",
    "mode": {
      "type": "live_typing"
    }
  },
  "type": "transcription"
}
```

Note: the modes that you get back in the output are just metadata. Your client application that talks to the server should also handle processing the transcription differently based on the mode.
