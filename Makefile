BIN_NAME = server
BUILD_MODE = release

TARGET = $(BIN_NAME)_$(BUILD_MODE)

BIN = target/$(BUILD_MODE)/$(BIN_NAME)

all: $(TARGET)

.PHONY: clean
clean:
	cargo clean
	rm -f $(TARGET)

$(TARGET): $(BIN)
	cp $(BIN) $(TARGET)

$(BIN):
	cargo build --$(BUILD_MODE) -p $(BIN_NAME)

DAEMON = local.personal.transcription

kick: $(TARGET)
	launchctl kickstart -k gui/$(shell id -u)/$(DAEMON)
