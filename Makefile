CARGO ?= cargo
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
APP_NAME := wallbrowse

.PHONY: all build run check fmt clippy release install uninstall clean

all: build

build:
	$(CARGO) build

run:
	$(CARGO) run

check:
	$(CARGO) check

fmt:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

release:
	$(CARGO) build --release

install: release
	install -Dm755 target/release/$(APP_NAME) "$(DESTDIR)$(BINDIR)/$(APP_NAME)"
	install -Dm644 assets/wallbrowse.png "$(DESTDIR)$(DATADIR)/icons/hicolor/256x256/apps/$(APP_NAME).png"
	install -Dm644 assets/wallbrowse.svg "$(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APP_NAME).svg"
	install -Dm644 packaging/$(APP_NAME).desktop "$(DESTDIR)$(DATADIR)/applications/$(APP_NAME).desktop"
	@if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$(DESTDIR)$(DATADIR)/applications"; fi

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/$(APP_NAME)"
	rm -f "$(DESTDIR)$(DATADIR)/applications/$(APP_NAME).desktop"
	rm -f "$(DESTDIR)$(DATADIR)/icons/hicolor/256x256/apps/$(APP_NAME).png"
	rm -f "$(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APP_NAME).svg"
	@if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$(DESTDIR)$(DATADIR)/applications"; fi

clean:
	$(CARGO) clean
