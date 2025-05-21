here's the plan:

1. **new http endpoint:** add a post endpoint, maybe `/transcribe`, in `server/src/web.rs`.
2. **receive audio and params:** this endpoint needs to accept the audio file data and transcription parameters (like model, sample rate, prompt) in one request. a standard way is `multipart/form-data`. actix-web has support for this.
3. **send job to daemon:** the web handler reads the audio bytes and parameters from the request. it sends this data to the daemon using the existing channel setup, but with a new command type.
4. **new command type:** introduce a `Command::TranscribeFile(Vec<u8>, TranscriptionParams)` command. `TranscriptionParams` would be a new struct holding model, sample rate, prompt, etc., that the client sends.
5. **daemon processes file:** the daemon receives `Command::TranscribeFile`. it converts the raw audio bytes (probably assuming a format like 16-bit pcm) into the `Vec<f32>` format the whisper worker expects. then it creates a whisper `Job` and sends it to the worker.
6. **daemon waits and responds:** the daemon waits *synchronously* for the transcription result from the worker (just like the `stop` command does now).
7. **include timings in response:** the whisper worker returns `Vec<sttx::Timing>`. modify the `Response::Transcription` struct to include this list of timings (word or sentence level). you'll need to make the timings data serializable to json.
8. **send response back:** the daemon sends the updated `Response::Transcription` back on the response channel.
9. **web handler returns json:** the web handler receives the response from the daemon and returns it as json using the `ApiResponder`.

this means changes in:
*  `server/src/web.rs` (new handler, multipart processing)
*  `src/app/command.rs` (new command type `TranscribeFile`)
*  `src/app/response.rs` (update `Response::Transcription`, add serializable timing struct)
*  `src/app/mod.rs` (daemon handles `TranscribeFile`, converts audio, creates job, adds timings to response)
*  `src/whisper/transcription.rs` (define `TranscriptionParams`)

you'd need a dependency like `actix-multipart` for the web server to handle file uploads. and you'll need to decide on the expected audio format for the raw bytes (e.g., 16-bit pcm mono at a specific sample rate, which the client must provide or the server assumes/validates).

this seems like a straightforward mapping of a file-based workflow onto your command/response architecture.
