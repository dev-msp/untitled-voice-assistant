# Untitled Voice UI

Uses [`whisper.cpp`](https://github.com/ggerganov/whisper.cpp) by way of [`whisper-rs`](https://github.com/tazz4843/whisper-rs).

> ⚠️ DISCLAIMER - this probably does not work with MBP mics. Whisper expects 16k
> sample rate, but Macbook mics don't have that option and I'm not handling
> downsampling yet. Use a mic that directly supports 16k in the meantime.

Bind to hotkeys as you prefer. I'm running the server as a daemon with `launchd`.

If you don't already have a `whisper.cpp`-compatible model, follow that project's [quick-start instructions](https://github.com/ggerganov/whisper.cpp#quick-start) to get one.

Start the server:
`./run.sh macbook ggml-base.en.bin /tmp/whisper.sock`

Start recording:

`$ echo -n "{\"type\": \"start\"}" | socat -t2 - /tmp/whisper.sock`

Output: `{ "type": "ack" }`

Stop recording:

`echo -n "{\"type\": \"stop\"}" | socat -t2 - /tmp/whisper.sock`

Output:
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

Note: the modes that you get back in the output are just metadata. Your client application that handles reading from the socket should also handle processing the transcription differently based on the mode.
