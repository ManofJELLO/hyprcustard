PREFIX ?= /usr/local
DESTDIR ?=
CARGO ?= cargo

BIN := hyprcustard
QMLLINT ?= qmllint
QML_FILES := $(shell find crates/custard-gui/qml -name '*.qml')

.PHONY: all build release test check install uninstall clean shots aur-bin

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
	@test -x target/release/$(BIN) || { \
		echo 'target/release/$(BIN) is missing - run `make release` first'; \
		exit 1; }
	install -Dm755 target/release/$(BIN) $(DESTDIR)$(PREFIX)/bin/$(BIN)
	install -Dm644 packaging/$(BIN).desktop \
		$(DESTDIR)$(PREFIX)/share/applications/$(BIN).desktop
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/$(BIN)/LICENSE
	install -Dm644 README.md $(DESTDIR)$(PREFIX)/share/doc/$(BIN)/README.md

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BIN)
	rm -f $(DESTDIR)$(PREFIX)/share/applications/$(BIN).desktop
	rm -rf $(DESTDIR)$(PREFIX)/share/licenses/$(BIN)
	rm -rf $(DESTDIR)$(PREFIX)/share/doc/$(BIN)

# Render every page to a PNG without a compositor.
shots: build
	@mkdir -p shots
	CUSTARD_SHOT_DIR=$(PWD)/shots QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
		./target/debug/$(BIN) || true

# Stage the AUR package, ready to copy into the AUR clone and push.
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
	@echo
	@echo 'target/aur-bin holds PKGBUILD and .SRCINFO. To publish:'
	@echo '  git clone ssh://aur@aur.archlinux.org/hyprcustard-bin.git'
	@echo '  cp target/aur-bin/PKGBUILD target/aur-bin/.SRCINFO <clone>/'
	@echo '  cd <clone> && git commit -am "..." && git push'

clean:
	$(CARGO) clean
