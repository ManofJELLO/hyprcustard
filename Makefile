PREFIX ?= /usr/local
DESTDIR ?=
CARGO ?= cargo

BIN := hyprcustard
# Follows the wallpaper for the Colors page; installed beside the app, which
# looks for it there first.
WATCH := custard-watch
QMLLINT ?= qmllint
QML_FILES := $(shell find crates/custard-gui/qml -name '*.qml')

.PHONY: all build release test check install uninstall clean shots aur-bin icons

all: release

build:
	$(CARGO) build

release:
	$(CARGO) build --release

# The engine is where the decisions live, and it needs no display to test.
test:
	$(CARGO) test -p custard-core

# clippy builds the crate, and that build is what writes the QML module the
# linter needs to resolve `import dev.hyprcustard.gui` — so it has to come
# first. qmllint reads what the running engine tolerates but no static tool
# should have to: reserved words, shadowed properties, bindings to nothing.
check:
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings -A clippy::field_reassign_with_default
	$(QMLLINT) -I target/cxxqt/qml_modules $(QML_FILES)

# No `release` prerequisite on purpose: this target is run under sudo, and a
# build as root leaves root-owned artifacts in target/ that the next ordinary
# `cargo build` cannot clean. Build first, as yourself, then install.
install:
	@for b in $(BIN) $(WATCH); do test -x target/release/$$b || { \
		echo "target/release/$$b is missing - run \`make release\` first"; \
		exit 1; }; done
	install -Dm755 target/release/$(BIN) $(DESTDIR)$(PREFIX)/bin/$(BIN)
	install -Dm755 target/release/$(WATCH) $(DESTDIR)$(PREFIX)/bin/$(WATCH)
	install -Dm644 packaging/$(BIN).desktop \
		$(DESTDIR)$(PREFIX)/share/applications/$(BIN).desktop
	install -Dm644 crates/custard-gui/icons/$(BIN).svg \
		$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(BIN).svg
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/$(BIN)/LICENSE
	install -Dm644 README.md $(DESTDIR)$(PREFIX)/share/doc/$(BIN)/README.md

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BIN)
	rm -f $(DESTDIR)$(PREFIX)/bin/$(WATCH)
	rm -f $(DESTDIR)$(PREFIX)/share/applications/$(BIN).desktop
	rm -f $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(BIN).svg
	rm -rf $(DESTDIR)$(PREFIX)/share/licenses/$(BIN)
	rm -rf $(DESTDIR)$(PREFIX)/share/doc/$(BIN)

# The PNGs the app shows its icon with, rendered from the SVGs. Qt reads a
# PNG without the qt6-svg package, which the app does not otherwise need.
# Committed, so building does not need rsvg-convert; run this after
# changing an SVG.
ICONS := crates/custard-gui/icons
icons:
	rsvg-convert -w 64 -h 64 $(ICONS)/hyprcustard-tray.svg -o $(ICONS)/hyprcustard-tray.png
	rsvg-convert -w 128 -h 128 $(ICONS)/hyprcustard.svg -o $(ICONS)/hyprcustard.png

# Render every page to a PNG without a compositor.
shots: build
	@mkdir -p shots
	CUSTARD_SHOT_DIR=$(PWD)/shots QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
		./target/debug/$(BIN) || true

# Make the AUR package ready to push: PKGBUILD and .SRCINFO, written back
# into packaging/aur-bin so the repository holds what the AUR gets.
#
# The checksum can only be filled in once the release it names exists, so
# this downloads the published artifact and hashes it rather than trusting
# anything local. `make aur-bin VERSION=0.2.0` packages that release; with no
# VERSION it packages whatever the PKGBUILD already says.
aur-bin:
	@command -v updpkgsums >/dev/null 2>&1 || { \
		echo 'updpkgsums is missing - install pacman-contrib'; exit 1; }
	rm -rf target/aur-bin
	mkdir -p target/aur-bin
	cp packaging/aur-bin/PKGBUILD target/aur-bin/PKGBUILD
	@test -z "$(VERSION)" || sed -i 's/^pkgver=.*/pkgver=$(VERSION)/' target/aur-bin/PKGBUILD
	cd target/aur-bin && updpkgsums && makepkg --printsrcinfo > .SRCINFO
	@! command -v namcap >/dev/null 2>&1 || namcap target/aur-bin/PKGBUILD
	cp target/aur-bin/PKGBUILD target/aur-bin/.SRCINFO packaging/aur-bin/
	@echo
	@echo 'packaging/aur-bin holds the PKGBUILD and .SRCINFO, checksummed against'
	@echo 'the release. Commit them, then to publish:'
	@echo '  git clone ssh://aur@aur.archlinux.org/hyprcustard-bin.git'
	@echo '  cp packaging/aur-bin/PKGBUILD packaging/aur-bin/.SRCINFO hyprcustard-bin/'
	@echo '  cd hyprcustard-bin && git add PKGBUILD .SRCINFO && git commit -m "..." && git push'

clean:
	$(CARGO) clean
