TARGET = voice
BUILD_MODE = release

BIN = target/$(BUILD_MODE)/$(TARGET)

all: $(TARGET)

.PHONY: clean
clean:
	cargo clean
	rm -f $(TARGET)

$(TARGET): $(BIN)
	cp $(BIN) $(TARGET)

$(BIN):
	cargo build --$(BUILD_MODE)

DAEMON = local.personal.transcription

kick: $(TARGET)
	launchctl kickstart -k gui/$(shell id -u)/$(DAEMON)
