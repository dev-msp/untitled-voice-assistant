# Untitled Voice UI

Uses [`whisper.cpp`](https://github.com/ggerganov/whisper.cpp) by way of [`whisper-rs`](https://github.com/tazz4843/whisper-rs).

Bind requests to hotkeys as you prefer. I'm running the server as a daemon with `launchd`.

If you don't already have a `whisper.cpp`-compatible model, follow that project's [quick-start instructions](https://github.com/ggerganov/whisper.cpp#quick-start) to get one.

## Quick start

In your terminal:

```sh
# Build the server and client
INSTALL_DIR=/something/on/your/PATH make install

# Start the server
# (see run.sh for why running the binary directly doesn't work yet)
./run.sh localhost:8088 $PATH_TO_MODEL
```

In a separate shell:

```sh
# Send a start command to the server.
#
# Note that `-i` is optional, without it the server will use the first
# compatible device. For example, you might pass "MacBook" if you want to use
# your laptop's built-in mic ("MacBook Pro Microphone").
voice-client localhost:8088 start --model small -i $PARTIAL_INPUT_DEVICE_NAME
```

After executing this command, the server will start recording from this specified input. To get the results, send the stop command:

```sh
voice-client localhost:8088 stop
```

The results will be printed to stdout.
