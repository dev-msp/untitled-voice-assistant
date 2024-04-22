BUILD_MODE:=release
PACKAGE=server client
BINARIES=$(addprefix target/$(BUILD_MODE)/, $(PACKAGE))
INSTALLED_BINARIES=$(addprefix $(INSTALL_DIR)/voice-, $(PACKAGE))

all: $(BINARIES)

.PHONY: require-bin
require-bin:
ifndef INSTALL_DIR
	$(error INSTALL_DIR is not set)
endif

.PHONY: install
install: require-bin $(INSTALLED_BINARIES)

.PHONY: clean
clean: clean-bin
	cargo clean

.PHONY: clean-bin
clean-bin:
	rm -f $(BINARIES)
ifdef $(INSTALL_DIR)
	rm -f $(INSTALLED_BINARIES)
endif

$(INSTALL_DIR)/voice-%: target/$(BUILD_MODE)/%
	cp $< $@

ifeq ($(words $(PACKAGE)),1)
target/$(BUILD_MODE)/%:
else
$(BINARIES):
endif
ifeq ($(BUILD_MODE),release)
	cargo build --$(BUILD_MODE) $(foreach p, $(PACKAGE),-p $p)
else
	cargo build $(foreach p, $(PACKAGE),-p $p)
endif

DAEMON:=local.personal.transcription

kick: require-bin $(INSTALL_DIR)/voice-server
	launchctl kickstart -k gui/$(shell id -u)/$(DAEMON)
