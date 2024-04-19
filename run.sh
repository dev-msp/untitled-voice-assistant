build_type=release

# As of this writing, whisper.cpp's Metal support depends on the presence of
# this environment variable at runtime. So until it's patched to be more
# flexible in its configuration, this is how we enable Metal.
export GGML_METAL_PATH_RESOURCES="$(find "target/$build_type" -type d -name 'whisper.cpp' -print -quit)"

if [[ -z "$GGML_METAL_PATH_RESOURCES" ]]; then
    echo "GGML_METAL_PATH_RESOURCES not found, try running cargo build"
    exit 1
fi

# Additionally, whisper-rs doesn't capture Metal-specific logs for some reason.
# So you'll still see those in the log output, even if you're suppressing.
export RUST_LOG=whisper_sys_log=error,voice=debug

# See the whisper.cpp repo for details on how to get a model. I recommend using
# base or small for best results.
model_path="$2"

# The address on which the HTTP server should listen (e.g. localhost:PORT)
addr="$3"

./voice run-daemon \
    --serve       "$addr"
    --model       "$model_path"
