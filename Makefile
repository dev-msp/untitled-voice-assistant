TARGET = voice
BUILD_MODE = release

BIN = target/$(BUILD_MODE)/$(TARGET)

.PHONY: clean
clean:
	cargo clean
	rm -f $(TARGET)

all: $(TARGET)

$(TARGET): $(BIN)
	cp $(BIN) $(TARGET)

$(BIN):
	cargo build --$(BUILD_MODE)
