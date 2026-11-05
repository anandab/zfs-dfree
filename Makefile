CARGO    ?= cargo
PREFIX   ?= /usr/local
BINDIR   ?= $(PREFIX)/bin
TARGET   := zfs-dfree

.PHONY: all build install clean test

all: build

build:
	$(CARGO) build --release

install: build
	sudo install -m 0755 target/release/$(TARGET) $(BINDIR)/$(TARGET)

test:
	$(CARGO) test

clean:
	$(CARGO) clean
